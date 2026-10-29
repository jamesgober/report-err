//! `Display` and `Debug` for `Error` (PLAN 7.8): `{}` the outermost message,
//! `{:#}` the chain on one line, `{:?}` the report, `{:#?}` a structural view.
//! None of them allocates.

use core::fmt::{self, Debug, Display, Write};

use crate::chain::Link;
use crate::error::Error;
use crate::ext::ShowAttachment;
use crate::trace::StepKind;

/// Bytes of a formatted link kept for the de-duplication check. A string the
/// error holds itself (a string context layer, a message root) is compared
/// in place, whatever its length.
const KEEP: usize = 256;

/// The last `KEEP` bytes written to it, and how many were written in all.
struct Tail {
    buf: [u8; KEEP],
    len: usize,
}

impl Tail {
    fn new() -> Tail {
        Tail {
            buf: [0; KEEP],
            len: 0,
        }
    }

    /// The whole text, when it fit.
    fn whole(&self) -> Option<&str> {
        if self.len > KEEP {
            return None;
        }
        self.buf
            .get(..self.len)
            .and_then(|b| core::str::from_utf8(b).ok())
    }

    /// The byte `back` positions before the end (0 is the last byte).
    fn byte_from_end(&self, back: usize) -> Option<u8> {
        if back >= self.len || back >= KEEP {
            return None;
        }
        let at = (self.len - 1 - back) % KEEP;
        self.buf.get(at).copied()
    }

    /// Whether the text ends with `text`.
    fn ends_with(&self, text: &[u8]) -> bool {
        if text.len() > self.len || text.len() > KEEP {
            return false;
        }
        text.iter()
            .rev()
            .enumerate()
            .all(|(back, b)| self.byte_from_end(back) == Some(*b))
    }
}

impl Write for Tail {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut rest = s.as_bytes();
        while !rest.is_empty() {
            let at = self.len % KEEP;
            let room = KEEP - at;
            let (now, later) = rest.split_at(room.min(rest.len()));
            if let Some(dst) = self.buf.get_mut(at..at + now.len()) {
                dst.copy_from_slice(now);
            }
            self.len += now.len();
            rest = later;
        }
        Ok(())
    }
}

/// The text of one link, for the de-duplication check: borrowed when the
/// error holds it as a string, otherwise in one of the two buffers.
#[derive(Clone, Copy)]
enum Text<'a> {
    None,
    Str(&'a str),
    Buf(usize),
}

/// Two buffers, made only when a link has to be formatted, and used in turn
/// so the previous link's text is never overwritten by the current one.
struct Buffers {
    tails: [Option<Tail>; 2],
}

impl Buffers {
    fn new() -> Buffers {
        Buffers {
            tails: [None, None],
        }
    }

    fn tail(&self, slot: usize) -> Option<&Tail> {
        self.tails.get(slot).and_then(Option::as_ref)
    }

    /// The text of `link`, formatted into the buffer `prev` does not use when
    /// the error does not hold it as a string.
    fn capture<'a>(&mut self, link: &Link<'a>, prev: Text<'_>) -> Result<Text<'a>, fmt::Error> {
        if let Some(text) = link.text {
            return Ok(Text::Str(text));
        }
        let slot = usize::from(matches!(prev, Text::Buf(0)));
        let Some(entry) = self.tails.get_mut(slot) else {
            return Ok(Text::None);
        };
        let tail = entry.get_or_insert_with(Tail::new);
        tail.len = 0;
        write!(tail, "{}", link.error)?;
        Ok(Text::Buf(slot))
    }

    /// Whether `cur` repeats what `prev` already said: `prev` is exactly
    /// `cur`, or ends with `": "` followed by exactly `cur`.
    fn repeats(&self, prev: Text<'_>, cur: Text<'_>) -> bool {
        let cur = match cur {
            Text::Str(text) => text,
            Text::Buf(slot) => match self.tail(slot).and_then(Tail::whole) {
                Some(text) => text,
                None => return false,
            },
            Text::None => return false,
        };
        match prev {
            Text::Str(prev) => {
                prev == cur
                    || prev
                        .strip_suffix(cur)
                        .is_some_and(|head| head.ends_with(": "))
            }
            Text::Buf(slot) => self.tail(slot).is_some_and(|prev| {
                prev.ends_with(cur.as_bytes())
                    && (prev.len == cur.len()
                        || (prev.byte_from_end(cur.len()) == Some(b' ')
                            && prev.byte_from_end(cur.len() + 1) == Some(b':')))
            }),
            Text::None => false,
        }
    }

    /// Writes the link's text, from the borrowed string or the buffer when
    /// it fit, else by formatting the link again.
    fn write(&self, out: &mut dyn Write, text: Text<'_>, link: &Link<'_>) -> fmt::Result {
        match text {
            Text::Str(text) => out.write_str(text),
            Text::Buf(slot) => match self.tail(slot).and_then(Tail::whole) {
                Some(text) => out.write_str(text),
                None => write!(out, "{}", link.error),
            },
            Text::None => write!(out, "{}", link.error),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write_chain(self, f)
        } else {
            write!(f, "{}", &**self)
        }
    }
}

/// `{:#}`: every link on one line, joined by `": "`, repeats skipped.
fn write_chain(error: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut buffers = Buffers::new();
    let mut prev = Text::None;
    let mut first = true;
    let mut chain = error.chain();
    while let Some(link) = chain.next_link() {
        let cur = buffers.capture(&link, prev)?;
        if !first && buffers.repeats(prev, cur) {
            prev = cur;
            continue;
        }
        if !first {
            f.write_str(": ")?;
        }
        buffers.write(f, cur, &link)?;
        first = false;
        prev = cur;
    }
    if chain.truncated() {
        f.write_str(": (chain truncated)")?;
    }
    Ok(())
}

impl Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write_structure(self, f)
        } else {
            write_report(self, f)
        }
    }
}

/// `{:?}`: the report (anyhow's layout first, then report-err's blocks).
fn write_report(error: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    report(error, f, false)
}

/// Writes the report to `out`. With `escape`, the form `ExitReport` prints
/// to a terminal: control characters in any message become `\u{..}` text
/// and a message's continuation lines are indented, so no message can move
/// the cursor or start a line that looks like a block of the report (T6).
pub(crate) fn report(error: &Error, out: &mut dyn Write, escape: bool) -> fmt::Result {
    // First pass: which causes repeat the link before them (bit n: cause n,
    // the link after the outermost one, is skipped). The chain holds at most
    // 128 links, so 127 causes.
    let mut skip: u128 = 0;
    let mut printed = 0usize;
    {
        let mut buffers = Buffers::new();
        let mut prev = Text::None;
        let mut chain = error.chain();
        let mut n = 0usize;
        while let Some(link) = chain.next_link() {
            let cur = buffers.capture(&link, prev)?;
            if n > 0 {
                if buffers.repeats(prev, cur) {
                    skip |= 1u128 << ((n - 1) % 128);
                } else {
                    printed += 1;
                }
            }
            prev = cur;
            n += 1;
        }
    }

    let mut chain = error.chain();
    if let Some(outer) = chain.next_link() {
        message(out, escape, "    ", &LinkText(&outer))?;
    }
    if printed > 0 {
        out.write_str("\n\nCaused by:")?;
        let mut number = 0usize;
        let mut n = 0usize;
        while let Some(link) = chain.next_link() {
            let skipped = skip & (1u128 << (n % 128)) != 0;
            n += 1;
            if skipped {
                continue;
            }
            out.write_str("\n")?;
            let mut indented = Indented {
                inner: out,
                number: (printed > 1).then_some(number),
                started: false,
            };
            message(&mut indented, escape, "", &LinkText(&link))?;
            number += 1;
        }
    } else {
        while chain.next_link().is_some() {}
    }
    if chain.truncated() {
        out.write_str("\n    (chain truncated)")?;
    }

    let extras = error.raw.extras();
    if let Some(extras) = extras.filter(|e| !e.sections.is_empty()) {
        out.write_str("\n")?;
        for section in &extras.sections {
            write!(out, "\n{}: ", section.kind.label())?;
            message(out, escape, "    ", &section.text)?;
        }
    }

    write!(out, "\n\nLocation: {}", error.location())?;

    let origin = error.location();
    if error.trail().any(|step| step.location != origin) {
        out.write_str("\nTrace:")?;
        for (n, step) in error.trail().enumerate() {
            write!(out, "\n{n: >5}: {}  ", step.location)?;
            match (step.kind, step.context) {
                (StepKind::Origin, _) => out.write_str("(origin)")?,
                (StepKind::Context, Some(layer)) => {
                    message(out, escape, "         ", &format_args!("{layer}"))?
                }
                _ => out.write_str("(trace)")?,
            }
        }
    }

    if let Some(extras) = extras {
        let mut shown = extras
            .attachments
            .iter()
            .filter(|a| a.display.is_some())
            .peekable();
        if shown.peek().is_some() {
            out.write_str("\nAttachments:")?;
            for attachment in shown {
                out.write_str("\n    ")?;
                message(out, escape, "    ", &ShowAttachment(attachment))?;
            }
        }
    }

    let omitted = Omitted::of(error);
    if omitted.any() {
        write!(out, "\nOmitted: {omitted}")?;
    }

    #[cfg(feature = "std")]
    if let Some(backtrace) = error.backtrace() {
        write!(out, "\n\nBacktrace:\n{backtrace}")?;
    }
    Ok(())
}

/// A link's text: the string the error holds, or the link's `Display`.
struct LinkText<'a, 'b>(&'a Link<'b>);

impl Display for LinkText<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.text {
            Some(text) => f.write_str(text),
            None => write!(f, "{}", self.0.error),
        }
    }
}

/// Writes one message, escaped when the report goes to a terminal.
fn message(
    out: &mut dyn Write,
    escape: bool,
    indent: &'static str,
    text: &dyn Display,
) -> fmt::Result {
    if escape {
        write!(Escaped { inner: out, indent }, "{text}")
    } else {
        write!(out, "{text}")
    }
}

/// Escapes terminal control characters (C0 except newline and tab, DEL, C1)
/// and Unicode bidirectional controls as `\u{..}` text, and indents
/// each continuation line by `indent`.
struct Escaped<'a> {
    inner: &'a mut dyn Write,
    indent: &'static str,
}

impl Write for Escaped<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            match c {
                '\n' => {
                    self.inner.write_char('\n')?;
                    self.inner.write_str(self.indent)?;
                }
                '\t' => self.inner.write_char('\t')?,
                c if c.is_control() || is_bidi_control(c) => {
                    write!(self.inner, "\\u{{{:x}}}", u32::from(c))?
                }
                c => self.inner.write_char(c)?,
            }
        }
        Ok(())
    }
}

/// The bidirectional embedding, override, and isolate controls, which make a
/// terminal show the rest of a line in another order (PLAN 19, 0.4).
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// What the budgets dropped, for the `Omitted:` line.
struct Omitted {
    frames: u32,
    attachments: u32,
    sections: u32,
    points: u32,
    texts: u32,
}

impl Omitted {
    fn of(error: &Error) -> Omitted {
        let extras = error.raw.extras();
        Omitted {
            frames: error.omitted_frames(),
            attachments: extras.map_or(0, |e| e.omitted.attachments),
            sections: extras.map_or(0, |e| e.omitted.sections),
            points: extras.map_or(0, |e| e.trace.dropped()),
            texts: extras.map_or(0, |e| e.omitted.truncated_text),
        }
    }

    fn any(&self) -> bool {
        self.frames > 0
            || self.attachments > 0
            || self.sections > 0
            || self.points > 0
            || self.texts > 0
    }
}

impl Display for Omitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts = [
            (self.frames, "context layer", "context layers"),
            (self.attachments, "attachment", "attachments"),
            (self.sections, "section", "sections"),
            (self.points, "trace point", "trace points"),
            (self.texts, "text cut", "texts cut"),
        ];
        let mut first = true;
        for (count, one, many) in parts {
            if count == 0 {
                continue;
            }
            if !first {
                f.write_str(", ")?;
            }
            first = false;
            write!(f, "{count} {}", if count == 1 { one } else { many })?;
        }
        f.write_str(" (budget)")
    }
}

/// `{:#?}`: the parts of the error, for developers.
fn write_structure(error: &Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut s = f.debug_struct("Error");
    s.field("message", &Quoted(&**error));
    s.field("location", &format_args!("{}", error.location()));
    s.field("layers", &Layers(error));
    s.field("root", &Root(error));
    if error.omitted_frames() > 0 {
        s.field("omitted", &error.omitted_frames());
    }
    s.finish()
}

struct Layers<'a>(&'a Error);

impl Debug for Layers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = &self.0.raw;
        f.debug_list()
            .entries(
                (0..raw.frames())
                    .rev()
                    .filter_map(|i| raw.frame(i))
                    .map(Quoted),
            )
            .finish()
    }
}

struct Root<'a>(&'a Error);

impl Debug for Root<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.0.raw.root(), f)
    }
}

/// Writes a value's `Display` text as a quoted, escaped string.
pub(crate) struct Quoted<T>(pub(crate) T);

impl<T: Display> Debug for Quoted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        write!(Escape { inner: f }, "{}", self.0)?;
        f.write_char('"')
    }
}

struct Escape<'a, 'b> {
    inner: &'a mut fmt::Formatter<'b>,
}

impl Write for Escape<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            // As `str`'s `Debug`: a single quote is not escaped inside "...".
            if c == '\'' {
                self.inner.write_char(c)?;
                continue;
            }
            for e in c.escape_debug() {
                self.inner.write_char(e)?;
            }
        }
        Ok(())
    }
}

/// Indents every line of a cause under `Caused by:`, numbered when there is
/// more than one (anyhow's layout).
struct Indented<'a> {
    inner: &'a mut dyn Write,
    number: Option<usize>,
    started: bool,
}

impl Write for Indented<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for (i, line) in s.split('\n').enumerate() {
            if !self.started {
                self.started = true;
                match self.number {
                    Some(number) => write!(self.inner, "{number: >5}: ")?,
                    None => self.inner.write_str("    ")?,
                }
            } else if i > 0 {
                self.inner.write_char('\n')?;
                if self.number.is_some() {
                    self.inner.write_str("       ")?;
                } else {
                    self.inner.write_str("    ")?;
                }
            }
            self.inner.write_str(line)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::String;

    use super::*;

    #[derive(Debug)]
    struct Io;
    impl Display for Io {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("No such file or directory (os error 2)")
        }
    }
    impl core::error::Error for Io {}

    #[derive(Debug)]
    struct Repeats(Io);
    impl Display for Repeats {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "io error: {}", self.0)
        }
    }
    impl core::error::Error for Repeats {
        fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn test_format_display_outer_only_and_alternate_chain() {
        let error = Error::new(Io)
            .context("reading config")
            .context("starting server");
        assert_eq!(format!("{error}"), "starting server");
        assert_eq!(
            format!("{error:#}"),
            "starting server: reading config: No such file or directory (os error 2)"
        );
    }

    #[test]
    fn test_format_dedup_exact_suffix_and_identical() {
        let error = Error::new(Repeats(Io));
        assert_eq!(
            format!("{error:#}"),
            "io error: No such file or directory (os error 2)"
        );
        let error = Error::msg("same").context("same");
        assert_eq!(format!("{error:#}"), "same");
    }

    #[test]
    fn test_format_no_dedup_on_prefix_match() {
        let error = Error::msg("disk").context("disk full");
        assert_eq!(format!("{error:#}"), "disk full: disk");
        let error = Error::msg("full").context("disk full");
        assert_eq!(format!("{error:#}"), "disk full: full");
    }

    /// The report up to and including its `Location:` line.
    fn head(report: &str) -> &str {
        let at = report
            .find(
                "

Location: ",
            )
            .map_or(report.len(), |i| {
                i + report[i + 2..]
                    .find('\n')
                    .map_or(report.len() - i, |j| j + 2)
            });
        &report[..at]
    }

    #[test]
    fn test_format_report_layout() {
        let error = Error::new(Io)
            .context("reading config")
            .context("starting server");
        let report = format!("{error:?}");
        let want = format!(
            "starting server

Caused by:
    0: reading config
    1: No such file or directory (os error 2)

Location: {}",
            error.location()
        );
        assert_eq!(head(&report), want);
        // The layers were added at other columns than the origin, so the
        // trail is printed.
        assert!(
            report.contains(
                "
Trace:
    0: "
            ),
            "{report}"
        );
        assert!(
            report.contains(
                "  (origin)
    1: "
            ),
            "{report}"
        );
        assert!(report.ends_with("  starting server"), "{report}");

        let error = Error::new(Io).context("reading config");
        let want = format!(
            "reading config

Caused by:
    No such file or directory (os error 2)

Location: {}",
            error.location()
        );
        assert_eq!(head(&format!("{error:?}")), want);

        let error = Error::msg("alone");
        assert_eq!(
            format!("{error:?}"),
            format!(
                "alone

Location: {}",
                error.location()
            )
        );
    }

    #[test]
    fn test_format_report_trace_only_when_the_error_travelled() {
        fn at_one_call() -> crate::Result<()> {
            let r: core::result::Result<(), Io> = Err(Io);
            crate::Context::context(r, "reading")
        }
        let error = at_one_call().unwrap_err();
        assert!(
            !format!("{error:?}").contains("Trace:"),
            "origin and layer share one location"
        );
    }

    #[test]
    fn test_format_report_skips_repeats_and_renumbers() {
        let error = Error::new(Repeats(Io)).context("loading");
        let want = format!(
            "loading

Caused by:
    io error: No such file or directory (os error 2)

Location: {}",
            error.location()
        );
        assert_eq!(head(&format!("{error:?}")), want);
    }

    #[test]
    fn test_format_report_indents_multi_line_causes() {
        let error = Error::msg("line one\nline two").context("a").context("b");
        let report = format!("{error:?}");
        assert!(
            report.contains("    0: a\n    1: line one\n       line two"),
            "{report}"
        );
    }

    #[test]
    fn test_format_long_text_is_written_in_full() {
        let long: String = "x".repeat(3 * KEEP);
        let error = Error::msg(long.clone()).context("outer");
        assert_eq!(format!("{error:#}"), format!("outer: {long}"));
        // Strings the error holds are compared in place at any length.
        let error = Error::msg(long.clone()).context(long.clone());
        assert_eq!(format!("{error:#}"), long);
        // A formatted link longer than the buffer cannot be compared, so it is
        // written out in full.
        #[derive(Debug)]
        struct Long(String);
        impl Display for Long {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl core::error::Error for Long {}
        let error = Error::new(Long(long.clone())).context(long.clone());
        assert_eq!(format!("{error:#}"), format!("{long}: {long}"));
    }

    #[test]
    fn test_format_flags_are_ignored_except_alternate() {
        let error = Error::msg("x").context("ab");
        assert_eq!(format!("{error:>10}"), "ab");
        assert_eq!(format!("{error:>10.1}"), "ab");
    }

    #[test]
    fn test_format_structure() {
        let error = Error::msg("root \"q\"").context("a");
        let text = format!("{error:#?}");
        assert!(
            text.starts_with("Error {\n    message: \"a\",\n    location: "),
            "{text}"
        );
        assert!(text.contains("layers: [\n        \"a\",\n    ],"), "{text}");
        assert!(text.contains("root: \"root \\\"q\\\"\""), "{text}");
    }

    #[test]
    fn test_tail_keeps_last_bytes() {
        let mut tail = Tail::new();
        for _ in 0..3 {
            tail.write_str(&"a".repeat(KEEP - 3)).ok();
        }
        tail.write_str(": end").ok();
        assert!(tail.whole().is_none());
        assert!(tail.ends_with(b"end"));
        let mut buffers = Buffers::new();
        buffers.tails[0] = Some(tail);
        assert!(buffers.repeats(Text::Buf(0), Text::Str("end")));
        assert!(!buffers.repeats(Text::Buf(0), Text::Str("nd")));
        assert!(buffers.repeats(Text::Str("x: end"), Text::Str("end")));
        assert!(buffers.repeats(Text::Str("end"), Text::Str("end")));
        assert!(!buffers.repeats(Text::Str("xend"), Text::Str("end")));
    }
}

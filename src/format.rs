//! `Display` and `Debug` for `Error` (PLAN 7.8): `{}` the outermost message,
//! `{:#}` the chain on one line, `{:?}` the report, `{:#?}` a structural view.
//! None of them allocates.

use core::fmt::{self, Debug, Display, Write};

use crate::chain::Link;
use crate::error::Error;

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
        match outer.text {
            Some(text) => f.write_str(text)?,
            None => write!(f, "{}", outer.error)?,
        }
    }
    if printed > 0 {
        f.write_str("\n\nCaused by:")?;
        let mut number = 0usize;
        let mut n = 0usize;
        while let Some(link) = chain.next_link() {
            let skipped = skip & (1u128 << (n % 128)) != 0;
            n += 1;
            if skipped {
                continue;
            }
            f.write_str("\n")?;
            let mut out = Indented {
                inner: f,
                number: (printed > 1).then_some(number),
                started: false,
            };
            match link.text {
                Some(text) => out.write_str(text)?,
                None => write!(out, "{}", link.error)?,
            }
            number += 1;
        }
    } else {
        while chain.next_link().is_some() {}
    }
    if chain.truncated() {
        f.write_str("\n    (chain truncated)")?;
    }
    write!(f, "\n\nLocation: {}", error.location())?;
    let omitted = error.omitted_frames();
    if omitted > 0 {
        let noun = if omitted == 1 { "layer" } else { "layers" };
        write!(f, "\nOmitted: {omitted} context {noun} (frame budget)")?;
    }
    Ok(())
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
struct Indented<'a, 'b> {
    inner: &'a mut fmt::Formatter<'b>,
    number: Option<usize>,
    started: bool,
}

impl Write for Indented<'_, '_> {
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

    #[test]
    fn test_format_report_layout() {
        let error = Error::new(Io)
            .context("reading config")
            .context("starting server");
        let report = format!("{error:?}");
        let want = format!(
            "starting server\n\nCaused by:\n    0: reading config\n    1: No such file or directory (os error 2)\n\nLocation: {}",
            error.location()
        );
        assert_eq!(report, want);

        let error = Error::new(Io).context("reading config");
        let want = format!(
            "reading config\n\nCaused by:\n    No such file or directory (os error 2)\n\nLocation: {}",
            error.location()
        );
        assert_eq!(format!("{error:?}"), want);

        let error = Error::msg("alone");
        assert_eq!(
            format!("{error:?}"),
            format!("alone\n\nLocation: {}", error.location())
        );
    }

    #[test]
    fn test_format_report_skips_repeats_and_renumbers() {
        let error = Error::new(Repeats(Io)).context("loading");
        let want = format!(
            "loading\n\nCaused by:\n    io error: No such file or directory (os error 2)\n\nLocation: {}",
            error.location()
        );
        assert_eq!(format!("{error:?}"), want);
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

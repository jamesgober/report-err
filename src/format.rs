//! `Display` and `Debug` for `Error` (PLAN 7.8): `{}` the outermost message,
//! `{:#}` the chain on one line, `{:?}` the report, `{:#?}` a structural view.
//! None of them allocates.

use core::fmt::{self, Debug, Display, Write};
use core::panic::Location;

use crate::chain::Link;
use crate::error::Error;
use crate::ext::ShowAttachment;
use crate::limits::MAX_DEPTH;
use crate::trace::StepKind;

/// How a rendering treats the user's `Display` impls and the terminal.
#[derive(Clone, Copy)]
pub(crate) struct Mode {
    /// Control characters escaped and continuation lines indented (T6).
    pub(crate) escape: bool,
    /// Each `Display` guarded: a panic prints `<display panicked>` and a
    /// failure `<display failed>`, and the rendering goes on.
    pub(crate) guard: bool,
}

impl Mode {
    /// `{:?}`: as written, not guarded.
    pub(crate) const PLAIN: Mode = Mode {
        escape: false,
        guard: false,
    };
    /// `render`, `record`, `report`: guarded.
    pub(crate) const GUARDED: Mode = Mode {
        escape: false,
        guard: true,
    };
    /// `ExitReport` and `unraisable`: guarded and escaped for a terminal.
    #[cfg_attr(not(feature = "std"), allow(dead_code))]
    pub(crate) const TERMINAL: Mode = Mode {
        escape: true,
        guard: true,
    };
}

/// An error as a `Display`, without trait upcasting (not on 1.81).
pub(crate) struct ErrorText<'a>(pub(crate) &'a (dyn core::error::Error + 'static));

impl Display for ErrorText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self.0, f)
    }
}

/// A user's `Display`, written as is or guarded (`Mode::guard`).
pub(crate) struct Shown<'a> {
    d: &'a dyn Display,
    guard: bool,
}

impl<'a> Shown<'a> {
    pub(crate) fn new(d: &'a dyn Display, guard: bool) -> Shown<'a> {
        Shown { d, guard }
    }

    pub(crate) fn guarded(d: &'a dyn Display) -> Shown<'a> {
        Shown { d, guard: true }
    }
}

impl Display for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.guard {
            return self.d.fmt(f);
        }
        let mut watch = Watch {
            inner: f,
            failed: false,
        };
        match guarded_write(&mut watch, self.d) {
            Some(Ok(())) => Ok(()),
            // The writer failed: pass it on.
            Some(Err(_)) if watch.failed => Err(fmt::Error),
            Some(Err(_)) => f.write_str("<display failed>"),
            None => f.write_str("<display panicked>"),
        }
    }
}

/// Writes `d`; `None` when its `Display` panicked (with `std`).
fn guarded_write(out: &mut dyn Write, d: &dyn Display) -> Option<fmt::Result> {
    #[cfg(feature = "std")]
    {
        let call = std::panic::AssertUnwindSafe(|| write!(out, "{d}"));
        std::panic::catch_unwind(call)
            .map_err(crate::panic::drop_payload)
            .ok()
    }
    #[cfg(not(feature = "std"))]
    {
        Some(write!(out, "{d}"))
    }
}

/// Remembers whether the writer itself failed, to tell that apart from a
/// `Display` that returned an error.
struct Watch<'a> {
    inner: &'a mut dyn Write,
    failed: bool,
}

impl Write for Watch<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let result = self.inner.write_str(s);
        self.failed |= result.is_err();
        result
    }
}

/// Bytes of a formatted link kept for the de-duplication check. A string the
/// error holds itself (a string context layer, a message root) is compared
/// in place, and the last link after a string as it is formatted, whatever
/// their length.
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
    /// The last link, compared with the link before it while it was
    /// formatted and not kept; whether it repeats it.
    Judged(bool),
}

/// Where a repeat of the previous link could start in it: at 0, and after
/// each `": "`. More than this many, and the link is buffered instead.
const STARTS: usize = 8;

/// Compares a link's text, as it is formatted, with each way it could repeat
/// `prev` (all of it, or what follows one of its `": "`), and stops the
/// formatting at the first write that rules every way out. No buffer.
struct Matcher<'p> {
    prev: &'p [u8],
    starts: [usize; STARTS],
    /// Bit `i` set while `starts[i]` still matches.
    alive: u8,
    written: usize,
}

impl<'p> Matcher<'p> {
    fn new(prev: &'p str) -> Option<Matcher<'p>> {
        let mut matcher = Matcher {
            prev: prev.as_bytes(),
            starts: [0; STARTS],
            alive: 1,
            written: 0,
        };
        // A byte scan: `str::match_indices` sets up a general searcher, which
        // costs more than the scan for these short texts.
        let mut n = 0;
        for (at, pair) in matcher.prev.windows(2).enumerate() {
            if pair == b": " {
                n += 1;
                let slot = matcher.starts.get_mut(n)?;
                *slot = at + 2;
                matcher.alive |= 1 << n;
            }
        }
        Some(matcher)
    }

    /// Whether what was written is exactly one of the ways.
    fn matched(&self) -> bool {
        self.starts
            .iter()
            .enumerate()
            .any(|(i, start)| self.alive & (1 << i) != 0 && start + self.written == self.prev.len())
    }
}

impl Write for Matcher<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let s = s.as_bytes();
        for (i, start) in self.starts.iter().enumerate() {
            let at = start + self.written;
            if self.alive & (1 << i) != 0 && self.prev.get(at..at + s.len()) != Some(s) {
                self.alive &= !(1 << i);
            }
        }
        self.written += s.len();
        if self.alive == 0 {
            // Not a repeat whatever comes next: stop the formatting.
            return Err(fmt::Error);
        }
        Ok(())
    }
}

/// Two buffers, made only when a link has to be formatted, and used in turn
/// so the previous link's text is never overwritten by the current one.
struct Buffers {
    tails: [Option<Tail>; 2],
    guard: bool,
}

impl Buffers {
    fn new(mode: Mode) -> Buffers {
        Buffers {
            tails: [None, None],
            guard: mode.guard,
        }
    }

    fn tail(&self, slot: usize) -> Option<&Tail> {
        self.tails.get(slot).and_then(Option::as_ref)
    }

    /// The text of `link`, formatted into the buffer `prev` does not use when
    /// the error does not hold it as a string. The last link (`last`) is
    /// only compared, since no link after it needs its text: with nothing
    /// before it not at all, after a string without a buffer.
    fn capture<'a>(
        &mut self,
        link: &Link<'a>,
        prev: Text<'_>,
        last: bool,
    ) -> Result<Text<'a>, fmt::Error> {
        if let Some(text) = link.text {
            return Ok(Text::Str(text));
        }
        if last {
            match prev {
                Text::None => return Ok(Text::Judged(false)),
                Text::Str(prev) => {
                    if let Some(mut matcher) = Matcher::new(prev) {
                        let text = ErrorText(link.error);
                        let shown = Shown::new(&text, self.guard);
                        return match write!(matcher, "{shown}") {
                            Ok(()) => Ok(Text::Judged(matcher.matched())),
                            // Stopped by the matcher: not a repeat.
                            Err(_) if matcher.alive == 0 => Ok(Text::Judged(false)),
                            Err(e) => Err(e),
                        };
                    }
                }
                Text::Buf(_) | Text::Judged(_) => {}
            }
        }
        let slot = usize::from(matches!(prev, Text::Buf(0)));
        let Some(entry) = self.tails.get_mut(slot) else {
            return Ok(Text::None);
        };
        let tail = entry.get_or_insert_with(Tail::new);
        tail.len = 0;
        write!(tail, "{}", Shown::new(&ErrorText(link.error), self.guard))?;
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
            Text::Judged(repeats) => return repeats,
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
            Text::None | Text::Judged(_) => false,
        }
    }

    /// Writes the link's text, from the borrowed string or the buffer when
    /// it fit, else by formatting the link again.
    fn write(&self, out: &mut dyn Write, text: Text<'_>, link: &Link<'_>) -> fmt::Result {
        match text {
            Text::Str(text) => out.write_str(text),
            Text::Buf(slot) => match self.tail(slot).and_then(Tail::whole) {
                Some(text) => out.write_str(text),
                None => write!(out, "{}", Shown::new(&ErrorText(link.error), self.guard)),
            },
            Text::None | Text::Judged(_) => {
                write!(out, "{}", Shown::new(&ErrorText(link.error), self.guard))
            }
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write_chain(self, f, Mode::PLAIN)
        } else {
            write!(f, "{}", &**self)
        }
    }
}

/// `{:#}`: every link on one line, joined by `": "`, repeats skipped.
pub(crate) fn write_chain(error: &Error, f: &mut dyn Write, mode: Mode) -> fmt::Result {
    let mut buffers = Buffers::new(mode);
    let mut prev = Text::None;
    let mut first = true;
    let mut chain = error.chain();
    while let Some(link) = chain.next_link() {
        let cur = buffers.capture(&link, prev, chain.at_end())?;
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
    report(error, f, Mode::PLAIN)
}

/// Cause `n` is skipped as a repeat (`n` below the 4,096 links `max_chain`
/// allows).
struct Skips([u64; 64]);

impl Skips {
    fn set(&mut self, n: usize) {
        if let Some(word) = self.0.get_mut(n / 64) {
            *word |= 1 << (n % 64);
        }
    }

    fn get(&self, n: usize) -> bool {
        self.0
            .get(n / 64)
            .is_some_and(|word| word & (1 << (n % 64)) != 0)
    }
}

/// Writes the report to `out`. With `mode.escape`, the form `ExitReport`
/// prints to a terminal: control characters in any message become `\u{..}`
/// text and a message's continuation lines are indented, so no message can
/// move the cursor or start a line that looks like a block of the report
/// (T6). With `mode.guard`, no `Display` can stop it (see `Shown`).
pub(crate) fn report(error: &Error, out: &mut dyn Write, mode: Mode) -> fmt::Result {
    // First pass: which causes repeat the link before them (cause n is the
    // link after the outermost one).
    let mut skip = Skips([0; 64]);
    let mut printed = 0usize;
    let mut buffers = Buffers::new(mode);
    let mut prev = Text::None;
    let mut links = 0usize;
    {
        let mut chain = error.chain();
        while let Some(link) = chain.next_link() {
            let cur = buffers.capture(&link, prev, chain.at_end())?;
            if links > 0 {
                if buffers.repeats(prev, cur) {
                    skip.set(links - 1);
                } else {
                    printed += 1;
                }
            }
            prev = cur;
            links += 1;
        }
    }
    // The innermost link's text is still in its buffer (usually the root,
    // the one link the error does not hold as a string), so it is written
    // from there, not formatted a second time.
    let kept = match prev {
        Text::Buf(slot) => buffers.tail(slot).and_then(Tail::whole),
        _ => None,
    };
    let last = links.saturating_sub(1);
    let show = |index: usize, link: &Link<'_>, out: &mut dyn Write, indent: &'static str| match kept
        .filter(|_| index == last)
    {
        Some(text) => message(out, mode, indent, &text),
        None => message(out, mode, indent, &LinkText(link)),
    };

    let mut chain = error.chain();
    if let Some(outer) = chain.next_link() {
        show(0, &outer, out, "    ")?;
    }
    if printed > 0 {
        out.write_str("\n\nCaused by:")?;
        let mut number = 0usize;
        let mut n = 0usize;
        while let Some(link) = chain.next_link() {
            let skipped = skip.get(n);
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
            show(n, &link, &mut indented, "")?;
            number += 1;
        }
    } else {
        while chain.next_link().is_some() {}
    }
    if chain.truncated() {
        out.write_str("\n    (chain truncated)")?;
    }

    write_causes(error, out, mode)?;

    let extras = error.raw.extras();
    if let Some(extras) = extras.filter(|e| !e.sections.is_empty()) {
        out.write_str("\n")?;
        for section in &extras.sections {
            write!(out, "\n{}: ", section.kind.label())?;
            message(out, mode, "    ", &section.text)?;
        }
    }

    let suppressed = error.suppressed();
    if !suppressed.is_empty() {
        out.write_str("\n\nSuppressed:")?;
        for other in suppressed {
            out.write_str("\n    ")?;
            one_line(out, mode, "    ", other)?;
        }
    }

    match error.code_line() {
        Some(line) => write!(out, "\n\nCode: {line}\nLocation: ")?,
        None => out.write_str("\n\nLocation: ")?,
    }
    write_location(out, error.location())?;

    let origin = error.location();
    if error.trail().any(|step| step.location != origin) {
        out.write_str("\nTrace:")?;
        for (n, step) in error.trail().enumerate() {
            let mut head = Short::new();
            head.push(b"\n");
            head.number(n, 5);
            head.push(b": ");
            out.write_str(head.text())?;
            write_location(out, step.location)?;
            out.write_str("  ")?;
            match (step.kind, step.context) {
                (StepKind::Origin, _) => out.write_str("(origin)")?,
                (StepKind::Context, Some(layer)) => {
                    message(out, mode, "         ", &format_args!("{layer}"))?
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
                message(out, mode, "    ", &ShowAttachment(attachment))?;
            }
        }
        if !extras.fields.is_empty() {
            out.write_str("\nFields:")?;
            for (key, value) in &extras.fields {
                write!(out, "\n    {key} = ")?;
                message(out, mode, "    ", value)?;
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

/// `file:line:column`, as `Location`'s own `Display` writes it, in two
/// writes and with the numbers made by hand: the report prints one per
/// trace step, and the formatter's integer path was most of the report's
/// time (D-052).
fn write_location(out: &mut dyn Write, at: &Location<'_>) -> fmt::Result {
    out.write_str(at.file())?;
    let mut tail = Short::new();
    tail.push(b":");
    tail.number(at.line() as usize, 0);
    tail.push(b":");
    tail.number(at.column() as usize, 0);
    out.write_str(tail.text())
}

/// A few ASCII bytes built on the stack and written at once.
struct Short {
    buf: [u8; 64],
    len: usize,
}

impl Short {
    fn new() -> Short {
        Short {
            buf: [0; 64],
            len: 0,
        }
    }

    /// Appends ASCII `bytes`; what does not fit is dropped.
    fn push(&mut self, bytes: &[u8]) {
        let end = (self.len + bytes.len()).min(self.buf.len());
        let src = bytes.get(..end - self.len).unwrap_or(&[]);
        if let Some(dst) = self.buf.get_mut(self.len..end) {
            dst.copy_from_slice(src);
        }
        self.len = end;
    }

    /// Appends `n` in decimal, right-aligned in `width` columns (at most 20).
    fn number(&mut self, mut n: usize, width: usize) {
        let mut digits = [b' '; 20];
        let mut start = digits.len();
        loop {
            start -= 1;
            if let Some(slot) = digits.get_mut(start) {
                // `n % 10` is below 10, so this is an ASCII digit.
                *slot = b'0' + (n % 10) as u8;
            }
            n /= 10;
            if n == 0 || start == 0 {
                break;
            }
        }
        let start = start.min(digits.len().saturating_sub(width));
        self.push(digits.get(start..).unwrap_or(&[]));
    }

    fn text(&self) -> &str {
        // Only ASCII is pushed, so this never fails.
        core::str::from_utf8(self.buf.get(..self.len).unwrap_or(&[])).unwrap_or("")
    }
}

/// Which form [`Error::render`] writes.
///
/// ```
/// use report_err::RenderStyle;
///
/// let error = report_err::err!("x").context("y");
/// let mut text = String::new();
/// error.render(&mut text, RenderStyle::Chain).unwrap();
/// assert_eq!(text, "y: x");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum RenderStyle {
    /// The outermost message, as `{}`.
    Message,
    /// The chain on one line, as `{:#}`.
    Chain,
    /// The report, as `{:?}`.
    Report,
}

impl Error {
    /// Writes `{}`, `{:#}`, or `{:?}` in a form that cannot fail on the
    /// error's account, for code that must not fail while it prints an error
    /// (a panic hook, a crash path).
    ///
    /// Each `Display` is guarded: one that panics (with `std` and unwinding)
    /// prints `<display panicked>`, one that fails prints
    /// `<display failed>`, and the rest goes on. Output stops at
    /// `max_render` bytes with `(report truncated)`.
    ///
    /// # Errors
    ///
    /// Only when `out` itself fails.
    ///
    /// ```
    /// use std::fmt;
    /// use report_err::{Error, RenderStyle};
    ///
    /// struct Broken;
    /// impl fmt::Display for Broken {
    ///     fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
    ///         Err(fmt::Error)
    ///     }
    /// }
    ///
    /// let error = report_err::err!("root").context(Broken);
    /// let mut text = String::new();
    /// error.render(&mut text, RenderStyle::Chain).unwrap();
    /// assert_eq!(text, "<display failed>: root");
    /// ```
    pub fn render(&self, out: &mut dyn Write, style: RenderStyle) -> fmt::Result {
        let mut capped = Capped {
            inner: out,
            written: 0,
            full: false,
        };
        let mode = Mode::GUARDED;
        let result = match style {
            RenderStyle::Message => write!(capped, "{}", Shown::guarded(&ErrorText(&**self))),
            RenderStyle::Chain => write_chain(self, &mut capped, mode),
            RenderStyle::Report => report(self, &mut capped, mode),
        };
        if capped.full {
            return capped.inner.write_str("\n(report truncated)");
        }
        result
    }
}

/// Writes to a `fmt::Write` and stops quietly at `max_render` bytes.
struct Capped<'a> {
    inner: &'a mut dyn Write,
    written: usize,
    full: bool,
}

impl Write for Capped<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.full {
            return Ok(());
        }
        let room = crate::limits::max_render().saturating_sub(self.written);
        let mut take = s.len().min(room);
        while take > 0 && !s.is_char_boundary(take) {
            take -= 1;
        }
        let part = s.get(..take).unwrap_or("");
        self.inner.write_str(part)?;
        self.written += part.len();
        if take < s.len() {
            self.full = true;
        }
        Ok(())
    }
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

/// Writes one message, escaped when the report goes to a terminal and
/// guarded when the mode says so.
fn message(
    out: &mut dyn Write,
    mode: Mode,
    indent: &'static str,
    text: &dyn Display,
) -> fmt::Result {
    let text = Shown::new(text, mode.guard);
    if mode.escape {
        write!(Escaped { inner: out, indent }, "{text}")
    } else {
        write!(out, "{text}")
    }
}

/// Writes an error's chain on one line (`{:#}`), escaped and guarded as the
/// mode says.
fn one_line(out: &mut dyn Write, mode: Mode, indent: &'static str, error: &Error) -> fmt::Result {
    if mode.escape {
        write_chain(error, &mut Escaped { inner: out, indent }, mode)
    } else {
        write_chain(error, out, mode)
    }
}

/// `{:#}` for a terminal: guarded and escaped (`unraisable`).
#[cfg(feature = "std")]
pub(crate) fn write_chain_terminal(error: &Error, out: &mut dyn Write) -> fmt::Result {
    one_line(out, Mode::TERMINAL, "    ", error)
}

/// Indents for each level of `Causes`.
const LEVELS: &str = "                                                                                                                                                                                                                                                                    ";

/// `Causes (n):`, the children as a tree, each on one line. The walk keeps a
/// stack as deep as `max_depth` allows, never recursion.
fn write_causes(error: &Error, out: &mut dyn Write, mode: Mode) -> fmt::Result {
    let children = error.children();
    if children.is_empty() {
        return Ok(());
    }
    write!(out, "\n\nCauses ({}):", children.len())?;
    let mut stack: [(&[Error], usize); MAX_DEPTH + 1] = [(&[], 0); MAX_DEPTH + 1];
    let mut top = 0usize;
    if let Some(slot) = stack.get_mut(0) {
        *slot = (children, 0);
    }
    loop {
        let Some((level, number)) = stack.get_mut(top) else {
            return Ok(());
        };
        let Some((child, rest)) = level.split_first() else {
            match top.checked_sub(1) {
                Some(up) => top = up,
                None => return Ok(()),
            }
            continue;
        };
        *level = rest;
        let n = *number;
        *number += 1;
        let indent = LEVELS.get(..4 * (top + 1)).unwrap_or(LEVELS);
        write!(out, "\n{indent}{n}: ")?;
        one_line(out, mode, indent, child)?;
        let theirs = child.children();
        if !theirs.is_empty() {
            if let Some(slot) = stack.get_mut(top + 1) {
                *slot = (theirs, 0);
                top += 1;
            }
        }
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
    children: u32,
    suppressed: u32,
    fields: u32,
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
            children: extras.map_or(0, |e| e.omitted.children),
            suppressed: extras.map_or(0, |e| e.omitted.suppressed),
            fields: extras.map_or(0, |e| e.omitted.fields),
        }
    }

    fn any(&self) -> bool {
        self.frames > 0
            || self.attachments > 0
            || self.sections > 0
            || self.points > 0
            || self.texts > 0
            || self.children > 0
            || self.suppressed > 0
            || self.fields > 0
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
            (self.children, "child", "children"),
            (self.suppressed, "suppressed error", "suppressed errors"),
            (self.fields, "field", "fields"),
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
    s.field("code", &CodeField(error.code()));
    s.field("location", &format_args!("{}", error.location()));
    s.field("layers", &Layers(error));
    s.field("root", &Root(error));
    s.field("children", &error.children().len());
    s.field("suppressed", &error.suppressed().len());
    s.field(
        "attachments",
        &error.raw.extras().map_or(0, |e| e.attachments.len()),
    );
    if error.omitted_frames() > 0 {
        s.field("omitted", &error.omitted_frames());
    }
    s.finish()
}

/// `None`, or the code as it displays (`DB-0042`).
struct CodeField(Option<define_err::Code>);

impl Debug for CodeField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(code) => write!(f, "{code}"),
            None => f.write_str("None"),
        }
    }
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
    use alloc::boxed::Box;
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
    fn test_format_dedup_last_link_compared_as_formatted() {
        // After a string layer, the root is compared without a buffer: all of
        // the layer, or what follows any of its `": "`.
        let io = "No such file or directory (os error 2)";
        let error = Error::new(Io).context(format!("a: b: {io}"));
        assert_eq!(format!("{error:#}"), format!("a: b: {io}"));
        let error = Error::new(Io).context(io);
        assert_eq!(format!("{error:#}"), io);
        // A layer that matches the root's start only, or that the root
        // outruns, is not a repeat.
        let error = Error::new(Io).context("No such file");
        assert_eq!(format!("{error:#}"), format!("No such file: {io}"));
        let error = Error::new(Io).context(format!("x: {io} (again)"));
        assert_eq!(format!("{error:#}"), format!("x: {io} (again): {io}"));
        // More `": "` than the matcher tracks: the buffer decides instead.
        let many = format!("{}{io}", "k: ".repeat(STARTS + 2));
        let error = Error::new(Io).context(many.clone());
        assert_eq!(format!("{error:#}"), many);
        // The report skips the same causes.
        let error = Error::new(Io).context(format!("a: {io}"));
        assert!(!format!("{error:?}").contains("Caused by"));
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
        // The last link is compared as it is formatted, at any length.
        #[derive(Debug)]
        struct Long(String, Option<Box<Long>>);
        impl Display for Long {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl core::error::Error for Long {
            fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
                self.1.as_deref().map(|l| l as _)
            }
        }
        let error = Error::new(Long(long.clone(), None)).context(long.clone());
        assert_eq!(format!("{error:#}"), long);
        // A formatted link followed by another is buffered; one longer than
        // the buffer cannot be compared, so the next is written out in full.
        let inner = Long(long.clone(), None);
        let error = Error::new(Long(long.clone(), Some(Box::new(inner))));
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
            text.starts_with("Error {\n    message: \"a\",\n    code: None,\n    location: "),
            "{text}"
        );
        assert!(text.contains("layers: [\n        \"a\",\n    ],"), "{text}");
        assert!(
            text.contains("children: 0,\n    suppressed: 0,\n    attachments: 0,"),
            "{text}"
        );
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
        let mut buffers = Buffers::new(Mode::PLAIN);
        buffers.tails[0] = Some(tail);
        assert!(buffers.repeats(Text::Buf(0), Text::Str("end")));
        assert!(!buffers.repeats(Text::Buf(0), Text::Str("nd")));
        assert!(buffers.repeats(Text::Str("x: end"), Text::Str("end")));
        assert!(buffers.repeats(Text::Str("end"), Text::Str("end")));
        assert!(!buffers.repeats(Text::Str("xend"), Text::Str("end")));
    }
}

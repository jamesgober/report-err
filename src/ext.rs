//! What an error carries besides its context layers: attachments, sections,
//! trace points, an explicit exit code, the backtrace, and the counters of
//! what a budget dropped. Kept in the extension block, which an error
//! allocates the first time it needs any of it (PLAN 7.2).

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::fmt::{self, Display, Write};
use core::panic::Location;

use crate::limits;

/// How a section reads in the report.
///
/// ```
/// use report_err::{Error, SectionKind};
///
/// let error = Error::msg("disk full").suggestion("free some space");
/// let kinds: Vec<SectionKind> = error.sections().map(|s| s.kind).collect();
/// assert_eq!(kinds, [SectionKind::Suggestion]);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum SectionKind {
    /// A fact that helps explain the failure (`Note:`).
    Note,
    /// Something the reader should be careful about (`Warning:`).
    Warning,
    /// What to do about it (`Suggestion:`).
    Suggestion,
}

impl SectionKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SectionKind::Note => "Note",
            SectionKind::Warning => "Warning",
            SectionKind::Suggestion => "Suggestion",
        }
    }
}

pub(crate) struct SectionData {
    pub(crate) kind: SectionKind,
    pub(crate) text: Cow<'static, str>,
}

type FmtFn = fn(&(dyn Any + Send + Sync), &mut fmt::Formatter<'_>) -> fmt::Result;

pub(crate) struct Attachment {
    pub(crate) value: Box<dyn Any + Send + Sync>,
    /// Set by `attach_display`: how to print the value under `Attachments:`.
    pub(crate) display: Option<FmtFn>,
}

impl Attachment {
    pub(crate) fn type_id(&self) -> TypeId {
        (*self.value).type_id()
    }
}

/// Prints an attachment added with `attach_display`.
pub(crate) struct ShowAttachment<'a>(pub(crate) &'a Attachment);

impl Display for ShowAttachment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.display {
            Some(show) => show(&*self.0.value, f),
            None => Ok(()),
        }
    }
}

pub(crate) fn show_as<A: Display + 'static>(
    value: &(dyn Any + Send + Sync),
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match value.downcast_ref::<A>() {
        Some(value) => write!(f, "{value}"),
        None => Ok(()),
    }
}

/// One `.trace()` point: where, and how many context layers had been added
/// before it (counting layers a budget later dropped), which places it among
/// the layers in time.
#[derive(Clone, Copy)]
pub(crate) struct Point {
    pub(crate) location: &'static Location<'static>,
    pub(crate) after: u32,
}

/// Trace points: the first 4 and the latest 11.
pub(crate) const FIRST_POINTS: usize = 4;
pub(crate) const LATEST_POINTS: usize = 11;

#[derive(Default)]
pub(crate) struct Trace {
    first: [Option<Point>; FIRST_POINTS],
    latest: [Option<Point>; LATEST_POINTS],
    /// Points recorded in all.
    recorded: u32,
}

impl Trace {
    pub(crate) fn push(&mut self, point: Point) {
        let n = self.recorded as usize;
        if n < FIRST_POINTS {
            if let Some(slot) = self.first.get_mut(n) {
                *slot = Some(point);
            }
        } else if let Some(slot) = self.latest.get_mut((n - FIRST_POINTS) % LATEST_POINTS) {
            *slot = Some(point);
        }
        self.recorded = self.recorded.saturating_add(1);
    }

    /// Points dropped between the first 4 and the latest 11.
    pub(crate) fn dropped(&self) -> u32 {
        self.recorded
            .saturating_sub((FIRST_POINTS + LATEST_POINTS) as u32)
    }

    /// How many points are kept.
    pub(crate) fn kept(&self) -> usize {
        (self.recorded as usize).min(FIRST_POINTS + LATEST_POINTS)
    }

    /// Kept point `i`, oldest first.
    pub(crate) fn get(&self, i: usize) -> Option<Point> {
        let n = self.recorded as usize;
        if i < FIRST_POINTS {
            return if i < n {
                self.first.get(i).copied().flatten()
            } else {
                None
            };
        }
        if i >= self.kept() {
            return None;
        }
        // The oldest kept slot of the ring.
        let start = if n > FIRST_POINTS + LATEST_POINTS {
            (n - FIRST_POINTS) % LATEST_POINTS
        } else {
            0
        };
        self.latest
            .get((start + i - FIRST_POINTS) % LATEST_POINTS)
            .copied()
            .flatten()
    }

    #[cfg(test)]
    pub(crate) fn points(&self) -> impl Iterator<Item = Point> + '_ {
        (0..self.kept()).filter_map(|i| self.get(i))
    }
}

/// Counts of what a budget dropped.
#[derive(Default, Clone, Copy)]
pub(crate) struct Omitted {
    pub(crate) attachments: u32,
    pub(crate) sections: u32,
    pub(crate) truncated_text: u32,
}

#[derive(Default)]
pub(crate) struct Extras {
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) sections: Vec<SectionData>,
    pub(crate) trace: Trace,
    pub(crate) exit: Option<u8>,
    #[cfg(feature = "std")]
    pub(crate) backtrace: Option<std::backtrace::Backtrace>,
    pub(crate) omitted: Omitted,
    /// Text and value bytes charged against `max_bytes`.
    pub(crate) bytes: usize,
}

impl Extras {
    fn room(&self) -> usize {
        limits::MAX_BYTES.saturating_sub(self.bytes)
    }

    /// Adds a section whose owned text came from
    /// [`format_bounded`](Extras::format_bounded), so it already fits.
    pub(crate) fn add_section(&mut self, kind: SectionKind, text: Cow<'static, str>) {
        if self.sections.len() >= limits::MAX_SECTIONS {
            self.omitted.sections = self.omitted.sections.saturating_add(1);
            return;
        }
        if let Cow::Owned(text) = &text {
            self.bytes = self.bytes.saturating_add(text.len());
        }
        self.sections.push(SectionData { kind, text });
    }

    /// Formats `value` once, within the bytes left: when it does not fit, it
    /// is cut at a character boundary and ends with `...(truncated)`, and the
    /// whole still fits.
    pub(crate) fn format_bounded(&mut self, value: &dyn Display) -> String {
        let room = self.room();
        let mut out = Bounded {
            text: String::new(),
            limit: room,
            cut: false,
        };
        let _ = write!(out, "{value}");
        if out.cut {
            let mut at = room.saturating_sub(TRUNCATED.len()).min(out.text.len());
            while at > 0 && !out.text.is_char_boundary(at) {
                at -= 1;
            }
            out.text.truncate(at);
            if room >= TRUNCATED.len() {
                out.text.push_str(TRUNCATED);
            }
            self.omitted.truncated_text = self.omitted.truncated_text.saturating_add(1);
        }
        out.text
    }

    pub(crate) fn add_attachment(&mut self, attachment: Attachment, size: usize) {
        if self.attachments.len() >= limits::MAX_ATTACHMENTS || size > self.room() {
            self.omitted.attachments = self.omitted.attachments.saturating_add(1);
            return;
        }
        self.bytes = self.bytes.saturating_add(size);
        self.attachments.push(attachment);
    }
}

/// Appended where text was cut at the byte budget.
pub(crate) const TRUNCATED: &str = "...(truncated)";

/// A `String` writer that keeps at most `limit` bytes, cut at a character
/// boundary.
struct Bounded {
    text: String,
    limit: usize,
    cut: bool,
}

impl Write for Bounded {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.cut {
            return Ok(());
        }
        let room = self.limit.saturating_sub(self.text.len());
        if s.len() <= room {
            self.text.push_str(s);
            return Ok(());
        }
        let mut at = room;
        while at > 0 && !s.is_char_boundary(at) {
            at -= 1;
        }
        self.text.push_str(s.get(..at).unwrap_or(""));
        self.cut = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec::Vec;

    use super::*;

    fn at() -> &'static Location<'static> {
        Location::caller()
    }

    #[test]
    fn test_trace_keeps_first_four_and_latest_eleven() {
        for count in [0u32, 4, 5, 15, 16, 1000] {
            let mut trace = Trace::default();
            for i in 0..count {
                trace.push(Point {
                    location: at(),
                    after: i,
                });
            }
            let kept: Vec<u32> = trace.points().map(|p| p.after).collect();
            let want: Vec<u32> = if count <= 15 {
                (0..count).collect()
            } else {
                (0..4).chain(count - 11..count).collect()
            };
            assert_eq!(kept, want, "count = {count}");
            assert_eq!(trace.dropped(), count.saturating_sub(15));
        }
    }

    #[test]
    fn test_trace_recorded_counter_saturates() {
        let mut trace = Trace {
            recorded: u32::MAX - 1,
            ..Trace::default()
        };
        trace.push(Point {
            location: at(),
            after: 0,
        });
        trace.push(Point {
            location: at(),
            after: 0,
        });
        assert_eq!(trace.recorded, u32::MAX);
    }

    #[test]
    fn test_section_text_is_cut_at_a_character_boundary() {
        // 18 bytes left: "aé" (3) and the 14-byte marker fit; "€" would not.
        let mut extras = Extras {
            bytes: limits::MAX_BYTES - 18,
            ..Extras::default()
        };
        let text = extras.format_bounded(&"aé€bcdefghijklmnopqrstuvwxyz");
        assert_eq!(text, "aé...(truncated)", "{text}");
        assert!(text.len() <= 18);
        assert_eq!(extras.omitted.truncated_text, 1);
        let mut full = Extras {
            bytes: limits::MAX_BYTES,
            ..Extras::default()
        };
        assert_eq!(full.format_bounded(&"x"), "");
        extras.add_section(SectionKind::Note, Cow::Owned("ok".to_string()));
        assert_eq!(extras.sections.len(), 1);
    }

    #[test]
    fn test_section_and_attachment_budgets() {
        let mut extras = Extras::default();
        for _ in 0..limits::MAX_SECTIONS + 2 {
            extras.add_section(SectionKind::Note, Cow::Borrowed("n"));
        }
        assert_eq!(extras.sections.len(), limits::MAX_SECTIONS);
        assert_eq!(extras.omitted.sections, 2);
        for i in 0..limits::MAX_ATTACHMENTS + 3 {
            extras.add_attachment(
                Attachment {
                    value: Box::new(i),
                    display: None,
                },
                8,
            );
        }
        assert_eq!(extras.attachments.len(), limits::MAX_ATTACHMENTS);
        assert_eq!(extras.omitted.attachments, 3);
    }
}

//! Typed attachments (read back by type) and sections (notes, warnings,
//! suggestions for the person reading the report). PLAN 7.10.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use core::any::{Any, TypeId};
use core::fmt::Display;
use core::marker::PhantomData;

use crate::error::Error;
use crate::ext::{show_as, Attachment, SectionData, SectionKind};

impl Error {
    /// Attaches a value of any type, to be read back by type with
    /// [`attachment`](Error::attachment). Attachments are **opaque**: never
    /// printed and never reported as text. Use
    /// [`attach_display`](Error::attach_display) for one that should appear
    /// in the report.
    ///
    /// Up to 32 are kept; past that, or past the error's byte budget (64
    /// KiB, counting each value's own size), the new one is dropped and
    /// counted under `Omitted:`.
    ///
    /// ```
    /// #[derive(Debug, Clone, Copy, PartialEq)]
    /// struct RequestId(u64);
    ///
    /// let error = report_err::err!("order not found").attach(RequestId(7));
    /// assert_eq!(error.attachment::<RequestId>(), Some(&RequestId(7)));
    /// assert!(!format!("{error:?}").contains("RequestId"));
    /// ```
    pub fn attach<A: Send + Sync + 'static>(mut self, value: A) -> Error {
        let size = core::mem::size_of::<A>();
        self.raw.extras_mut().add_attachment(
            Attachment {
                value: Box::new(value),
                display: None,
            },
            size,
        );
        self
    }

    /// As [`attach`](Error::attach), and also printed under `Attachments:`
    /// in the report.
    ///
    /// ```
    /// let error = report_err::err!("query failed").attach_display("tenant 42");
    /// assert!(format!("{error:?}").contains("Attachments:\n    tenant 42"));
    /// assert_eq!(error.attachment::<&str>(), Some(&"tenant 42"));
    /// ```
    pub fn attach_display<A: Display + Send + Sync + 'static>(mut self, value: A) -> Error {
        let size = core::mem::size_of::<A>();
        let attachment = Attachment {
            value: Box::new(value),
            display: Some(show_as::<A>),
        };
        self.raw.extras_mut().add_attachment(attachment, size);
        self
    }

    /// The most recently attached value of type `A`.
    ///
    /// ```
    /// let error = report_err::err!("x").attach(1u32).attach(2u32);
    /// assert_eq!(error.attachment::<u32>(), Some(&2));
    /// assert!(error.attachment::<u8>().is_none());
    /// ```
    pub fn attachment<A: 'static>(&self) -> Option<&A> {
        self.attachments::<A>().next()
    }

    /// Every attached value of type `A`, newest first.
    ///
    /// ```
    /// let error = report_err::err!("x").attach(1u32).attach("a").attach(2u32);
    /// let all: Vec<u32> = error.attachments::<u32>().copied().collect();
    /// assert_eq!(all, [2, 1]);
    /// ```
    pub fn attachments<A: 'static>(&self) -> Attachments<'_, A> {
        let list = self
            .raw
            .extras()
            .map(|e| e.attachments.as_slice())
            .unwrap_or(&[]);
        Attachments {
            list,
            index: list.len(),
            kind: PhantomData,
        }
    }

    /// Adds a note for the person reading the report, printed as `Note:`
    /// under the cause list. A `&'static str` is stored as is; anything else
    /// is formatted once, now, within the byte budget.
    ///
    /// ```
    /// let error = report_err::err!("cache rebuild failed")
    ///     .note("the cache was rebuilt 3 times in the last minute");
    /// assert!(format!("{error:?}").contains("Note: the cache was rebuilt 3 times in the last minute"));
    /// ```
    pub fn note<S: Display + Send + Sync + 'static>(self, text: S) -> Error {
        self.section(SectionKind::Note, text)
    }

    /// Adds a warning, printed as `Warning:`.
    ///
    /// ```
    /// let error = report_err::err!("sync stopped").warning("data written after 12:04 may be missing");
    /// assert!(format!("{error:?}").contains("Warning: data written after 12:04 may be missing"));
    /// ```
    pub fn warning<S: Display + Send + Sync + 'static>(self, text: S) -> Error {
        self.section(SectionKind::Warning, text)
    }

    /// Adds a suggestion, printed as `Suggestion:`.
    ///
    /// ```
    /// let error = report_err::err!("no config").suggestion("run `tool init` to create one");
    /// assert!(format!("{error:?}").contains("Suggestion: run `tool init` to create one"));
    /// ```
    pub fn suggestion<S: Display + Send + Sync + 'static>(self, text: S) -> Error {
        self.section(SectionKind::Suggestion, text)
    }

    /// As [`note`](Error::note), with the text built by `f`.
    ///
    /// ```
    /// let count = 3;
    /// let error = report_err::err!("x").with_note(|| format!("{count} retries"));
    /// assert_eq!(error.sections().next().map(|s| s.text), Some("3 retries"));
    /// ```
    pub fn with_note<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> Error {
        self.section(SectionKind::Note, f())
    }

    /// As [`warning`](Error::warning), with the text built by `f`.
    ///
    /// ```
    /// let error = report_err::err!("x").with_warning(|| "partial write");
    /// assert_eq!(error.sections().count(), 1);
    /// ```
    pub fn with_warning<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> Error {
        self.section(SectionKind::Warning, f())
    }

    /// As [`suggestion`](Error::suggestion), with the text built by `f`.
    ///
    /// ```
    /// let error = report_err::err!("x").with_suggestion(|| "retry later");
    /// assert_eq!(error.sections().count(), 1);
    /// ```
    pub fn with_suggestion<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(
        self,
        f: F,
    ) -> Error {
        self.section(SectionKind::Suggestion, f())
    }

    fn section<S: Display + Send + Sync + 'static>(mut self, kind: SectionKind, text: S) -> Error {
        let extras = self.raw.extras_mut();
        let text: Cow<'static, str> = match (&text as &dyn Any).downcast_ref::<&'static str>() {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(extras.format_bounded(&text)),
        };
        extras.add_section(kind, text);
        self
    }

    /// The sections, in the order they were added.
    ///
    /// ```
    /// use report_err::SectionKind;
    ///
    /// let error = report_err::err!("x").note("a").suggestion("b");
    /// let all: Vec<(SectionKind, &str)> = error.sections().map(|s| (s.kind, s.text)).collect();
    /// assert_eq!(all, [(SectionKind::Note, "a"), (SectionKind::Suggestion, "b")]);
    /// ```
    pub fn sections(&self) -> Sections<'_> {
        let list = self
            .raw
            .extras()
            .map(|e| e.sections.as_slice())
            .unwrap_or(&[]);
        Sections { inner: list.iter() }
    }
}

/// The attachments of one type, newest first. Made by
/// [`Error::attachments`].
#[must_use = "iterators do nothing unless consumed"]
pub struct Attachments<'a, A> {
    list: &'a [Attachment],
    index: usize,
    kind: PhantomData<fn() -> A>,
}

impl<'a, A: 'static> Iterator for Attachments<'a, A> {
    type Item = &'a A;

    fn next(&mut self) -> Option<&'a A> {
        while self.index > 0 {
            self.index -= 1;
            let Some(attachment) = self.list.get(self.index) else {
                continue;
            };
            if attachment.type_id() == TypeId::of::<A>() {
                if let Some(value) = attachment.value.downcast_ref::<A>() {
                    return Some(value);
                }
            }
        }
        None
    }
}

/// One section of an error: its kind and its text.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    /// How it reads in the report.
    pub kind: SectionKind,
    /// The text, as it will be printed.
    pub text: &'a str,
}

/// The sections of an error, in the order added. Made by
/// [`Error::sections`].
#[must_use = "iterators do nothing unless consumed"]
pub struct Sections<'a> {
    inner: core::slice::Iter<'a, SectionData>,
}

impl<'a> Iterator for Sections<'a> {
    type Item = Section<'a>;

    fn next(&mut self) -> Option<Section<'a>> {
        self.inner.next().map(|s| Section {
            kind: s.kind,
            text: &s.text,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for Sections<'_> {}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::Error;

    #[test]
    fn test_attach_same_type_twice_newest_first() {
        let error = Error::msg("x")
            .attach(1u8)
            .attach(String::from("s"))
            .attach(2u8);
        assert_eq!(error.attachment::<u8>(), Some(&2));
        assert_eq!(
            error.attachments::<u8>().copied().collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(error.attachment::<String>().map(String::as_str), Some("s"));
    }

    #[test]
    fn test_attach_zero_sized_value() {
        #[derive(Debug, PartialEq)]
        struct Marker;
        let error = Error::msg("x").attach(Marker);
        assert_eq!(error.attachment::<Marker>(), Some(&Marker));
    }

    #[test]
    fn test_attach_opaque_never_printed() {
        let canary = "canary-7f3a";
        let error = Error::msg("x").attach(canary).context("y");
        for text in [
            format!("{error}"),
            format!("{error:#}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            assert!(!text.contains(canary), "{text}");
        }
    }

    #[test]
    fn test_attach_budget_drops_and_counts() {
        let mut error = Error::msg("x");
        for i in 0..40u32 {
            error = error.attach(i);
        }
        assert_eq!(error.attachments::<u32>().count(), 32);
        assert!(format!("{error:?}").contains("8 attachments"));
    }

    #[test]
    fn test_sections_formatted_once_at_attach_time() {
        let error = Error::msg("x").note(format!("{} pages", 3)).warning(7u8);
        let texts: Vec<&str> = error.sections().map(|s| s.text).collect();
        assert_eq!(texts, ["3 pages", "7"]);
        assert_eq!(error.sections().len(), 2);
    }

    #[test]
    fn test_sections_none_without_extension_block() {
        let error = Error::msg("x");
        assert_eq!(error.sections().count(), 0);
        assert!(error.attachment::<u8>().is_none());
    }
}

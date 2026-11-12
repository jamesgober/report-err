//! The creation hook, reporting, and records (PLAN 7.16).

use alloc::borrow::Cow;
use core::fmt::Display;
use core::panic::Location;

use define_err::{Annotate, CreationInfo, ErrorRecord, Kinds, TrailEntry, TrailStep};

use crate::error::Error;
use crate::format::{ErrorText, Shown};
use crate::raw::REPORTED;
use crate::trace::StepKind;

#[cfg(feature = "std")]
std::thread_local! {
    /// Set while this thread runs the creation hook, so an error the hook
    /// makes does not call it again.
    static IN_HOOK: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// What the hook writes to: the new error's fields and kinds. The extension
/// block is made only if the hook adds something.
struct Annotator<'a> {
    error: &'a mut Error,
}

impl Annotate for Annotator<'_> {
    fn field(&mut self, key: &'static str, value: &dyn Display) {
        self.error.raw.extras_mut().add_field(key, value);
    }

    fn kind(&mut self, kinds: Kinds) {
        let extras = self.error.raw.extras_mut();
        extras.kinds |= kinds;
    }
}

impl Error {
    /// What every new error gets: a backtrace when std's rules turn them
    /// on, then the creation hook. Context layers never come through here.
    pub(crate) fn created(self) -> Error {
        self.capture_backtrace().run_hook()
    }

    fn run_hook(mut self) -> Error {
        let Some(hook) = define_err::creation_hook() else {
            return self;
        };
        #[cfg(feature = "std")]
        {
            if IN_HOOK.with(core::cell::Cell::get) {
                return self;
            }
            IN_HOOK.with(|flag| flag.set(true));
        }
        let info = CreationInfo::new(self.raw.origin(), self.raw.code(), self.raw.type_name());
        #[cfg(feature = "std")]
        {
            let mut out = Annotator { error: &mut self };
            let call = std::panic::AssertUnwindSafe(|| hook.on_create(&info, &mut out));
            if let Err(payload) = std::panic::catch_unwind(call) {
                crate::panic::drop_payload(payload);
                self.raw
                    .extras_mut()
                    .add_field("creation_hook", &"panicked");
            }
            IN_HOOK.with(|flag| flag.set(false));
        }
        #[cfg(not(feature = "std"))]
        hook.on_create(&info, &mut Annotator { error: &mut self });
        self
    }

    /// Hands the error to define-err's reporting facade, once.
    ///
    /// The first call, from any thread, sets a flag and returns `true`; later
    /// calls return `false` and do nothing, so outer layers never report
    /// twice. When a reporter is installed it receives the error's
    /// [`record`](Error::record), with every message already written through
    /// the guarded renderer. With none, the cost is the flag and one load.
    /// When define-err's minimum severity or kinds mask would drop the event,
    /// the record is not built.
    ///
    /// ```
    /// let error = report_err::err!("request failed");
    /// assert!(error.report());
    /// assert!(!error.report());
    /// assert!(error.is_reported());
    /// ```
    #[track_caller]
    pub fn report(&self) -> bool {
        if self.raw.set_flags(REPORTED) & REPORTED != 0 {
            return false;
        }
        if define_err::is_installed() && self.passes_filter() {
            let record = self.record_with(true, Some(Location::caller()));
            define_err::report_record(&record);
        }
        true
    }

    /// Whether define-err's filter lets an event with this error's severity
    /// and kinds through: the same test `report_record` makes, done before
    /// the record is built (D-074).
    fn passes_filter(&self) -> bool {
        if let Some(min) = define_err::min_severity() {
            if self.severity().get() < min.get() {
                return false;
            }
        }
        let mask = define_err::kinds_mask();
        mask == Kinds::ALL || self.kinds().intersects(mask)
    }

    /// Whether [`report`](Error::report) has run.
    #[must_use]
    pub fn is_reported(&self) -> bool {
        self.raw.flags() & REPORTED != 0
    }

    /// An owned, size-capped snapshot of the error, for anything that
    /// outlives it: a queue, a crash ring, another process.
    ///
    /// The chain (every context layer, the root, its sources) with each
    /// message written through the guarded renderer; the resolved code,
    /// severity, category, kinds, and public message; the sections, the
    /// hook's fields, the counts of children and suppressed errors; the
    /// trail; and the backtrace text when one was captured. Within
    /// define-err's `RecordLimits`.
    ///
    /// ```
    /// use report_err::Context;
    ///
    /// let error = std::fs::read("no/such/file").context("loading").unwrap_err();
    /// let record = error.record();
    /// assert_eq!(record.chain.len(), 2);
    /// assert_eq!(record.chain[0].message, "loading");
    /// assert!(record.handled);
    /// ```
    #[must_use]
    pub fn record(&self) -> ErrorRecord {
        self.record_with(true, None)
    }

    pub(crate) fn record_with(
        &self,
        handled: bool,
        reported_at: Option<&'static Location<'static>>,
    ) -> ErrorRecord {
        let mut record =
            ErrorRecord::new(self.code(), self.severity(), self.category(), self.kinds());
        // Public text only from a catalog entry, a `from_info` root, or the
        // category, never from an identity probe found on a link: any
        // `description()` can carry a probe (D-075).
        if let Some(text) = self.public_message() {
            record.public = Cow::Borrowed(text);
        }
        let frames = self.raw.frames();
        for (index, link) in self.chain().enumerate() {
            let text = ErrorText(link);
            let shown = Shown::guarded(&text);
            if index < frames {
                record.push_chain(&shown, None, "context");
            } else if let Some(id) = define_err::identify(link) {
                record.push_chain(&shown, id.code, id.type_name);
            } else if index == frames {
                record.push_chain(&shown, None, self.raw.type_name());
            } else {
                record.push_chain(&shown, None, "unknown");
            }
        }
        for section in self.sections() {
            let key = match section.kind {
                crate::SectionKind::Note => "note",
                crate::SectionKind::Warning => "warning",
                _ => "suggestion",
            };
            record.push_field(key, &section.text);
        }
        if let Some(extras) = self.raw.extras() {
            for (key, value) in &extras.fields {
                record.push_field(*key, value);
            }
        }
        if !self.children().is_empty() {
            record.push_field("children", &self.children().len());
        }
        if !self.suppressed().is_empty() {
            record.push_field("suppressed", &self.suppressed().len());
        }
        for step in self.trail() {
            let kind = match step.kind {
                StepKind::Origin => TrailStep::Created,
                StepKind::Context => TrailStep::Context,
                _ => TrailStep::Converted,
            };
            record.push_trail(TrailEntry::new(step.location, kind));
        }
        if let Some(location) = reported_at {
            record.push_trail(TrailEntry::new(location, TrailStep::Reported));
        }
        #[cfg(feature = "std")]
        if let Some(backtrace) = self.backtrace() {
            record.set_backtrace(backtrace);
        }
        record.set_handled(handled);
        record
    }
}

/// Reports an error that has nowhere to go (in `Drop`, in a thread nobody
/// joins) through define-err's facade, as unhandled. Returns `true` when a
/// reporter is installed to receive it.
///
/// With no reporter installed and `std` on, it writes one guarded, escaped
/// line to standard error instead, so the error is never lost silently.
///
/// ```
/// struct Segment;
///
/// impl Drop for Segment {
///     fn drop(&mut self) {
///         let flushed: Result<(), report_err::Error> = Err(report_err::err!("disk full"));
///         if let Err(error) = flushed {
///             report_err::report_unhandled(error.context("flushing a segment on drop"));
///         }
///     }
/// }
///
/// drop(Segment);
/// ```
#[track_caller]
pub fn report_unhandled(error: Error) -> bool {
    if define_err::is_installed() {
        error.raw.set_flags(REPORTED);
        let record = error.record_with(false, Some(Location::caller()));
        define_err::report_record(&record);
        return true;
    }
    #[cfg(feature = "std")]
    crate::exit::unhandled_line(&error);
    false
}

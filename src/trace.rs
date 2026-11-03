//! The propagation trace (which way an error travelled, PLAN 7.9) and lazy
//! backtraces (7.13).

use core::error::Error as StdError;
use core::panic::Location;

use crate::error::Error;
use crate::ext::{Point, Trace};
use crate::raw::RawError;

/// What a step of the trail is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum StepKind {
    /// Where the error was created.
    Origin,
    /// A context layer was added.
    Context,
    /// A `.trace()` point.
    Trace,
}

/// One step of an error's trail. Made by [`Error::trail`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct Step<'a> {
    /// Where it happened.
    pub location: &'static Location<'static>,
    /// What happened there.
    pub kind: StepKind,
    /// The context layer added at this step, for [`StepKind::Context`].
    pub context: Option<&'a (dyn StdError + 'static)>,
}

impl Error {
    /// Records the caller's location in the error's trail.
    ///
    /// A `?` that does not change the error type (an `Error` returned where
    /// an `Error` is expected) cannot record where it happened: Rust calls the
    /// identity conversion, which cannot be tracked. `.trace()` records that
    /// line. The first 4 and the latest 11 points are kept, with a count of
    /// those dropped between. One store, plus the extension block the first
    /// time.
    ///
    /// ```
    /// use report_err::{err, Context, Result, StepKind};
    ///
    /// fn inner() -> Result<()> {
    ///     Err(err!("disk full"))
    /// }
    /// fn outer() -> Result<()> {
    ///     inner().trace()?;               // recorded
    ///     Ok(())
    /// }
    ///
    /// let error = outer().unwrap_err();
    /// let kinds: Vec<StepKind> = error.trail().map(|s| s.kind).collect();
    /// assert_eq!(kinds, [StepKind::Origin, StepKind::Trace]);
    /// ```
    #[track_caller]
    pub fn trace(mut self) -> Error {
        let after = self.raw.pushed();
        self.raw.extras_mut().trace.push(Point {
            location: Location::caller(),
            after,
        });
        self
    }

    /// The way the error travelled, in the order it happened: its origin,
    /// every context layer, and every `.trace()` point.
    ///
    /// ```
    /// use report_err::StepKind;
    ///
    /// let error = report_err::err!("disk full").context("saving").trace().context("closing");
    /// let kinds: Vec<StepKind> = error.trail().map(|s| s.kind).collect();
    /// assert_eq!(kinds, [StepKind::Origin, StepKind::Context, StepKind::Trace, StepKind::Context]);
    /// for step in error.trail() {
    ///     println!("{:?} at {}", step.kind, step.location);
    /// }
    /// ```
    pub fn trail(&self) -> Trail<'_> {
        let trace = self.raw.extras().map(|e| &e.trace);
        Trail {
            raw: &self.raw,
            origin: true,
            frame: 0,
            trace,
            point: 0,
        }
    }

    /// The backtrace captured when the error was created, if any.
    ///
    /// A backtrace is captured only at the root (never for context), only
    /// when `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE` turns them on (std's
    /// rules: `RUST_LIB_BACKTRACE=0` turns them off for errors only), or when
    /// [`with_backtrace`](Error::with_backtrace) forces one. Symbols are
    /// resolved only when it is printed.
    ///
    /// ```
    /// let error = report_err::err!("x").with_backtrace();
    /// assert!(error.backtrace().is_some());
    /// ```
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn backtrace(&self) -> Option<&std::backtrace::Backtrace> {
        self.raw.extras().and_then(|e| e.backtrace.as_ref())
    }

    /// Captures a backtrace now if the error has none, whatever the
    /// environment says. A second call keeps the first.
    ///
    /// ```
    /// let error = report_err::err!("invariant broken").with_backtrace();
    /// assert!(format!("{error:?}").contains("Backtrace:"));
    /// ```
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn with_backtrace(mut self) -> Error {
        if self.backtrace().is_none() {
            self.raw.extras_mut().backtrace = Some(std::backtrace::Backtrace::force_capture());
        }
        self
    }

    /// Captures a backtrace for a new root when std's rules turn them on.
    /// With them off this is one cached check and stores nothing.
    #[cfg(feature = "std")]
    pub(crate) fn capture_backtrace(mut self) -> Error {
        let backtrace = std::backtrace::Backtrace::capture();
        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            self.raw.extras_mut().backtrace = Some(backtrace);
        }
        self
    }

    #[cfg(not(feature = "std"))]
    pub(crate) fn capture_backtrace(self) -> Error {
        self
    }
}

/// The steps of an error's trail, in the order they happened. Made by
/// [`Error::trail`].
#[must_use = "iterators do nothing unless consumed"]
pub struct Trail<'a> {
    raw: &'a RawError,
    origin: bool,
    /// The next context layer to yield (innermost first).
    frame: usize,
    trace: Option<&'a Trace>,
    /// The next trace point to yield (oldest first).
    point: usize,
}

impl Trail<'_> {
    /// Trace points dropped between the first 4 and the latest 11.
    ///
    /// ```
    /// let mut error = report_err::err!("x");
    /// for _ in 0..20 {
    ///     error = error.trace();
    /// }
    /// assert_eq!(error.trail().omitted(), 5);
    /// ```
    pub fn omitted(&self) -> u32 {
        self.raw.extras().map_or(0, |e| e.trace.dropped())
    }

    /// The order in which layer `index` was added, counting layers a budget
    /// dropped: the innermost eight keep their place, and every later layer
    /// was added after all the dropped ones.
    fn added(&self, index: usize) -> u32 {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let omitted = self.raw.omitted();
        if omitted == 0 || index < 8 {
            index
        } else {
            index.saturating_add(omitted)
        }
    }
}

impl<'a> Iterator for Trail<'a> {
    type Item = Step<'a>;

    fn next(&mut self) -> Option<Step<'a>> {
        if core::mem::take(&mut self.origin) {
            return Some(Step {
                location: self.raw.origin(),
                kind: StepKind::Origin,
                context: None,
            });
        }
        let frame = self.raw.frame(self.frame);
        let point = self.trace.and_then(|t| t.get(self.point));
        match (frame, point) {
            // A point recorded after `after` layers comes before layer `after`.
            (Some(frame), Some(point)) if self.added(self.frame) < point.after => {
                self.frame += 1;
                Some(Step {
                    location: frame.location,
                    kind: StepKind::Context,
                    context: Some(frame),
                })
            }
            (_, Some(point)) => {
                self.point += 1;
                Some(Step {
                    location: point.location,
                    kind: StepKind::Trace,
                    context: None,
                })
            }
            (Some(frame), None) => {
                self.frame += 1;
                Some(Step {
                    location: frame.location,
                    kind: StepKind::Context,
                    context: Some(frame),
                })
            }
            (None, None) => None,
        }
    }
}

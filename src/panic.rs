//! The panic boundary (PLAN 7.15).

use alloc::boxed::Box;
use alloc::string::String;
use core::any::Any;
use core::panic::Location;

use crate::error::Error;
use crate::raw::{RawError, PANIC};

/// Whether `boundary` can catch a panic in this build: `true` when panics
/// unwind, `false` under `panic = "abort"`, where the process aborts at the
/// panic as std documents for `catch_unwind`.
///
/// ```
/// assert_eq!(report_err::PANICS_CAUGHT, cfg!(panic = "unwind"));
/// ```
pub const PANICS_CAUGHT: bool = cfg!(panic = "unwind");

/// The root of an error made from a payload that is not a string.
const NOT_A_STRING: &str = "panicked with a non-string payload";

/// Drops a panic payload whose own `Drop` may panic: that panic is caught
/// and its payload dropped the same way, up to four times; a payload whose
/// drops keep panicking is leaked instead of propagated.
pub(crate) fn drop_payload(payload: Box<dyn Any + Send + 'static>) {
    #[cfg(feature = "std")]
    {
        let mut payload = payload;
        for _ in 0..4 {
            let drop_it = std::panic::AssertUnwindSafe(move || drop(payload));
            match std::panic::catch_unwind(drop_it) {
                Ok(()) => return,
                Err(again) => payload = again,
            }
        }
        core::mem::forget(payload);
    }
    #[cfg(not(feature = "std"))]
    drop(payload);
}

impl Error {
    /// Turns a panic payload (from `JoinHandle::join` or `catch_unwind`) into
    /// an error: its root is the panic message, or
    /// `panicked with a non-string payload`. It has
    /// [`is_panic`](Error::is_panic), `Kinds::PANIC`, category `Internal`,
    /// severity `FATAL`, and exit code 101.
    ///
    /// ```
    /// let payload = std::panic::catch_unwind(|| panic!("worker {} died", 3)).unwrap_err();
    /// let error = report_err::Error::from_panic(payload);
    /// assert_eq!(error.to_string(), "worker 3 died");
    /// assert!(error.is_panic());
    /// assert_eq!(error.exit_code(), 101);
    /// ```
    #[track_caller]
    pub fn from_panic(payload: Box<dyn Any + Send + 'static>) -> Error {
        Error::from_panic_at(payload, Location::caller())
    }

    pub(crate) fn from_panic_at(
        payload: Box<dyn Any + Send + 'static>,
        location: &'static Location<'static>,
    ) -> Error {
        let raw = match payload.downcast::<&'static str>() {
            Ok(text) => RawError::from_message(*text, location),
            Err(payload) => match payload.downcast::<String>() {
                Ok(text) => RawError::from_message(*text, location),
                Err(payload) => {
                    drop_payload(payload);
                    RawError::from_message(NOT_A_STRING, location)
                }
            },
        };
        raw.set_flags(PANIC);
        Error { raw }.created()
    }

    /// Whether the error was made from a panic (`boundary` or
    /// [`from_panic`](Error::from_panic)).
    ///
    /// ```
    /// assert!(!report_err::err!("x").is_panic());
    /// ```
    #[must_use]
    pub fn is_panic(&self) -> bool {
        self.raw.flags() & PANIC != 0
    }
}

/// Calls `f`, and turns a panic in it into an error (see
/// [`Error::from_panic`]) whose location is this call.
///
/// Std's panic hook still runs first; report-err never installs one. Under
/// `panic = "abort"` nothing is caught ([`PANICS_CAUGHT`]). The closure must
/// be `UnwindSafe`, as std requires: wrap state you have checked in
/// `AssertUnwindSafe`.
///
/// ```
/// let caught = report_err::boundary(|| -> report_err::Result<u8> { panic!("plugin bug") });
/// let error = caught.unwrap_err();
/// assert!(error.is_panic());
/// assert_eq!(error.to_string(), "plugin bug");
///
/// let fine = report_err::boundary(|| Ok(7));
/// assert_eq!(fine.unwrap(), 7);
/// ```
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[track_caller]
pub fn boundary<T, F>(f: F) -> crate::Result<T>
where
    F: FnOnce() -> crate::Result<T> + std::panic::UnwindSafe,
{
    let location = Location::caller();
    match std::panic::catch_unwind(f) {
        Ok(result) => result,
        Err(payload) => Err(Error::from_panic_at(payload, location)),
    }
}

/// [`boundary`] and [`ExitReport`](crate::ExitReport) in one call: a panic
/// in `app` becomes a report and exit code 101 when panics unwind.
///
/// ```no_run
/// fn main() -> report_err::ExitReport {
///     report_err::run(|| {
///         println!("serving");
///         Ok(())
///     })
/// }
/// ```
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[track_caller]
pub fn run<F>(app: F) -> crate::ExitReport
where
    F: FnOnce() -> crate::Result<()> + std::panic::UnwindSafe,
{
    match boundary(app) {
        Ok(()) => crate::ExitReport::from(Ok::<(), Error>(())),
        Err(error) => crate::ExitReport::from(error),
    }
}

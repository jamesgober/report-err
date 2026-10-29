//! `main` integration (PLAN 7.14): `ExitReport`, which prints the report to
//! standard error, escaped for a terminal, and exits with the error's code.

use core::fmt;
use std::io::Write as _;
use std::process::{ExitCode, Termination};

use crate::error::Error;
use crate::limits;

/// The return type of a `main` that exits with a code chosen from the error.
///
/// On `Err`, it writes `Error: ` and the report (`{:?}`) to standard error,
/// with terminal control characters escaped and a message's continuation
/// lines indented, so no message can move the cursor or fake a block of the
/// report; then it exits with [`Error::exit_code`]. A closed or broken
/// standard error is ignored; the exit code is still right. The report is cut
/// at 1 MiB with `(report truncated)`. `Ok` exits with 0.
///
/// ```no_run
/// fn run() -> report_err::Result<()> {
///     Ok(())
/// }
///
/// fn main() -> report_err::ExitReport {
///     run().into()
/// }
/// ```
#[must_use = "return it from `main`"]
pub struct ExitReport {
    error: Option<Error>,
}

impl<E: Into<Error>> From<Result<(), E>> for ExitReport {
    #[track_caller]
    fn from(result: Result<(), E>) -> ExitReport {
        ExitReport {
            error: result.err().map(Into::into),
        }
    }
}

impl From<Error> for ExitReport {
    fn from(error: Error) -> ExitReport {
        ExitReport { error: Some(error) }
    }
}

impl Termination for ExitReport {
    fn report(self) -> ExitCode {
        let Some(error) = self.error else {
            return ExitCode::SUCCESS;
        };
        let stderr = std::io::stderr();
        let mut out = Capped {
            inner: stderr.lock(),
            written: 0,
            full: false,
        };
        let _ = fmt::Write::write_str(&mut out, "Error: ");
        let _ = crate::format::report(&error, &mut out, true);
        if out.full {
            let _ = out.inner.write_all(b"\n(report truncated)");
        }
        let _ = out.inner.write_all(b"\n");
        let _ = out.inner.flush();
        ExitCode::from(error.exit_code())
    }
}

impl fmt::Debug for ExitReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error {
            None => f.write_str("ExitReport(Ok)"),
            Some(error) => f.debug_tuple("ExitReport").field(error).finish(),
        }
    }
}

/// Writes to an `io::Write` and stops at `max_render` bytes. Write failures
/// (a closed or broken pipe) end the output quietly.
struct Capped<W: std::io::Write> {
    inner: W,
    written: usize,
    full: bool,
}

impl<W: std::io::Write> fmt::Write for Capped<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.full {
            return Ok(());
        }
        let room = limits::MAX_RENDER.saturating_sub(self.written);
        let mut take = s.len().min(room);
        while take > 0 && !s.is_char_boundary(take) {
            take -= 1;
        }
        let part = s.get(..take).unwrap_or("");
        if self.inner.write_all(part.as_bytes()).is_err() {
            return Err(fmt::Error);
        }
        self.written += part.len();
        if take < s.len() {
            self.full = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::vec::Vec;
    use core::fmt::Write;

    use super::*;

    #[test]
    fn test_capped_stops_at_the_budget_on_a_character_boundary() {
        let mut out = Capped {
            inner: Vec::new(),
            written: limits::MAX_RENDER - 1,
            full: false,
        };
        out.write_str("é").ok();
        assert!(out.full);
        assert!(out.inner.is_empty());
        out.write_str("more").ok();
        assert!(out.inner.is_empty());
    }

    #[test]
    fn test_exit_report_from_ok_and_err() {
        let ok: ExitReport = Ok::<(), Error>(()).into();
        assert_eq!(format!("{ok:?}"), "ExitReport(Ok)");
        let err: ExitReport = Err::<(), _>(std::io::Error::other("x")).into();
        assert!(format!("{err:?}").starts_with("ExitReport(x"));
    }
}

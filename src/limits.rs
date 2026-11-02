//! Budgets on everything that can grow (PLAN 7.18), and the process-wide
//! settings. The budget defaults apply from `0.3.0`; `configure` makes them
//! settable once per process in `0.5.0`.

use core::fmt;

use define_err::{catalog, Code, CodeInfo};

use crate::raw::WriteOnce;

/// Context layers kept per error. Past it, the oldest layer above the
/// innermost eight is dropped and counted.
pub(crate) const MAX_FRAMES: usize = 64;

/// Links walked in one chain. Also stops a `source()` that never ends.
pub(crate) const MAX_CHAIN: usize = 128;

/// Attachments kept per error; past it a new one is dropped and counted.
pub(crate) const MAX_ATTACHMENTS: usize = 32;

/// Sections kept per error; past it a new one is dropped and counted.
pub(crate) const MAX_SECTIONS: usize = 32;

/// Text and value bytes one error's extension block holds: formatted
/// sections, display attachments, and the shallow size of attached values.
pub(crate) const MAX_BYTES: usize = 64 * 1024;

/// Bytes one guarded rendering writes (`ExitReport`); past it the report is
/// cut with `(report truncated)`.
#[cfg_attr(not(feature = "std"), allow(dead_code))]
pub(crate) const MAX_RENDER: usize = 1024 * 1024;

/// The catalogs set with [`configure_catalogs`].
static CATALOGS: WriteOnce<&'static [&'static [CodeInfo]]> = WriteOnce::new();

/// Sets the catalogs an explicit code is looked up in, once per process.
///
/// A code set with `coded` or a code-first macro carries no metadata of its
/// own; this lets it find its title, severity, category, kinds, and public
/// message through define-err's `catalog::find`. A code found on a typed
/// error in the chain has all of those but the public text without it.
/// Optional; call it at the start of `main`. One allocation.
///
/// # Errors
///
/// [`ConfigureError::AlreadyConfigured`] on a second call.
///
/// ```
/// define_err::define_errors! {
///     namespace = "DB";
///     pub enum DbError {
///         #[code(1, name = TIMEOUT, help = "Raise the timeout.", title = "Query timed out")]
///         #[category(Unavailable)]
///         Timeout => "query timed out",
///     }
/// }
///
/// static CATALOGS: &[&[define_err::CodeInfo]] = &[DbError::CATALOG];
/// report_err::configure_catalogs(CATALOGS).unwrap();
///
/// let error = report_err::err!("no reply").coded(DbError::TIMEOUT);
/// assert_eq!(error.category(), report_err::Category::Unavailable);
/// ```
pub fn configure_catalogs(catalogs: &'static [&'static [CodeInfo]]) -> Result<(), ConfigureError> {
    CATALOGS
        .set(catalogs)
        .map_err(|_| ConfigureError::AlreadyConfigured)
}

/// The catalog entry of `code`, when catalogs are configured and name it.
pub(crate) fn find_code(code: Code) -> Option<&'static CodeInfo> {
    CATALOGS.get().and_then(|c| catalog::find(c, code))
}

/// Why a process-wide setting was refused.
///
/// ```
/// use report_err::ConfigureError;
///
/// let text = ConfigureError::AlreadyConfigured.to_string();
/// assert!(text.contains("once"));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum ConfigureError {
    /// It was set before. Each setting can be made once per process.
    AlreadyConfigured,
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigureError::AlreadyConfigured => f.write_str(
                "this setting was already configured; set it once, at the start of main, before other threads start",
            ),
        }
    }
}

impl core::error::Error for ConfigureError {}

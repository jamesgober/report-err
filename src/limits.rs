//! Budgets on everything that can grow (PLAN 7.18), and the process-wide
//! settings, each set once per process.

use core::fmt;

use define_err::{catalog, Code, CodeInfo};

use crate::raw::WriteOnce;

/// The budgets on everything that can grow in an error (PLAN 7.18). Safe
/// for servers by default; change a copy and pass it to [`configure`].
///
/// ```
/// let mut limits = report_err::Limits::DEFAULT;
/// limits.max_frames = 32;
/// assert_eq!(limits.max_chain, 128);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub struct Limits {
    /// Context layers per error (4 to 4,096). Past it, the oldest layer above
    /// the innermost eight is dropped and counted.
    pub max_frames: u32,
    /// Attachments per error (0 to 1,024); past it a new one is refused.
    pub max_attachments: u32,
    /// Sections per error (0 to 1,024); past it a new one is refused.
    pub max_sections: u32,
    /// Suppressed errors per error (0 to 1,024); past it a new one is refused.
    pub max_suppressed: u32,
    /// Errors in one tree, children and suppressed errors and theirs included
    /// (1 to 1,048,576). A child whose tree does not fit is refused whole.
    pub max_tree: u32,
    /// Nesting of children and suppressed errors (1 to 64).
    pub max_depth: u32,
    /// Text and value bytes one error's extension block holds (1,024 to
    /// 16,777,216). Text is cut at a character boundary; values are refused.
    pub max_bytes: u32,
    /// Links walked in one chain (8 to 4,096); also stops a `source()` that
    /// never ends.
    pub max_chain: u32,
    /// Bytes one guarded rendering writes (4,096 to 67,108,864).
    pub max_render: u32,
}

impl Limits {
    /// The defaults: 64 frames, 32 attachments, 32 sections, 16 suppressed,
    /// a tree of 1,024, depth 16, 64 KiB, a chain of 128, and 1 MiB rendered.
    pub const DEFAULT: Limits = Limits {
        max_frames: 64,
        max_attachments: 32,
        max_sections: 32,
        max_suppressed: 16,
        max_tree: 1024,
        max_depth: 16,
        max_bytes: 64 * 1024,
        max_chain: 128,
        max_render: 1024 * 1024,
    };

    /// Each field with its name and allowed range.
    fn fields(&self) -> [(&'static str, u32, u32, u32); 9] {
        [
            ("max_frames", self.max_frames, 4, 4096),
            ("max_attachments", self.max_attachments, 0, 1024),
            ("max_sections", self.max_sections, 0, 1024),
            ("max_suppressed", self.max_suppressed, 0, 1024),
            ("max_tree", self.max_tree, 1, 1 << 20),
            ("max_depth", self.max_depth, 1, MAX_DEPTH as u32),
            ("max_bytes", self.max_bytes, 1024, 16 << 20),
            ("max_chain", self.max_chain, 8, 4096),
            ("max_render", self.max_render, 4096, 64 << 20),
        ]
    }

    fn check(&self) -> Result<(), ConfigureError> {
        for (field, value, min, max) in self.fields() {
            if value < min || value > max {
                return Err(ConfigureError::OutOfRange {
                    field,
                    value: u64::from(value),
                    min: u64::from(min),
                    max: u64::from(max),
                });
            }
        }
        Ok(())
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// The largest `max_depth` allowed; tree walks keep a stack this deep.
pub(crate) const MAX_DEPTH: usize = 64;

/// The budgets set with [`configure`].
static LIMITS: WriteOnce<Limits> = WriteOnce::new();

/// Sets the budgets, once per process, at the start of `main` before other
/// threads create errors.
///
/// Every field is checked first; one field out of its range refuses the
/// whole value and the defaults stay. Reading the budgets afterwards is one
/// acquire load; there is no lock.
///
/// # Errors
///
/// [`ConfigureError::OutOfRange`] naming the first bad field, or
/// [`ConfigureError::AlreadyConfigured`] on a second call.
///
/// ```
/// let mut limits = report_err::Limits::DEFAULT;
/// limits.max_frames = 2;
/// let refused = report_err::configure(limits).unwrap_err();
/// assert_eq!(refused.to_string(), "max_frames = 2 is outside 4..=4096; choose a value in that range");
///
/// limits.max_frames = 32;
/// report_err::configure(limits).unwrap();
/// assert_eq!(report_err::limits().max_frames, 32);
/// ```
pub fn configure(limits: Limits) -> Result<(), ConfigureError> {
    limits.check()?;
    LIMITS
        .set(limits)
        .map_err(|_| ConfigureError::AlreadyConfigured)
}

/// The budgets in force: those set with [`configure`], else
/// [`Limits::DEFAULT`].
///
/// ```
/// assert_eq!(report_err::limits().max_depth, 16);
/// ```
#[must_use]
pub fn limits() -> &'static Limits {
    LIMITS.get().unwrap_or(&Limits::DEFAULT)
}

pub(crate) fn max_frames() -> usize {
    limits().max_frames as usize
}

pub(crate) fn max_chain() -> usize {
    limits().max_chain as usize
}

pub(crate) fn max_attachments() -> usize {
    limits().max_attachments as usize
}

pub(crate) fn max_sections() -> usize {
    limits().max_sections as usize
}

pub(crate) fn max_suppressed() -> usize {
    limits().max_suppressed as usize
}

pub(crate) fn max_tree() -> u32 {
    limits().max_tree
}

pub(crate) fn max_depth() -> u32 {
    limits().max_depth
}

pub(crate) fn max_bytes() -> usize {
    limits().max_bytes as usize
}

#[cfg_attr(not(feature = "std"), allow(dead_code))]
pub(crate) fn max_render() -> usize {
    limits().max_render as usize
}

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
    /// A budget is outside its allowed range; nothing was changed.
    OutOfRange {
        /// The field, as named in [`Limits`].
        field: &'static str,
        /// The value given.
        value: u64,
        /// The smallest value allowed.
        min: u64,
        /// The largest value allowed.
        max: u64,
    },
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigureError::AlreadyConfigured => f.write_str(
                "this setting was already configured; set it once, at the start of main, before other threads start",
            ),
            ConfigureError::OutOfRange {
                field,
                value,
                min,
                max,
            } => write!(
                f,
                "{field} = {value} is outside {min}..={max}; choose a value in that range"
            ),
        }
    }
}

impl core::error::Error for ConfigureError {}

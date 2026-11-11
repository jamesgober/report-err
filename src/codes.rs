//! Codes, severity, category, and kinds (PLAN 7.17): looked up from
//! define-err, never invented here.

use core::error::Error as StdError;
use core::panic::Location;

use define_err::{Category, Code, CodeInfo, ErrorInfo, Identity, Kinds, Severity};

use crate::error::Error;
use crate::limits;
use crate::raw::RawError;

/// What is known about an error's code. The sources, in order: the coded
/// error's identity in the chain, the catalog entry of the code, and the root
/// when it was built with `from_info`.
struct Facts<'a> {
    code: Option<Code>,
    identity: Option<Identity<'a>>,
    entry: Option<&'static CodeInfo>,
    info: Option<&'a dyn ErrorInfo>,
}

impl Facts<'_> {
    fn severity(&self) -> Option<Severity> {
        self.identity
            .map(|i| i.severity)
            .or(self.entry.map(|e| e.severity))
            .or(self.info.map(ErrorInfo::severity))
    }

    fn category(&self) -> Option<Category> {
        self.identity
            .map(|i| i.category)
            .or(self.entry.map(|e| e.category))
            .or(self.info.map(ErrorInfo::category))
    }

    fn kinds(&self) -> Kinds {
        self.identity
            .map(|i| i.kinds)
            .or(self.entry.map(|e| e.kinds))
            .or(self.info.map(ErrorInfo::kinds))
            .unwrap_or_default()
    }
}

impl Error {
    fn facts(&self) -> Facts<'_> {
        let root: &(dyn StdError + 'static) = self.raw.root();
        let found = define_err::innermost_code(root);
        let info = self.raw.info();
        let code = self
            .raw
            .code()
            .or(found.map(|f| f.0))
            .or_else(|| info.and_then(ErrorInfo::code));
        let identity = match code {
            Some(_) => found.filter(|f| Some(f.0) == code).map(|f| f.1),
            // A defined error with no `#[code]` still knows its category.
            None => define_err::identify(root),
        };
        Facts {
            code,
            identity,
            entry: code.and_then(limits::find_code),
            info: info.filter(|i| code.is_none() || i.code() == code),
        }
    }

    /// The category when something says what it is: the code, its catalog
    /// entry, the root's `ErrorInfo`, or an `io::Error` root.
    fn known_category(&self) -> Option<Category> {
        self.facts()
            .category()
            .or_else(|| self.io_category())
            .or_else(|| self.is_panic().then_some(Category::Internal))
    }

    #[cfg(feature = "std")]
    fn io_category(&self) -> Option<Category> {
        let root: &(dyn StdError + 'static) = self.raw.root();
        root.downcast_ref::<std::io::Error>()
            .map(|e| Category::from_io_kind(e.kind()))
    }

    #[cfg(not(feature = "std"))]
    fn io_category(&self) -> Option<Category> {
        None
    }

    /// The error's code: the one set with [`coded`](Error::coded) or a
    /// code-first macro; else the innermost coded error in the chain; else
    /// the code of a [`from_info`](Error::from_info) root; else `None`.
    /// Context layers never carry a code.
    ///
    /// ```
    /// use report_err::{bail, Context, Result};
    ///
    /// define_err::define_errors! {
    ///     namespace = "DB";
    ///     pub enum DbError {
    ///         #[code(7, name = TIMEOUT, help = "Raise the timeout.")]
    ///         #[category(Unavailable)]
    ///         Timeout => "query timed out",
    ///     }
    /// }
    ///
    /// fn query() -> Result<()> {
    ///     Err(DbError::Timeout).context("loading the user")
    /// }
    ///
    /// let error = query().unwrap_err();
    /// assert_eq!(error.code(), Some(DbError::TIMEOUT));
    /// ```
    #[must_use]
    #[inline]
    pub fn code(&self) -> Option<Code> {
        match self.raw.code() {
            Some(code) => Some(code),
            None => self.found_code(),
        }
    }

    /// The code when none was set: the identity probe over the chain, then
    /// the root's `ErrorInfo`. Out of line, so the probe define-err inlines
    /// does not make `code` too large to inline at its callers (D-070).
    fn found_code(&self) -> Option<Code> {
        define_err::innermost_code(self.raw.root())
            .map(|f| f.0)
            .or_else(|| self.raw.info().and_then(ErrorInfo::code))
    }

    /// Sets the code, replacing any set before. It wins over a code found in
    /// the chain.
    ///
    /// ```
    /// let code = define_err::code!("APP", 12);
    /// let error = report_err::err!("disk full").coded(code);
    /// assert_eq!(error.code().map(|c| c.to_string()).as_deref(), Some("APP-0012"));
    /// ```
    pub fn coded(mut self, code: Code) -> Error {
        self.raw.set_code(code);
        self
    }

    /// How severe the error is: [`with_severity`](Error::with_severity); else
    /// the code's; else the category's default.
    ///
    /// ```
    /// use report_err::Severity;
    ///
    /// assert_eq!(report_err::err!("x").severity(), Severity::ERROR);
    /// ```
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.raw
            .severity()
            .or_else(|| self.facts().severity())
            .or_else(|| self.is_panic().then_some(Severity::FATAL))
            .unwrap_or_else(|| self.category().default_severity())
    }

    /// What kind of failure this is: the code's category; else, for an
    /// `io::Error` root, the category of its kind; else `Unknown`.
    ///
    /// ```
    /// use report_err::Category;
    ///
    /// assert_eq!(report_err::err!("x").category(), Category::Unknown);
    ///
    /// # #[cfg(feature = "std")] {
    /// let error = report_err::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
    /// assert_eq!(error.category(), Category::NotFound);
    /// # }
    /// ```
    #[must_use]
    pub fn category(&self) -> Category {
        self.known_category().unwrap_or(Category::Unknown)
    }

    /// The error's kinds: those added with [`with_kinds`](Error::with_kinds)
    /// and those of its code; `Kinds::ERROR` when there are none.
    ///
    /// ```
    /// use report_err::Kinds;
    ///
    /// let error = report_err::err!("x").with_kinds(Kinds::SECURITY);
    /// assert!(error.kinds().contains(Kinds::SECURITY));
    /// assert_eq!(report_err::err!("y").kinds(), Kinds::ERROR);
    /// ```
    #[must_use]
    pub fn kinds(&self) -> Kinds {
        let added = self.raw.extras().map(|e| e.kinds).unwrap_or_default();
        let mut kinds = added | self.facts().kinds();
        if self.is_panic() {
            kinds |= Kinds::PANIC;
        }
        if kinds.is_empty() {
            Kinds::ERROR
        } else {
            kinds
        }
    }

    /// Overrides the severity.
    ///
    /// ```
    /// use report_err::Severity;
    ///
    /// let error = report_err::err!("x").with_severity(Severity::CRITICAL);
    /// assert_eq!(error.severity(), Severity::CRITICAL);
    /// ```
    pub fn with_severity(mut self, severity: Severity) -> Error {
        self.raw.set_severity(severity);
        self
    }

    /// Adds every kind in `kinds` to the error's own (combined with OR).
    ///
    /// ```
    /// use report_err::Kinds;
    ///
    /// let error = report_err::err!("x").with_kinds(Kinds::AUDIT).with_kinds(Kinds::USER);
    /// assert!(error.kinds().contains(Kinds::AUDIT | Kinds::USER));
    /// ```
    pub fn with_kinds(mut self, kinds: Kinds) -> Error {
        let extras = self.raw.extras_mut();
        extras.kinds |= kinds;
        self
    }

    /// Text a client may see, for an error with a code: the code's catalog
    /// entry; else the root's `ErrorInfo` when built with
    /// [`from_info`](Error::from_info); else the category's default public
    /// message. `None` without a code. It never comes from any `Display`:
    /// `Display` is a private message for logs.
    ///
    /// ```
    /// let error = report_err::err!("db password rejected for root");
    /// assert_eq!(error.public_message(), None);
    /// let error = error.coded(define_err::code!("APP", 1));
    /// assert_eq!(error.public_message(), Some("An unexpected error occurred."));
    /// ```
    #[must_use]
    pub fn public_message(&self) -> Option<&'static str> {
        let facts = self.facts();
        facts.code?;
        if let Some(entry) = facts.entry {
            return Some(entry.public);
        }
        if let Some(info) = facts.info {
            return Some(info.public_message());
        }
        Some(self.category().default_public_message())
    }

    /// The variant's own `#[public]` text, when only the identity probe
    /// knows the code (no catalog entry, no `from_info` root). The probe
    /// lends it, so `public_message` cannot return it; the owned record
    /// carries it.
    pub(crate) fn identity_public(&self) -> Option<alloc::string::String> {
        let facts = self.facts();
        if facts.entry.is_some() || facts.info.is_some() {
            return None;
        }
        facts.identity?.public.map(alloc::string::String::from)
    }

    /// Wraps an error like [`Error::new`], and keeps it reachable as
    /// `&dyn ErrorInfo` through [`info`](Error::info), so per-value metadata
    /// the identity probe cannot carry (`retry_after`, a `#[location]` field)
    /// survives.
    ///
    /// ```
    /// define_err::define_errors! {
    ///     pub enum Busy {
    ///         Pool { #[retry_after] wait: core::time::Duration } => "pool is busy",
    ///     }
    /// }
    ///
    /// let wait = core::time::Duration::from_millis(250);
    /// let error = report_err::Error::from_info(Busy::Pool { wait });
    /// assert_eq!(error.info().and_then(|i| i.retry_after()), Some(wait));
    /// ```
    #[track_caller]
    pub fn from_info<E>(error: E) -> Error
    where
        E: ErrorInfo + Send + Sync + 'static,
    {
        Error {
            raw: RawError::from_info(error, Location::caller()),
        }
        .created()
    }

    /// The root as `&dyn ErrorInfo`, for an error built with
    /// [`from_info`](Error::from_info).
    ///
    /// ```
    /// assert!(report_err::err!("x").info().is_none());
    /// ```
    #[must_use]
    pub fn info(&self) -> Option<&dyn ErrorInfo> {
        self.raw.info()
    }

    /// The exit code `main` ends with when it returns this error through
    /// `ExitReport` (feature `std`): an explicit
    /// [`with_exit_code`](Error::with_exit_code); else the category's exit
    /// code, when the category is known from a code, a catalog, or an
    /// `io::Error` root; otherwise 1, what anyhow users get today.
    ///
    /// ```
    /// let error = report_err::err!("usage: tool <file>");
    /// assert_eq!(error.exit_code(), 1);
    /// assert_eq!(error.with_exit_code(64).exit_code(), 64);
    /// ```
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        self.raw
            .exit()
            .or_else(|| self.is_panic().then_some(101))
            .or_else(|| self.known_category().map(Category::exit_code))
            .unwrap_or(1)
    }

    /// Sets the exit code. 0 is not a failure code and becomes 1.
    ///
    /// ```
    /// let error = report_err::err!("x").with_exit_code(0);
    /// assert_eq!(error.exit_code(), 1);
    /// ```
    pub fn with_exit_code(mut self, code: u8) -> Error {
        self.raw.set_exit(code.max(1));
        self
    }

    /// The error for `None` given a code: the catalog title of the code, or
    /// the code itself when no catalog names it.
    #[track_caller]
    pub(crate) fn from_code(code: Code) -> Error {
        let error = match limits::find_code(code) {
            Some(entry) => Error::from_display(entry.title),
            None => Error::from_display(code),
        };
        error.coded(code)
    }

    /// Writes the report's `Code:` line, when a code is known.
    pub(crate) fn code_line(&self) -> Option<CodeLine> {
        let code = self.code()?;
        Some(CodeLine {
            code,
            category: self.category(),
            severity: self.severity(),
        })
    }
}

/// The report's `Code: DB-0042 (unavailable, severity 17)` line.
pub(crate) struct CodeLine {
    code: Code,
    category: Category,
    severity: Severity,
}

impl core::fmt::Display for CodeLine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} ({}, severity {})",
            self.code,
            self.category.name(),
            self.severity.get()
        )
    }
}

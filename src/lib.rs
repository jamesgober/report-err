//! # report_err
//!
//! Dynamic errors for Rust applications. One type, [`Error`], holds any
//! error, gathers context as it travels up the stack, and prints a clear
//! report when it reaches `main`. Code written for `anyhow` reads the same:
//! `Error`, [`Result`], [`Context`], [`bail!`], [`ensure!`], `format_err!`,
//! and `?` keep their names, so moving from anyhow is a change of crate name
//! (anyhow's `anyhow!` is [`err!`]).
//!
//! An error that carries only its cause and location is one allocation and
//! one machine word, the same as anyhow; adding context costs less (the
//! first layer is free, layers two to five share one block). An `Error`
//! dereferences to `dyn std::error::Error` with every context layer in its
//! `source()` chain.
//!
//! ## Tier 1
//!
//! ```
//! use report_err::{bail, ensure, Context, Result};
//!
//! #[derive(Debug)]
//! struct Config { port: u16 }
//!
//! fn parse(text: &str) -> Result<Config> {
//!     let port = text.trim().parse().context("the port is not a number")?;
//!     Ok(Config { port })
//! }
//!
//! fn load_config(path: &str) -> Result<Config> {
//!     let text = std::fs::read_to_string(path)
//!         .with_context(|| format!("reading config file {path}"))?;
//!     ensure!(!text.is_empty(), "config file {path} is empty");
//!     let config = parse(&text).context("parsing config")?;
//!     if config.port == 0 {
//!         bail!("port 0 is not allowed");
//!     }
//!     Ok(config)
//! }
//!
//! let error = load_config("no/such/app.conf").unwrap_err();
//! assert_eq!(error.to_string(), "reading config file no/such/app.conf");
//! assert!(error.downcast_ref::<std::io::Error>().is_some());
//! ```
//!
//! Returned from `main`, the error prints its report and the process exits
//! with code 1:
//!
//! ```text
//! Error: reading config file app.conf
//!
//! Caused by:
//!     No such file or directory (os error 2)
//!
//! Location: src/main.rs:12:10
//! ```
//!
//! ## Formatting
//!
//! | Format | Prints |
//! |---|---|
//! | `{}` | the outermost message |
//! | `{:#}` | the whole chain on one line, joined by `": "`, a layer skipped when the one before already ends with its text |
//! | `{:?}` | the report: the message, `Caused by:` (numbered when there are several), `Location:` |
//! | `{:#?}` | the parts of the error, for developers |
//!
//! `{}` and `{:#}` never allocate. Width, fill, and precision are ignored.
//!
//! ## `no_std`
//!
//! The crate is `no_std` with `alloc`. The `std` feature (on by default)
//! adds what needs the operating system as it arrives in later versions:
//! backtraces, exit codes, the panic boundary.
//!
//! ## Status
//!
//! `0.3.0` ships the anyhow surface. Codes, attachments, sections, the
//! propagation trace, and exit codes arrive in `0.4.0`; several causes,
//! suppressed errors, budgets, and the panic boundary in `0.5.0`. The whole
//! API, with the version each item arrives in, is in `docs/API.md` in the
//! [repository](https://github.com/jamesgober/report-err).

#![doc(html_root_url = "https://docs.rs/report-err")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![no_std]
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]
#![deny(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    clippy::ptr_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::borrow_as_ptr
)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::dbg_macro
    )
)]

extern crate alloc;
// define-err's types are first used in 0.4.0 (dev/PLAN.md, D-019).
use define_err as _;
#[cfg(feature = "std")]
extern crate std;

// Several shared parts (the write-once configuration, the per-error
// `reported` flag) need atomic compare-and-swap.
#[cfg(not(target_has_atomic = "ptr"))]
compile_error!(
    "report-err needs atomic compare-and-swap on pointer-sized values; \
     targets without it (such as thumbv6m-none-eabi) are not supported"
);

mod chain;
mod context;
mod error;
mod format;
mod frame;
mod limits;
mod macros;
#[allow(unsafe_code)]
mod raw;
mod std_interop;

#[doc(hidden)]
pub mod __private;

pub use crate::chain::Chain;
pub use crate::context::Context;
pub use crate::error::Error;

/// `Result<T, Error>`, with `Error` as the default error type, so
/// `Result<T>` reads like anyhow's. `Result<T, MyError>` still works.
///
/// ```
/// fn parse(text: &str) -> report_err::Result<u32> {
///     Ok(text.trim().parse()?)
/// }
/// assert_eq!(parse(" 7 ").unwrap(), 7);
/// ```
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// The Tier 1 names in one import: `Error`, `Result`, `Context`, `err!`,
/// `bail!`, `ensure!`.
///
/// ```
/// use report_err::prelude::*;
///
/// fn half(n: u32) -> Result<u32> {
///     ensure!(n % 2 == 0, "{n} is odd");
///     Ok(n / 2)
/// }
/// assert!(half(3).is_err());
/// ```
pub mod prelude {
    pub use crate::{bail, ensure, err, Context, Error, Result};
}

/// Crate version string, populated by Cargo at build time.
///
/// ```
/// assert!(!report_err::VERSION.is_empty());
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Compile-fail cases: each example must not compile.
///
/// A `!Send` error cannot become an `Error`:
///
/// ```compile_fail
/// #[derive(Debug)]
/// struct NotSend(std::rc::Rc<()>);
/// impl std::fmt::Display for NotSend {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("x") }
/// }
/// impl std::error::Error for NotSend {}
/// let _ = report_err::Error::new(NotSend(std::rc::Rc::new(())));
/// ```
///
/// A `!Sync` error cannot become an `Error`:
///
/// ```compile_fail
/// #[derive(Debug)]
/// struct NotSync(std::cell::Cell<u8>);
/// impl std::fmt::Display for NotSync {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("x") }
/// }
/// impl std::error::Error for NotSync {}
/// let _ = report_err::Error::new(NotSync(std::cell::Cell::new(0)));
/// ```
///
/// An error that borrows (not `'static`) cannot become an `Error`:
///
/// ```compile_fail
/// #[derive(Debug)]
/// struct Borrowed<'a>(&'a str);
/// impl std::fmt::Display for Borrowed<'_> {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.0) }
/// }
/// impl std::error::Error for Borrowed<'_> {}
/// let text = String::from("x");
/// let _ = report_err::Error::new(Borrowed(&text));
/// ```
///
/// `?` does not convert a `Box<dyn Error + Send + Sync>` (use
/// `Error::from_boxed`), as with anyhow:
///
/// ```compile_fail
/// fn f() -> Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
/// fn g() -> report_err::Result<()> { f()?; Ok(()) }
/// ```
///
/// `Context` is sealed:
///
/// ```compile_fail
/// struct Mine;
/// impl report_err::Context<(), ()> for Mine {
///     fn context<C>(self, _: C) -> report_err::Result<()>
///     where C: std::fmt::Display + Send + Sync + 'static { Ok(()) }
///     fn with_context<C, F>(self, _: F) -> report_err::Result<()>
///     where C: std::fmt::Display + Send + Sync + 'static, F: FnOnce() -> C { Ok(()) }
/// }
/// ```
///
/// `Error` is not `Clone`:
///
/// ```compile_fail
/// let a = report_err::err!("x");
/// let b = a.clone();
/// ```
#[cfg(doctest)]
pub struct CompileFail;

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn test_version_matches_manifest() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        // Pre-release and build metadata (`1.0.0-rc.1`, `+build`) follow the core.
        let core = VERSION.split(['-', '+']).next().unwrap_or_default();
        assert_eq!(core.split('.').count(), 3, "expected MAJOR.MINOR.PATCH");
        assert!(
            core.split('.').all(|part| part.parse::<u64>().is_ok()),
            "every version component is a number"
        );
    }
}

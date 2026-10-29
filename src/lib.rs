//! # report_err
//!
//! Dynamic errors for Rust applications. One type, `report_err::Error`,
//! holds any error, gathers context as it travels up the stack, and prints a
//! clear report when it reaches `main`. Code written for `anyhow` reads the
//! same: the migration is a change of crate name.
//!
//! On top of what `anyhow` does, an error can carry an error code (with its
//! severity, category, and exit code), a propagation trace of every place it
//! passed through, typed attachments read back by type, notes and
//! suggestions, several causes, and suppressed errors. A panic boundary turns
//! a caught panic into an error. All of it costs nothing until used: an error
//! that only carries its cause and location is one allocation and one machine
//! word, the same as `anyhow`.
//!
//! ## Tier 1
//!
//! ```
//! println!("report-err {}", report_err::VERSION);
//! ```
//!
//! The Tier 1 API arrives in `0.3.0`:
//!
//! ```text
//! use report_err::{bail, Context, Result};
//!
//! fn load_config(path: &str) -> Result<Config> {
//!     let text = std::fs::read_to_string(path)
//!         .with_context(|| format!("reading config file {path}"))?;
//!     if text.is_empty() {
//!         bail!("config file {path} is empty");
//!     }
//!     parse(&text).context("parsing config")
//! }
//!
//! fn main() -> Result<()> {
//!     let config = load_config("app.noml")?;
//!     run(config)
//! }
//! ```
//!
//! ## Status
//!
//! Design lock. The public API is written in `docs/API.md`, the design in
//! `dev/PLAN.md`, and the milestone schedule in `dev/ROADMAP.md` (all in the
//! [repository](https://github.com/jamesgober/report-err)). The core (the
//! `anyhow` surface) ships in `0.3.0`.

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

// An error lives on the heap, so the crate needs `alloc` even without `std`.
// The core types start using it in 0.3.0, and define-err's types in 0.4.0.
#[allow(unused_extern_crates)]
extern crate alloc;
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

/// Crate version string, populated by Cargo at build time.
///
/// ```
/// assert!(!report_err::VERSION.is_empty());
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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

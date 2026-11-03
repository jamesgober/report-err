//! Assertions for tests of code that returns `Error` (feature `testing`).
//! Each panics with the full report when the check fails, so a failing test
//! shows the whole error.
#![allow(clippy::panic)]

use crate::{Code, Error, SectionKind};

/// Panics unless some link of the chain contains `text`.
///
/// ```
/// use report_err::Context;
///
/// let error = std::fs::read("missing.noml")
///     .with_context(|| "loading missing.noml")
///     .unwrap_err();
/// report_err::testing::assert_chain_contains(&error, "missing.noml");
/// ```
#[track_caller]
pub fn assert_chain_contains(error: &Error, text: &str) {
    use alloc::string::ToString;
    if !error.chain().any(|link| link.to_string().contains(text)) {
        panic!("no link of the chain contains {text:?}:\n{error:?}");
    }
}

/// Panics unless the error's code is `code`.
///
/// ```
/// let code = define_err::code!("APP", 3);
/// report_err::testing::assert_code(&report_err::err!(code, "x"), code);
/// ```
#[track_caller]
pub fn assert_code(error: &Error, code: Code) {
    if error.code() != Some(code) {
        panic!(
            "expected code {code}, found {:?}:\n{error:?}",
            error.code().map(|c| alloc::format!("{c}"))
        );
    }
}

/// Panics unless the error has a section of `kind` whose text contains
/// `text`.
///
/// ```
/// use report_err::SectionKind;
///
/// let error = report_err::err!("x").suggestion("run the migration");
/// report_err::testing::assert_section(&error, SectionKind::Suggestion, "migration");
/// ```
#[track_caller]
pub fn assert_section(error: &Error, kind: SectionKind, text: &str) {
    if !error
        .sections()
        .any(|s| s.kind == kind && s.text.contains(text))
    {
        panic!("no {kind:?} section contains {text:?}:\n{error:?}");
    }
}

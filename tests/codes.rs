//! Codes, severity, category, kinds, and the public message (PLAN 7.17,
//! 12.4 E-50 to E-56). No catalog is configured in this process; the
//! catalog cases are in `tests/catalog.rs`.

use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;

use define_err::{code, define_errors};
use report_err::{bail, ensure, err, Category, Context, Error, Kinds, Result, Severity};

define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.")]
        #[category(Unavailable)]
        #[severity(WARNING)]
        #[kinds(AUDIT)]
        #[public("The database is busy.")]
        Timeout => "query timed out",

        #[code(2, name = CORRUPT, help = "Restore from a replica.")]
        #[category(DataLoss)]
        Corrupt { page: u32 } => "page {page} is corrupt",

        #[code(3, name = WRAPPED, help = "Look at the cause.")]
        Wrapped(#[source] source: Inner) => "the query failed",
    }
}

define_errors! {
    namespace = "NET";
    pub enum Inner {
        #[code(9, name = RESET, help = "Reconnect.")]
        #[category(Unavailable)]
        Reset => "connection reset",
    }
}

define_errors! {
    pub enum Uncoded {
        #[category(InvalidArgument)]
        BadInput => "bad input",
    }
}

define_errors! {
    namespace = "POOL";
    pub enum Busy {
        #[code(1, name = FULL, help = "Wait.")]
        #[category(ResourceExhausted)]
        Full { #[retry_after] wait: Duration } => "pool is full",
    }
}

#[derive(Debug)]
struct Plain;
impl fmt::Display for Plain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("plain")
    }
}
impl StdError for Plain {}

/// A foreign error with a coded error as its source.
#[derive(Debug)]
struct Foreign(DbError);
impl fmt::Display for Foreign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("foreign")
    }
}
impl StdError for Foreign {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.0)
    }
}

// ---------------------------------------------------------------- code

#[test]
fn test_no_code_anywhere_has_the_defaults() {
    // E-50
    let error = err!("x");
    assert_eq!(error.code(), None);
    assert_eq!(error.severity(), Severity::ERROR);
    assert_eq!(error.category(), Category::Unknown);
    assert_eq!(error.kinds(), Kinds::ERROR);
    assert_eq!(error.public_message(), None);
}

#[test]
fn test_coded_twice_the_latest_wins() {
    // E-51
    let (a, b) = (code!("APP", 1), code!("APP", 2));
    assert_eq!(err!("x").coded(a).coded(b).code(), Some(b));
}

#[test]
fn test_code_deep_in_the_sources_is_found() {
    // E-52
    let error = Error::new(Foreign(DbError::Timeout))
        .context("a")
        .context("b");
    assert_eq!(error.code(), Some(DbError::TIMEOUT));
    assert_eq!(error.category(), Category::Unavailable);
    assert_eq!(error.severity(), Severity::WARNING);
}

#[test]
fn test_innermost_of_two_codes_wins() {
    // E-53
    let error = Error::new(DbError::Wrapped(Inner::Reset));
    assert_eq!(error.code(), Some(Inner::RESET));
    assert_eq!(error.category(), Category::Unavailable);
}

#[test]
fn test_explicit_code_beats_the_chain() {
    // E-54: and the chain's metadata does not leak onto a different code.
    let mine = code!("APP", 5);
    let error = Error::new(DbError::Timeout).coded(mine);
    assert_eq!(error.code(), Some(mine));
    assert_eq!(error.severity(), Severity::ERROR);
    assert_eq!(error.category(), Category::Unknown);
}

#[test]
fn test_explicit_code_equal_to_the_chain_keeps_its_metadata() {
    let error = Error::new(DbError::Timeout).coded(DbError::TIMEOUT);
    assert_eq!(error.severity(), Severity::WARNING);
    assert_eq!(error.category(), Category::Unavailable);
}

#[test]
fn test_foreign_chain_identifies_nothing() {
    // E-55
    let error = Error::new(Plain).context("a");
    assert_eq!(error.code(), None);
    assert_eq!(define_err::identify(&*error), None);
}

#[test]
fn test_option_coded_without_a_catalog_says_the_code() {
    // E-56
    let code = code!("USER", 4);
    let error = None::<u8>.coded(code).unwrap_err();
    assert_eq!(error.to_string(), "USER-0004");
    assert_eq!(error.code(), Some(code));
    assert_eq!(Some(1).coded(code).unwrap(), 1);
}

#[test]
fn test_result_coded_keeps_the_error_and_ok() {
    let code = code!("USER", 4);
    let failed: std::result::Result<(), Plain> = Err(Plain);
    let error = failed.coded(code).unwrap_err();
    assert!(error.is::<Plain>());
    assert_eq!(error.code(), Some(code));
    let fine: std::result::Result<u8, Plain> = Ok(1);
    assert_eq!(fine.coded(code).unwrap(), 1);
}

#[test]
fn test_code_survives_context_and_conversion() {
    fn inner() -> Result<()> {
        Err(DbError::Timeout)?;
        Ok(())
    }
    let error = inner().context("outer").unwrap_err();
    assert_eq!(error.code(), Some(DbError::TIMEOUT));
    assert!(error.is::<DbError>());
}

// ---------------------------------------------------------------- macros

#[test]
fn test_code_first_macros_set_the_code() {
    let code = code!("APP", 1);
    let port = 80;
    assert_eq!(err!(code, "plain").code(), Some(code));
    assert_eq!(err!(code, "plain").to_string(), "plain");
    assert_eq!(err!(code, "port {port}").to_string(), "port 80");
    assert_eq!(err!(code, "port {}", port).to_string(), "port 80");
    assert_eq!(err!(code, "trailing",).code(), Some(code));
    assert_eq!(report_err::format_err!(code, "x").code(), Some(code));

    fn bails(code: report_err::Code) -> Result<()> {
        bail!(code, "n = {}", 1);
    }
    assert_eq!(bails(code).unwrap_err().to_string(), "n = 1");
    assert_eq!(bails(code).unwrap_err().code(), Some(code));

    fn ensures(n: u8, code: report_err::Code) -> Result<()> {
        ensure!(n > 0, code, "n is {n}");
        Ok(())
    }
    assert!(ensures(1, code).is_ok());
    assert_eq!(ensures(0, code).unwrap_err().to_string(), "n is 0");
    assert_eq!(ensures(0, code).unwrap_err().code(), Some(code));
}

#[test]
fn test_code_first_macro_records_the_call_site() {
    let line = line!() + 1;
    let error = err!(code!("APP", 1), "x");
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().file(), file!());
}

#[test]
fn test_macro_with_constant_code() {
    let error = err!(DbError::TIMEOUT, "no reply");
    assert_eq!(error.code(), Some(DbError::TIMEOUT));
}

// ---------------------------------------------------------------- metadata

#[test]
fn test_severity_override_beats_the_code() {
    let error = Error::new(DbError::Timeout).with_severity(Severity::CRITICAL);
    assert_eq!(error.severity(), Severity::CRITICAL);
    assert_eq!(error.code(), Some(DbError::TIMEOUT));
}

#[test]
fn test_severity_falls_to_the_category_default() {
    let error = Error::new(DbError::Corrupt { page: 1 });
    assert_eq!(error.severity(), Category::DataLoss.default_severity());
}

#[test]
fn test_kinds_combine_code_and_added() {
    let error = Error::new(DbError::Timeout).with_kinds(Kinds::SECURITY);
    assert!(error.kinds().contains(Kinds::SECURITY));
    assert!(error.kinds().contains(Kinds::AUDIT));
    let added = err!("x").with_kinds(Kinds::USER).with_kinds(Kinds::AUDIT);
    assert_eq!(added.kinds(), Kinds::USER | Kinds::AUDIT);
}

#[cfg(feature = "std")]
#[test]
fn test_io_root_category_from_its_kind() {
    let error = Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    assert_eq!(error.category(), Category::PermissionDenied);
    assert_eq!(
        error.severity(),
        Category::PermissionDenied.default_severity()
    );
    assert_eq!(error.code(), None);
    assert_eq!(error.public_message(), None);
}

#[test]
fn test_uncoded_defined_error_keeps_its_category() {
    let error = Error::new(Uncoded::BadInput).context("a");
    assert_eq!(error.code(), None);
    assert_eq!(error.category(), Category::InvalidArgument);
    assert_eq!(error.exit_code(), 64);
    assert_eq!(error.public_message(), None);
}

// ---------------------------------------------------------------- public

#[test]
fn test_public_message_never_comes_from_display() {
    // D-015
    let secret = "password hunter2 rejected";
    let error = err!(code!("APP", 1), "{secret}");
    let public = error.public_message().unwrap();
    assert!(!public.contains("hunter2"));
    assert_eq!(public, Category::Unknown.default_public_message());
}

#[test]
fn test_public_message_with_a_category_from_the_code() {
    let error = Error::new(DbError::Corrupt { page: 7 });
    assert_eq!(
        error.public_message(),
        Some(Category::DataLoss.default_public_message())
    );
}

#[test]
fn test_public_message_from_info_uses_the_variant_text() {
    let error = Error::from_info(DbError::Timeout).context("loading a user");
    assert_eq!(error.public_message(), Some("The database is busy."));
}

/// The probe's public text is never trusted (D-075): any `description()` can
/// carry a probe, so a record takes the variant's text only from a catalog
/// entry or a `from_info` root.
#[test]
fn test_record_public_text_never_comes_from_the_probe() {
    let error = Error::new(DbError::Timeout).context("loading a user");
    let category = Category::Unavailable.default_public_message();
    assert_eq!(error.public_message(), Some(category));
    assert_eq!(error.record().public, category);
    let error = Error::from_info(DbError::Timeout).context("loading a user");
    assert_eq!(error.record().public, "The database is busy.");
}

#[test]
fn test_from_info_with_another_code_does_not_answer_for_it() {
    let error = Error::from_info(DbError::Timeout).coded(code!("APP", 1));
    assert_eq!(
        error.public_message(),
        Some(Category::Unknown.default_public_message())
    );
    assert!(error.info().is_some());
}

// ---------------------------------------------------------------- from_info

#[test]
fn test_from_info_keeps_per_value_metadata() {
    let wait = Duration::from_millis(250);
    let error = Error::from_info(Busy::Full { wait })
        .context("a")
        .context("b");
    assert_eq!(error.info().unwrap().retry_after(), Some(wait));
    assert_eq!(error.code(), Some(Busy::FULL));
    assert_eq!(error.category(), Category::ResourceExhausted);
    assert!(error.is::<Busy>());
    assert!(matches!(error.downcast::<Busy>(), Ok(Busy::Full { wait: w }) if w == wait));
}

#[test]
fn test_info_is_none_for_other_constructors() {
    assert!(Error::new(DbError::Timeout).info().is_none());
    assert!(err!("x").info().is_none());
    assert!(Error::msg("x").info().is_none());
}

// ---------------------------------------------------------------- exit

#[test]
fn test_exit_code_order() {
    // E-93, the part that needs no process.
    assert_eq!(err!("x").exit_code(), 1);
    assert_eq!(Error::new(Plain).exit_code(), 1);
    assert_eq!(err!(code!("APP", 1), "x").exit_code(), 1);
    assert_eq!(Error::new(DbError::Timeout).exit_code(), 69);
    assert_eq!(
        Error::new(DbError::Timeout).with_exit_code(3).exit_code(),
        3
    );
    #[cfg(feature = "std")]
    assert_eq!(
        Error::new(std::io::Error::from(std::io::ErrorKind::NotFound)).exit_code(),
        66
    );
    assert_eq!(err!("x").with_exit_code(0).exit_code(), 1);
    assert_eq!(err!("x").with_exit_code(255).exit_code(), 255);
}

// ---------------------------------------------------------------- report

#[test]
fn test_report_has_the_code_block_before_the_location() {
    let error = Error::new(DbError::Timeout).context("loading a user");
    let text = format!("{error:?}");
    let code = text.find("Code: DB-0001 (unavailable, severity").unwrap();
    let location = text.find("Location: ").unwrap();
    assert!(code < location);
    assert!(text.contains(&format!("severity {})", Severity::WARNING.get())));
}

#[test]
fn test_report_has_no_code_block_without_a_code() {
    let text = format!("{:?}", err!("x"));
    assert!(!text.contains("Code:"));
    assert!(text.contains("\n\nLocation: "));
}

#[test]
fn test_report_code_line_uses_a_group_of_its_own() {
    let text = format!("{:?}", err!(code!("APP", 1), "x"));
    assert!(text.starts_with("x\n\nCode: APP-0001 ("), "{text}");
    assert!(text.contains(")\nLocation: "), "{text}");
}

// ---------------------------------------------------------------- reexports

#[test]
fn test_reexports_are_the_define_err_types() {
    fn same<T>(_: T, _: T) {}
    same(Severity::ERROR, define_err::Severity::ERROR);
    same(Category::Unknown, define_err::Category::Unknown);
    same(Kinds::ERROR, define_err::Kinds::ERROR);
    same(
        code!("A", 1),
        report_err::Code::from_u64(code!("A", 1).as_u64()).unwrap(),
    );
}

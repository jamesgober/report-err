//! `configure_catalogs` sets process state once, so this binary holds one
//! test (PLAN 7.18, invariant I-14, E-56).

use define_err::{code, define_errors, CodeInfo};
use report_err::{configure_catalogs, err, Category, ConfigureError, Context, Severity};

define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.", title = "Query timed out")]
        #[category(Unavailable)]
        #[severity(WARNING)]
        #[kinds(AUDIT)]
        #[public("The database is busy.")]
        Timeout => "query timed out",
    }
}

static CATALOGS: &[&[CodeInfo]] = &[DbError::CATALOG];
static OTHER: &[&[CodeInfo]] = &[];

#[test]
fn test_catalogs_are_set_once_and_answer_for_explicit_codes() {
    // Before: an explicit code knows nothing.
    let before = err!("no reply").coded(DbError::TIMEOUT);
    assert_eq!(before.category(), Category::Unknown);
    assert_eq!(before.severity(), Severity::ERROR);

    assert_eq!(configure_catalogs(CATALOGS), Ok(()));
    assert_eq!(
        configure_catalogs(OTHER),
        Err(ConfigureError::AlreadyConfigured)
    );

    // After: the same code finds its entry, and the first value stayed.
    let error = err!("no reply").coded(DbError::TIMEOUT);
    assert_eq!(error.category(), Category::Unavailable);
    assert_eq!(error.severity(), Severity::WARNING);
    assert!(error.kinds().contains(define_err::Kinds::AUDIT));
    assert_eq!(error.public_message(), Some("The database is busy."));
    assert_eq!(error.exit_code(), 69);
    assert!(format!("{error:?}").contains("Code: DB-0001 (unavailable, "));

    // E-56: the title on `None`, and the code itself when no catalog names it.
    let none = None::<u8>.coded(DbError::TIMEOUT).unwrap_err();
    assert_eq!(none.to_string(), "Query timed out");
    let missing = code!("DB", 2);
    assert_eq!(
        None::<u8>.coded(missing).unwrap_err().to_string(),
        "DB-0002"
    );
}

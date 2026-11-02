//! Codes from define-err: found in the chain, set by hand, and what a client
//! may see.

use report_err::{bail, err, Context, Error, Result};

define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.", title = "Query timed out")]
        #[category(Unavailable)]
        #[public("The database is busy. Try again shortly.")]
        Timeout { shard: u32 } => "query against shard {shard} timed out",
    }
}

const NOT_ALLOWED: report_err::Code = define_err::code!("APP", 12);

fn query() -> Result<()> {
    Err(DbError::Timeout { shard: 3 }).context("loading the user")
}

fn check(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!(NOT_ALLOWED, "an empty name for {name:?}");
    }
    Ok(())
}

fn main() {
    // Optional: lets a code find its catalog entry, public text included.
    static CATALOGS: &[&[define_err::CodeInfo]] = &[DbError::CATALOG];
    report_err::configure_catalogs(CATALOGS).unwrap();

    let error: Error = query().unwrap_err();
    assert_eq!(error.code(), Some(DbError::TIMEOUT));
    println!("code     {:?}", error.code().map(|c| c.to_string()));
    println!("category {}", error.category().name());
    println!("severity {}", error.severity().get());
    println!("exit     {}", error.exit_code());
    println!("public   {:?}", error.public_message());
    println!("log      {error:#}");
    println!();
    println!("{error:?}");
    println!();

    let error = check("").unwrap_err();
    println!("{error:?}");

    let error = err!("no reply").coded(DbError::TIMEOUT);
    println!("\ncode only: {:?}", error.public_message());
}

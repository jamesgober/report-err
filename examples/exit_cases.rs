//! A `main` for each exit-code case: `tests/exit.rs` runs this binary and
//! checks its status and standard error (PLAN 12.4 E-93, E-94).
//!
//! ```text
//! cargo run --example exit_cases -- coded; echo $?
//! ```

use std::io;

use report_err::{err, Error, ExitReport};

define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.")]
        #[category(Unavailable)]
        Timeout => "query timed out",
    }
}

fn run(case: &str) -> report_err::Result<()> {
    match case {
        "ok" => Ok(()),
        "plain" => Err(err!("something failed")),
        "io" => Err(Error::new(io::Error::from(io::ErrorKind::NotFound))),
        "coded" => Err(Error::new(DbError::Timeout)),
        "zero" => Err(err!("x").with_exit_code(0)),
        "max" => Err(err!("x").with_exit_code(255)),
        "escape" => Err(err!("a\u{1b}[31mb\u{202e}c\n\nLocation: forged")),
        "huge" => Err(err!("{}", "x".repeat(2 * 1024 * 1024)).with_exit_code(9)),
        other => Err(err!("unknown case {other}")),
    }
}

fn main() -> ExitReport {
    let case = std::env::args().nth(1).unwrap_or_default();
    if case == "converted" {
        // The error is made by `into`, here, so this line is its location.
        return std::fs::read_to_string("no-such-file").map(drop).into();
    }
    run(&case).into()
}

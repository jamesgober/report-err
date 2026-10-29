//! Looking inside an error: downcasting, the cause chain, and the `source()`
//! chain through `&dyn Error`.
//!
//! Run: `cargo run --example inspect`

use std::error::Error as StdError;
use std::io;

use report_err::{Context, Result};

fn read(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {path}"))
}

fn start() -> Result<String> {
    read("no/such/settings.conf").context("starting the service")
}

/// Any library that takes `&dyn Error` sees every layer.
fn log_chain(error: &(dyn StdError + 'static)) {
    let mut cause = Some(error);
    while let Some(c) = cause {
        println!("  - {c}");
        cause = c.source();
    }
}

fn main() {
    let Err(error) = start() else { return };

    println!("{{}}   {error}");
    println!("{{:#}} {error:#}");
    println!("{{:?}}\n{error:?}\n");

    if let Some(io) = error.downcast_ref::<io::Error>() {
        println!("the root is an io::Error of kind {:?}", io.kind());
    }
    for (index, cause) in error.chain().enumerate() {
        println!("chain[{index}] = {cause}");
    }
    println!("as &dyn Error:");
    log_chain(&*error);
}

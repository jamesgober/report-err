//! `main` that exits with a code chosen from the error.
//!
//! Run it with a path that does not exist; it exits with 66, the code of
//! `Category::NotFound`.

use report_err::{Context, ExitReport, Result};

fn run(path: &str) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    println!("{} bytes", text.len());
    Ok(())
}

fn main() -> ExitReport {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "no-such-file".into());
    run(&path).into()
}

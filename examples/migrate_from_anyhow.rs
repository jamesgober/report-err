//! Code written for anyhow, moved to report-err by changing the crate name
//! (and `anyhow!` to `err!`). Nothing else changes.
//!
//! Run: `cargo run --example migrate_from_anyhow`

use report_err::{bail, err, Context, Result};

fn find_entry(dir: Option<&str>) -> Result<String> {
    // Was: dir.ok_or_else(|| anyhow::anyhow!("entry file has no parent directory"))?
    let dir = dir.ok_or_else(|| err!("entry file has no parent directory"))?;
    // The shorter form report-err and anyhow both accept:
    let _same = Some(dir).context("entry file has no parent directory")?;
    if dir.is_empty() {
        bail!("empty directory name");
    }
    Ok(format!("{dir}/main.kr"))
}

fn lower(source: &str) -> Result<usize> {
    // `.map_err(|e| anyhow!(e))` for a value that is not a std error:
    let parsed: std::result::Result<usize, String> = if source.is_empty() {
        Err(String::from("empty program"))
    } else {
        Ok(source.len())
    };
    parsed.map_err(|e| err!("IR lowering failed: {e}"))
}

fn main() -> Result<()> {
    println!("{}", find_entry(Some("src"))?);
    println!("{}", lower("fn main() {}")?);
    match lower("") {
        Err(error) => println!("{error:#}"),
        Ok(_) => println!("unexpected"),
    }
    Ok(())
}

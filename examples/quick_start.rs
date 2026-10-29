//! The Tier 1 API: `Result`, `?`, `context`, `bail!`, `ensure!`.
//!
//! Run: `cargo run --example quick_start` (it fails on purpose, to show the
//! report `main` prints).

use report_err::{bail, ensure, Context, Result};

struct Config {
    port: u16,
}

fn parse(text: &str) -> Result<Config> {
    let port = text.trim().parse().context("the port is not a number")?;
    Ok(Config { port })
}

fn load_config(path: &str) -> Result<Config> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading config file {path}"))?;
    ensure!(!text.is_empty(), "config file {path} is empty");
    let config = parse(&text).context("parsing config")?;
    if config.port == 0 {
        bail!("port 0 is not allowed");
    }
    Ok(config)
}

fn main() -> Result<()> {
    let config = load_config("app.conf")?;
    println!("listening on port {}", config.port);
    Ok(())
}

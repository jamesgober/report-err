//! A panic inside `boundary` becomes an error when panics unwind; under
//! `panic = "abort"` the process aborts at the panic instead.
//!
//! ```text
//! cargo run --example panic_boundary
//! RUSTFLAGS="-C panic=abort" cargo run --example panic_boundary
//! ```

use report_err::{boundary, Context, Result, PANICS_CAUGHT};

fn render_page(page: u32) -> Result<String> {
    if page == 0 {
        panic!("page 0 has no layout");
    }
    Ok(format!("page {page}"))
}

fn main() {
    println!("PANICS_CAUGHT={PANICS_CAUGHT}");
    match boundary(|| render_page(0)).context("plugin `gallery` failed") {
        Ok(text) => println!("{text}"),
        Err(error) => println!("caught: {error:#} (exit code {})", error.exit_code()),
    }
}

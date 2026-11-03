//! Typed attachments, sections, and the propagation trail.

use report_err::{Context, Result};

#[derive(Debug, PartialEq)]
struct RequestId(u64);

fn lowest() -> Result<()> {
    Err(report_err::err!("connection reset"))
        .attach(RequestId(7))
        .note("the pool was full")
        .trace()
}

fn middle() -> Result<()> {
    lowest().context("sending the query").trace()
}

fn main() {
    let error = middle().unwrap_err();

    // Code reads a value back by its type.
    assert_eq!(error.attachment::<RequestId>(), Some(&RequestId(7)));

    for step in error.trail() {
        println!("{:?} at line {}", step.kind, step.location.line());
    }
    println!();

    // The report prints sections and the trail.
    println!("{error:?}");
}

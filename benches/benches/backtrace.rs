//! B14: creating an error with backtraces enabled (PLAN 11.1). std reads the
//! variables once per process, so this file turns them on before anything
//! captures, and is a binary of its own.

use std::hint::black_box;
use std::io;

use criterion::{criterion_group, criterion_main, Criterion};

fn enabled(c: &mut Criterion) {
    // Before the first capture in this process; edition 2021, one thread.
    std::env::set_var("RUST_LIB_BACKTRACE", "1");
    let probe = std::backtrace::Backtrace::capture();
    assert_eq!(probe.status(), std::backtrace::BacktraceStatus::Captured);

    let mut g = c.benchmark_group("backtrace");
    g.bench_function("B14 create, backtraces on/report-err", |b| {
        b.iter(|| {
            black_box(report_err::Error::new(io::Error::from(
                io::ErrorKind::NotFound,
            )))
        })
    });
    g.bench_function("B14 create, backtraces on/anyhow", |b| {
        b.iter(|| black_box(anyhow::Error::new(io::Error::from(io::ErrorKind::NotFound))))
    });
    g.bench_function("B14 create, backtraces on/eyre", |b| {
        b.iter(|| black_box(eyre::Report::new(io::Error::from(io::ErrorKind::NotFound))))
    });
    g.finish();
}

criterion_group!(benches, enabled);
criterion_main!(benches);

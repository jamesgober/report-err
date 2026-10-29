//! report-err against anyhow on the 0.3 surface (PLAN 11.1, B2 to B13), and
//! the per-link cost of a hand-written `source()` walk (D-011).
//!
//! Backtraces must be off for a fair comparison: run with `RUST_BACKTRACE`
//! and `RUST_LIB_BACKTRACE` unset.

use std::error::Error as StdError;
use std::fmt::{self, Write};
use std::hint::black_box;
use std::io;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

/// A writer that keeps nothing, so formatting measures only formatting.
struct Sink(usize);
impl Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

fn io_error() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

macro_rules! library {
    ($name:ident, $krate:ident, $adhoc:ident) => {
        mod $name {
            use super::*;
            use ::$krate::{Context, Result};

            #[inline(never)]
            pub fn question_mark() -> Result<()> {
                Err(io_error())?;
                Ok(())
            }

            #[inline(never)]
            pub fn literal() -> ::$krate::Error {
                ::$krate::$adhoc!("no input files")
            }

            #[inline(never)]
            pub fn formatted(x: u32) -> ::$krate::Error {
                ::$krate::$adhoc!("value {x} is out of range")
            }

            #[inline(never)]
            pub fn context_on_result() -> Result<()> {
                Err(io_error()).context("reading the config")?;
                Ok(())
            }

            #[inline(never)]
            pub fn context_after_question_mark() -> Result<()> {
                question_mark().context("reading the config")?;
                Ok(())
            }

            #[inline(never)]
            pub fn layers(n: usize) -> ::$krate::Error {
                let mut error = ::$krate::Error::new(io_error());
                for _ in 0..n {
                    error = error.context("while doing a thing");
                }
                error
            }

            #[inline(never)]
            pub fn ok_5(n: u64) -> Result<u64> {
                fn a(n: u64) -> Result<u64> {
                    if n == u64::MAX {
                        Err(io_error())?;
                    }
                    Ok(n + 1)
                }
                fn b(n: u64) -> Result<u64> {
                    Ok(a(n)? + 1)
                }
                fn c(n: u64) -> Result<u64> {
                    Ok(b(n)? + 1)
                }
                fn d(n: u64) -> Result<u64> {
                    Ok(c(n)? + 1)
                }
                Ok(d(n)? + 1)
            }
        }
    };
}

library!(ours, report_err, err);
library!(theirs, anyhow, anyhow);

fn walk(error: &(dyn StdError + 'static)) -> usize {
    let mut n = 1;
    let mut cur = error.source();
    while let Some(link) = cur {
        n += 1;
        cur = link.source();
    }
    n
}

fn create(c: &mut Criterion) {
    let mut g = c.benchmark_group("create");
    g.bench_function("B3 question mark/report-err", |b| {
        b.iter(|| black_box(ours::question_mark()))
    });
    g.bench_function("B3 question mark/anyhow", |b| {
        b.iter(|| black_box(theirs::question_mark()))
    });
    g.bench_function("B4 literal/report-err", |b| {
        b.iter(|| black_box(ours::literal()))
    });
    g.bench_function("B4 literal/anyhow", |b| {
        b.iter(|| black_box(theirs::literal()))
    });
    g.bench_function("B5 formatted/report-err", |b| {
        b.iter(|| black_box(ours::formatted(black_box(7))))
    });
    g.bench_function("B5 formatted/anyhow", |b| {
        b.iter(|| black_box(theirs::formatted(black_box(7))))
    });
    g.finish();
}

fn context(c: &mut Criterion) {
    let mut g = c.benchmark_group("context");
    g.bench_function("B6 context on result/report-err", |b| {
        b.iter(|| black_box(ours::context_on_result()))
    });
    g.bench_function("B6 context on result/anyhow", |b| {
        b.iter(|| black_box(theirs::context_on_result()))
    });
    g.bench_function("B7 context after ?/report-err", |b| {
        b.iter(|| black_box(ours::context_after_question_mark()))
    });
    g.bench_function("B7 context after ?/anyhow", |b| {
        b.iter(|| black_box(theirs::context_after_question_mark()))
    });
    for n in [3usize, 5, 9] {
        g.bench_with_input(
            BenchmarkId::new("B8 create + layers/report-err", n),
            &n,
            |b, &n| b.iter(|| black_box(ours::layers(black_box(n)))),
        );
        g.bench_with_input(
            BenchmarkId::new("B8 create + layers/anyhow", n),
            &n,
            |b, &n| b.iter(|| black_box(theirs::layers(black_box(n)))),
        );
    }
    g.bench_function("B9 drop of 3 layers/report-err", |b| {
        b.iter_batched(|| ours::layers(3), drop, BatchSize::SmallInput)
    });
    g.bench_function("B9 drop of 3 layers/anyhow", |b| {
        b.iter_batched(|| theirs::layers(3), drop, BatchSize::SmallInput)
    });
    g.finish();
}

fn inspect(c: &mut Criterion) {
    let mut g = c.benchmark_group("inspect");
    let r3 = ours::layers(3);
    let a3 = theirs::layers(3);
    g.bench_function("B10 downcast root after 3/report-err", |b| {
        b.iter(|| black_box(black_box(&r3).downcast_ref::<io::Error>().is_some()))
    });
    g.bench_function("B10 downcast root after 3/anyhow", |b| {
        b.iter(|| black_box(black_box(&a3).downcast_ref::<io::Error>().is_some()))
    });
    for n in [1usize, 3, 5, 9] {
        let r = ours::layers(n);
        let a = theirs::layers(n);
        g.bench_with_input(BenchmarkId::new("source walk/report-err", n), &n, |b, _| {
            b.iter(|| black_box(walk(black_box(&*r))))
        });
        g.bench_with_input(BenchmarkId::new("source walk/anyhow", n), &n, |b, _| {
            b.iter(|| black_box(walk(black_box(a.as_ref()))))
        });
        g.bench_with_input(BenchmarkId::new("chain/report-err", n), &n, |b, _| {
            b.iter(|| black_box(black_box(&r).chain().count()))
        });
        g.bench_with_input(BenchmarkId::new("chain/anyhow", n), &n, |b, _| {
            b.iter(|| black_box(black_box(&a).chain().count()))
        });
    }
    g.finish();
}

fn format(c: &mut Criterion) {
    let mut g = c.benchmark_group("format");
    let r = ours::layers(3);
    let a = theirs::layers(3);
    g.bench_function("B11 display/report-err", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&r)))
    });
    g.bench_function("B11 display/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&a)))
    });
    g.bench_function("B12 alternate/report-err", |b| {
        b.iter(|| write!(Sink(0), "{:#}", black_box(&r)))
    });
    g.bench_function("B12 alternate/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{:#}", black_box(&a)))
    });
    g.bench_function("B13 report/report-err", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&r)))
    });
    g.bench_function("B13 report/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&a)))
    });
    g.finish();
}

fn success(c: &mut Criterion) {
    let mut g = c.benchmark_group("success");
    g.bench_function("B2 5-deep ok/report-err", |b| {
        b.iter(|| black_box(ours::ok_5(black_box(1))))
    });
    g.bench_function("B2 5-deep ok/anyhow", |b| {
        b.iter(|| black_box(theirs::ok_5(black_box(1))))
    });
    g.finish();
}

criterion_group!(benches, create, context, inspect, format, success);
criterion_main!(benches);

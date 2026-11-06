//! What only report-err does, against the nearest plain alternative (PLAN
//! 11.1, B15 to B18), and what codes cost. Backtraces off.

use std::hint::black_box;
use std::io;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use report_err::{err, CollectAll, Error};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, help = "Raise the timeout.")]
        #[category(Unavailable)]
        Timeout => "query timed out",
    }
}

fn codes(c: &mut Criterion) {
    let mut g = c.benchmark_group("codes");
    let code = define_err::code!("APP", 1);
    let found = Error::new(DbError::Timeout)
        .context("a")
        .context("b")
        .context("c");
    let set = err!("x").coded(code);
    g.bench_function("coded literal", |b| {
        b.iter(|| black_box(err!(black_box(code), "x")))
    });
    g.bench_function("code set by hand", |b| {
        b.iter(|| black_box(black_box(&set).code()))
    });
    g.bench_function("code found after 3", |b| {
        b.iter(|| black_box(black_box(&found).code()))
    });
    g.bench_function("severity found after 3", |b| {
        b.iter(|| black_box(black_box(&found).severity()))
    });
    g.finish();
}

fn hook(c: &mut Criterion) {
    // B15: what an error pays when no creation hook is installed.
    let mut g = c.benchmark_group("hook");
    g.bench_function("B15 hook slot load", |b| {
        b.iter(|| black_box(define_err::creation_hook().is_none()))
    });
    g.bench_function("B15 report with no reporter", |b| {
        b.iter_batched(
            || err!("x"),
            |e| black_box(e.report()),
            criterion::BatchSize::SmallInput,
        )
    });
    g.finish();
}

fn parse(text: &str) -> Result<u32, std::num::ParseIntError> {
    text.parse()
}

fn collect(c: &mut Criterion) {
    // B16: 10,000 items, 1% failing.
    let items: Vec<String> = (0..10_000)
        .map(|i| {
            if i % 100 == 7 {
                "x".to_string()
            } else {
                i.to_string()
            }
        })
        .collect();
    let mut g = c.benchmark_group("collect");
    g.bench_function("B16 collect_all, 1% failing", |b| {
        b.iter(|| {
            let all: report_err::Result<Vec<u32>> =
                black_box(&items).iter().map(|s| parse(s)).collect_all();
            black_box(all.is_err())
        })
    });
    g.bench_function("B16 std collect, stops at the first", |b| {
        b.iter(|| {
            let first: Result<Vec<u32>, _> = black_box(&items).iter().map(|s| parse(s)).collect();
            black_box(first.is_err())
        })
    });
    let good: Vec<String> = (0..10_000).map(|i| i.to_string()).collect();
    g.bench_function("B16 collect_all, none failing", |b| {
        b.iter(|| {
            let all: report_err::Result<Vec<u32>> =
                black_box(&good).iter().map(|s| parse(s)).collect_all();
            black_box(all.is_ok())
        })
    });
    g.bench_function("B16 std collect, none failing", |b| {
        b.iter(|| {
            let all: Result<Vec<u32>, _> = black_box(&good).iter().map(|s| parse(s)).collect();
            black_box(all.is_ok())
        })
    });
    g.finish();
}

#[inline(never)]
fn work(n: u64) -> report_err::Result<u64> {
    Ok(black_box(n).wrapping_mul(31))
}

fn boundary(c: &mut Criterion) {
    // B17: `boundary` with no panic, over a plain call.
    let mut g = c.benchmark_group("boundary");
    g.bench_function("B17 plain call", |b| {
        b.iter(|| black_box(work(black_box(3))))
    });
    g.bench_function("B17 boundary", |b| {
        b.iter(|| black_box(report_err::boundary(|| work(black_box(3)))))
    });
    g.finish();
}

/// Creates and drops `per_thread` errors on each of `threads` threads; the
/// time from the first start to the last finish.
fn contended<F: Fn() + Sync>(threads: usize, per_thread: u64, make: F) -> Duration {
    let start = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                for _ in 0..per_thread {
                    make();
                }
            });
        }
    });
    start.elapsed()
}

fn contention(c: &mut Criterion) {
    // B18: 64 threads creating and dropping errors.
    let mut g = c.benchmark_group(if cfg!(feature = "mimalloc") {
        "contention (mimalloc)"
    } else {
        "contention (system)"
    });
    g.sample_size(10);
    g.bench_function("B18 64 threads/report-err", |b| {
        b.iter_custom(|iters| {
            contended(64, iters, || {
                drop(black_box(Error::new(io::Error::from(
                    io::ErrorKind::NotFound,
                ))))
            })
        })
    });
    g.bench_function("B18 64 threads/anyhow", |b| {
        b.iter_custom(|iters| {
            contended(64, iters, || {
                drop(black_box(anyhow::Error::new(io::Error::from(
                    io::ErrorKind::NotFound,
                ))))
            })
        })
    });
    g.finish();
}

criterion_group!(benches, codes, hook, collect, boundary, contention);
criterion_main!(benches);

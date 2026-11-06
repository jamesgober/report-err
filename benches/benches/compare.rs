//! report-err against anyhow, eyre, error-stack, and `Box<dyn Error>` on the
//! common operations (PLAN 11.1, B2 to B13), and the per-link cost of a
//! hand-written `source()` walk (D-011).
//!
//! Each library is written the way its users write it. Backtraces must be
//! off for a fair comparison: run with `RUST_BACKTRACE` and
//! `RUST_LIB_BACKTRACE` unset (`backtrace.rs` measures them on).

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

const LAYER: &str = "while doing a thing";

/// The libraries with anyhow's shape: `Error::new`, `.context`, an ad hoc
/// macro.
macro_rules! anyhow_shaped {
    ($name:ident, $krate:ident, $adhoc:ident) => {
        mod $name {
            use super::*;
            use ::$krate::{Context, Result};

            pub type E = ::$krate::Error;

            #[inline(never)]
            pub fn question_mark() -> Result<()> {
                Err(io_error())?;
                Ok(())
            }

            #[inline(never)]
            pub fn literal() -> E {
                ::$krate::$adhoc!("no input files")
            }

            #[inline(never)]
            pub fn formatted(x: u32) -> E {
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
            pub fn layers(n: usize) -> E {
                let mut error = E::new(io_error());
                for _ in 0..n {
                    error = error.context(LAYER);
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

            pub fn downcast(e: &E) -> bool {
                e.downcast_ref::<io::Error>().is_some()
            }
        }
    };
}

anyhow_shaped!(ours, report_err, err);
anyhow_shaped!(anyhow_lib, anyhow, anyhow);

mod eyre_lib {
    use super::*;
    use eyre::{Result, WrapErr};

    pub type E = eyre::Report;

    #[inline(never)]
    pub fn question_mark() -> Result<()> {
        Err(io_error())?;
        Ok(())
    }

    #[inline(never)]
    pub fn literal() -> E {
        eyre::eyre!("no input files")
    }

    #[inline(never)]
    pub fn formatted(x: u32) -> E {
        eyre::eyre!("value {x} is out of range")
    }

    #[inline(never)]
    pub fn context_on_result() -> Result<()> {
        Err(io_error()).wrap_err("reading the config")?;
        Ok(())
    }

    #[inline(never)]
    pub fn context_after_question_mark() -> Result<()> {
        question_mark().wrap_err("reading the config")?;
        Ok(())
    }

    #[inline(never)]
    pub fn layers(n: usize) -> E {
        let mut error = E::new(io_error());
        for _ in 0..n {
            error = error.wrap_err(LAYER);
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

    pub fn downcast(e: &E) -> bool {
        e.downcast_ref::<io::Error>().is_some()
    }
}

/// error-stack: a report is typed by its current context; text layers are
/// attachments.
mod stack_lib {
    use super::*;
    use error_stack::{Report, ResultExt};

    #[derive(Debug)]
    pub struct Message(pub String);
    impl fmt::Display for Message {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl StdError for Message {}

    #[derive(Debug)]
    pub struct Literal(pub &'static str);
    impl fmt::Display for Literal {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }
    impl StdError for Literal {}

    pub type E = Report<io::Error>;
    type Result<T> = core::result::Result<T, E>;

    #[inline(never)]
    pub fn question_mark() -> Result<()> {
        Err(io_error())?;
        Ok(())
    }

    #[inline(never)]
    pub fn literal() -> Report<Literal> {
        Report::new(Literal("no input files"))
    }

    #[inline(never)]
    pub fn formatted(x: u32) -> Report<Message> {
        Report::new(Message(format!("value {x} is out of range")))
    }

    #[inline(never)]
    pub fn context_on_result() -> Result<()> {
        Err(io_error()).attach("reading the config")?;
        Ok(())
    }

    #[inline(never)]
    pub fn context_after_question_mark() -> Result<()> {
        question_mark().attach("reading the config")?;
        Ok(())
    }

    #[inline(never)]
    pub fn layers(n: usize) -> E {
        let mut error = Report::new(io_error());
        for _ in 0..n {
            error = error.attach(LAYER);
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

    pub fn downcast(e: &E) -> bool {
        e.downcast_ref::<io::Error>().is_some()
    }
}

/// The floor: no context, no location, two words.
mod boxed_lib {
    use super::*;

    pub type E = Box<dyn StdError + Send + Sync>;
    type Result<T> = core::result::Result<T, E>;

    #[inline(never)]
    pub fn question_mark() -> Result<()> {
        Err(io_error())?;
        Ok(())
    }

    #[inline(never)]
    pub fn literal() -> E {
        E::from("no input files")
    }

    #[inline(never)]
    pub fn formatted(x: u32) -> E {
        E::from(format!("value {x} is out of range"))
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

    pub fn downcast(e: &E) -> bool {
        e.downcast_ref::<io::Error>().is_some()
    }
}

fn walk(error: &(dyn StdError + 'static)) -> usize {
    let mut n = 1;
    let mut cur = error.source();
    while let Some(link) = cur {
        n += 1;
        cur = link.source();
    }
    n
}

/// One benchmark per library for an operation every library has.
macro_rules! each {
    ($g:expr, $id:literal, [$($lib:ident = $name:literal),+], |$m:ident| $body:expr) => {
        $(
            $g.bench_function(concat!($id, "/", $name), |b| {
                use $lib as $m;
                b.iter(|| black_box($body))
            });
        )+
    };
}

fn size(_: &mut Criterion) {
    // B1, exact: printed once.
    println!(
        "B1 size_of::<Result<(), E>>(): report-err {}, anyhow {}, eyre {}, error-stack {}, Box {}",
        size_of::<report_err::Result<()>>(),
        size_of::<anyhow::Result<()>>(),
        size_of::<eyre::Result<()>>(),
        size_of::<Result<(), stack_lib::E>>(),
        size_of::<Result<(), boxed_lib::E>>(),
    );
}

fn create(c: &mut Criterion) {
    let mut g = c.benchmark_group("create");
    each!(
        g,
        "B3 question mark",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack",
            boxed_lib = "box"
        ],
        |m| m::question_mark()
    );
    each!(
        g,
        "B4 literal",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack",
            boxed_lib = "box"
        ],
        |m| m::literal()
    );
    each!(
        g,
        "B5 formatted",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack",
            boxed_lib = "box"
        ],
        |m| m::formatted(black_box(7))
    );
    g.finish();
}

fn context(c: &mut Criterion) {
    let mut g = c.benchmark_group("context");
    each!(
        g,
        "B6 context on result",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack"
        ],
        |m| m::context_on_result()
    );
    each!(
        g,
        "B7 context after ?",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack"
        ],
        |m| m::context_after_question_mark()
    );
    for n in [3usize, 5, 9] {
        macro_rules! layers {
            ($lib:ident, $name:literal) => {
                g.bench_with_input(
                    BenchmarkId::new(concat!("B8 create + layers/", $name), n),
                    &n,
                    |b, &n| b.iter(|| black_box($lib::layers(black_box(n)))),
                );
            };
        }
        layers!(ours, "report-err");
        layers!(anyhow_lib, "anyhow");
        layers!(eyre_lib, "eyre");
        layers!(stack_lib, "error-stack");
    }
    macro_rules! drops {
        ($lib:ident, $name:literal) => {
            g.bench_function(concat!("B9 drop of 3 layers/", $name), |b| {
                b.iter_batched(|| $lib::layers(3), drop, BatchSize::SmallInput)
            });
        };
    }
    drops!(ours, "report-err");
    drops!(anyhow_lib, "anyhow");
    drops!(eyre_lib, "eyre");
    drops!(stack_lib, "error-stack");
    g.finish();
}

fn inspect(c: &mut Criterion) {
    let mut g = c.benchmark_group("inspect");
    let (r, a, e, s) = (
        ours::layers(3),
        anyhow_lib::layers(3),
        eyre_lib::layers(3),
        stack_lib::layers(3),
    );
    g.bench_function("B10 downcast root after 3/report-err", |b| {
        b.iter(|| ours::downcast(black_box(&r)))
    });
    g.bench_function("B10 downcast root after 3/anyhow", |b| {
        b.iter(|| anyhow_lib::downcast(black_box(&a)))
    });
    g.bench_function("B10 downcast root after 3/eyre", |b| {
        b.iter(|| eyre_lib::downcast(black_box(&e)))
    });
    g.bench_function("B10 downcast root after 3/error-stack", |b| {
        b.iter(|| stack_lib::downcast(black_box(&s)))
    });
    let boxed: boxed_lib::E = Box::new(io_error());
    g.bench_function("B10 downcast root after 3/box", |b| {
        b.iter(|| boxed_lib::downcast(black_box(&boxed)))
    });
    for n in [1usize, 3, 5, 9] {
        let r = ours::layers(n);
        let a = anyhow_lib::layers(n);
        let e = eyre_lib::layers(n);
        g.bench_with_input(BenchmarkId::new("source walk/report-err", n), &n, |b, _| {
            b.iter(|| black_box(walk(black_box(&*r))))
        });
        g.bench_with_input(BenchmarkId::new("source walk/anyhow", n), &n, |b, _| {
            b.iter(|| black_box(walk(black_box(a.as_ref()))))
        });
        g.bench_with_input(BenchmarkId::new("source walk/eyre", n), &n, |b, _| {
            b.iter(|| black_box(walk(black_box(e.as_ref()))))
        });
        g.bench_with_input(BenchmarkId::new("chain/report-err", n), &n, |b, _| {
            b.iter(|| black_box(black_box(&r).chain().count()))
        });
        g.bench_with_input(BenchmarkId::new("chain/anyhow", n), &n, |b, _| {
            b.iter(|| black_box(black_box(&a).chain().count()))
        });
        g.bench_with_input(BenchmarkId::new("chain/eyre", n), &n, |b, _| {
            b.iter(|| black_box(black_box(&e).chain().count()))
        });
    }
    g.finish();
}

fn format(c: &mut Criterion) {
    let mut g = c.benchmark_group("format");
    let (r, a, e, s) = (
        ours::layers(3),
        anyhow_lib::layers(3),
        eyre_lib::layers(3),
        stack_lib::layers(3),
    );
    g.bench_function("B11 display/report-err", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&r)))
    });
    g.bench_function("B11 display/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&a)))
    });
    g.bench_function("B11 display/eyre", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&e)))
    });
    g.bench_function("B11 display/error-stack", |b| {
        b.iter(|| write!(Sink(0), "{}", black_box(&s)))
    });
    g.bench_function("B12 alternate/report-err", |b| {
        b.iter(|| write!(Sink(0), "{:#}", black_box(&r)))
    });
    g.bench_function("B12 alternate/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{:#}", black_box(&a)))
    });
    g.bench_function("B12 alternate/eyre", |b| {
        b.iter(|| write!(Sink(0), "{:#}", black_box(&e)))
    });
    g.bench_function("B13 report/report-err", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&r)))
    });
    g.bench_function("B13 report/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&a)))
    });
    g.bench_function("B13 report/eyre", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&e)))
    });
    g.bench_function("B13 report/error-stack", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&s)))
    });
    // B13 with equal content (D-038): one layer added where the root was
    // made prints no `Trace:` block, so report-err adds only `Location:`.
    let one = report_err::Context::context(Err::<(), _>(io_error()), LAYER).unwrap_err();
    let a1 = anyhow::Context::context(Err::<(), _>(io_error()), LAYER).unwrap_err();
    g.bench_function("B13 report, 1 layer/report-err", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&one)))
    });
    g.bench_function("B13 report, 1 layer/anyhow", |b| {
        b.iter(|| write!(Sink(0), "{:?}", black_box(&a1)))
    });
    g.finish();
}

fn success(c: &mut Criterion) {
    let mut g = c.benchmark_group("success");
    each!(
        g,
        "B2 5-deep ok",
        [
            ours = "report-err",
            anyhow_lib = "anyhow",
            eyre_lib = "eyre",
            stack_lib = "error-stack",
            boxed_lib = "box"
        ],
        |m| m::ok_5(black_box(1))
    );
    g.finish();
}

criterion_group!(benches, size, create, context, inspect, format, success);
criterion_main!(benches);

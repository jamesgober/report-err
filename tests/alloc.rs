//! Allocation counts are part of the contract (PLAN 7.5, 12.5). Every count
//! here is exact; the allocator counts per thread, so tests run in parallel.
//!
//! Not run under Miri: a global allocator that forwards to `System` makes
//! Miri interpret the platform allocator itself, which is outside this crate
//! and outside what Miri checks for it.
#![cfg(not(miri))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::error::Error as StdError;
use std::fmt::{self, Write};
use std::io;

use report_err::{bail, err, Context, Error, Result};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static FREES: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call to the system allocator unchanged; the counters
// are thread-local cells with constant initialisers, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = FREES.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        let _ = FREES.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `f` and returns its value with the allocations and frees it made.
///
/// The first backtrace check in a process reads the environment once; it is
/// done here, outside the counted region. Backtraces must be off: with them
/// on, every new error also captures one, which is a different contract.
fn counted<R>(f: impl FnOnce() -> R) -> (R, usize, usize) {
    let backtrace = std::backtrace::Backtrace::capture();
    assert_ne!(
        backtrace.status(),
        std::backtrace::BacktraceStatus::Captured,
        "unset RUST_BACKTRACE (or set RUST_LIB_BACKTRACE=0) to run the allocation tests"
    );
    drop(backtrace);
    let a = ALLOCS.with(Cell::get);
    let d = FREES.with(Cell::get);
    let r = f();
    (r, ALLOCS.with(Cell::get) - a, FREES.with(Cell::get) - d)
}

fn allocs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let (r, a, _) = counted(f);
    (r, a)
}

/// An error that allocates nothing itself.
fn io_error() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

/// A writer that keeps nothing, so formatting allocates nothing of its own.
struct Sink(usize);
impl Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

#[test]
fn test_alloc_question_mark_on_typed_error_is_one() {
    fn f() -> Result<()> {
        Err(io_error())?;
        Ok(())
    }
    let (error, n) = allocs(f);
    assert_eq!(n, 1);
    drop(error);
}

#[test]
fn test_alloc_literal_macros_are_one() {
    let (error, n) = allocs(|| err!("no input"));
    assert_eq!(n, 1);
    drop(error);
    fn f() -> Result<()> {
        bail!("no input");
    }
    let (error, n) = allocs(f);
    assert_eq!(n, 1);
    drop(error);
}

#[test]
fn test_alloc_formatted_message_is_two() {
    let x = 7;
    let (error, n) = allocs(|| err!("value {x}"));
    assert_eq!(n, 2);
    assert_eq!(error.to_string(), "value 7");
}

#[test]
fn test_alloc_context_on_typed_result_is_one() {
    let (error, n) = allocs(|| Err::<(), _>(io_error()).context("reading").unwrap_err());
    assert_eq!(n, 1);
    drop(error);
}

#[test]
fn test_alloc_context_after_question_mark_is_free() {
    let (error, n) = allocs(|| Error::new(io_error()).context("reading"));
    assert_eq!(n, 1);
    drop(error);
}

#[test]
fn test_alloc_contexts_two_to_five_share_one_block() {
    let error = Error::new(io_error()).context("one");
    let (error, n) = allocs(|| error.context("two"));
    assert_eq!(n, 1);
    let (error, n) = allocs(|| error.context("three").context("four").context("five"));
    assert_eq!(n, 0);
    drop(error);
}

#[test]
fn test_alloc_layers_six_to_nine_add_one_vector() {
    let (error, n) = allocs(|| (0..9).fold(Error::new(io_error()), |e, _| e.context("layer")));
    assert_eq!(
        n, 3,
        "the node, the extension block, and the spill vector (capacity 4)"
    );
    let (error, n) = allocs(|| (0..4).fold(error, |e, _| e.context("layer")));
    assert_eq!(n, 1, "the spill vector grows once, to capacity 8");
    drop(error);
}

#[test]
fn test_alloc_string_context_is_moved_not_copied() {
    let error = Error::new(io_error()).context("one").context("two");
    let text = format!("layer {}", 3);
    let (error, n) = allocs(|| error.context(text));
    assert_eq!(n, 0);
    drop(error);
}

#[test]
fn test_alloc_value_context_is_one_box() {
    #[derive(Debug)]
    struct Shard(u32);
    impl fmt::Display for Shard {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "shard {}", self.0)
        }
    }
    let error = Error::new(io_error()).context("one").context("two");
    let (error, n) = allocs(|| error.context(Shard(4)));
    assert_eq!(n, 1);
    drop(error);
}

#[test]
fn test_alloc_ok_paths_allocate_nothing() {
    let (_, n) = allocs(|| {
        Ok::<u8, io::Error>(1)
            .with_context(|| format!("never {}", 1))
            .ok()
    });
    assert_eq!(n, 0);
    let (_, n) = allocs(|| Some(1u8).context("never").ok());
    assert_eq!(n, 0);
}

#[test]
fn test_alloc_question_mark_on_error_is_free() {
    let error = Error::new(io_error());
    fn pass(e: Error) -> Result<()> {
        Err(e)?;
        Ok(())
    }
    let (error, n) = allocs(|| pass(error));
    assert_eq!(n, 0);
    drop(error);
}

#[test]
fn test_alloc_drop_with_three_literal_contexts_is_two_frees() {
    let error = Error::new(io_error())
        .context("a")
        .context("b")
        .context("c");
    let ((), _, frees) = counted(|| drop(error));
    assert_eq!(frees, 2);
}

#[test]
fn test_alloc_reading_allocates_nothing() {
    let error = Error::new(io_error())
        .context("a")
        .context(String::from("b"))
        .context("c");
    let (_, n) = allocs(|| {
        let mut sink = Sink(0);
        write!(sink, "{error}").ok();
        write!(sink, "{error:#}").ok();
        write!(sink, "{error:?}").ok();
        sink.0
    });
    assert_eq!(n, 0);
    let (_, n) =
        allocs(|| error.chain().count() + usize::from(error.downcast_ref::<io::Error>().is_some()));
    assert_eq!(n, 0);
    let (_, n) = allocs(|| {
        let mut links = 0;
        let mut cur: Option<&(dyn StdError + 'static)> = Some(&*error);
        while let Some(link) = cur {
            links += 1;
            cur = link.source();
        }
        links
    });
    assert_eq!(n, 0);
    let (_, n) = allocs(|| error.root_cause().to_string().len());
    assert_eq!(n, 1, "only the String the test itself asked for");
}

#[test]
fn test_alloc_into_std_is_one_whatever_the_depth() {
    let error = Error::new(io_error())
        .context("a")
        .context("b")
        .context("c");
    let (boxed, n) = allocs(|| error.into_std());
    assert_eq!(n, 1);
    let (error, n) = allocs(|| Error::from_boxed(boxed));
    assert_eq!(n, 0, "a box made by into_std gives the error back");
    drop(error);
}

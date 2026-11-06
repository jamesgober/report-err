//! T9: allocation failure while creating an error aborts the process through
//! `handle_alloc_error`, as everywhere in `alloc`: no undefined behaviour, no
//! hang, no partial error (PLAN 10.2).
//!
//! The global allocator here fails every allocation once `FAIL` is set. The
//! check re-runs this binary as a child that sets it and creates an error;
//! the parent expects the child to end abnormally, quickly. `harness = false`
//! in `Cargo.toml`, as for `exit.rs`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct Failing;

static FAIL: AtomicBool = AtomicBool::new(false);

// SAFETY: forwards to `System`, or returns null, which `GlobalAlloc` allows
// as the report of a failed allocation.
unsafe impl GlobalAlloc for Failing {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if FAIL.load(Ordering::SeqCst) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Failing = Failing;

const CHILD: &str = "REPORT_ERR_OOM_CHILD";

fn child() {
    let io = std::io::Error::from(std::io::ErrorKind::NotFound);
    FAIL.store(true, Ordering::SeqCst);
    let error = report_err::Error::new(io);
    // Not reached: the allocation above aborts.
    FAIL.store(false, Ordering::SeqCst);
    println!("created {error}");
}

fn main() {
    if std::env::var_os(CHILD).is_some() {
        child();
        return;
    }
    if cfg!(miri) {
        return;
    }
    let exe = std::env::current_exe().expect("own path");
    let start = Instant::now();
    let output = Command::new(exe)
        .env(CHILD, "1")
        .output()
        .expect("child runs");
    let took = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "the child should abort: {output:?}"
    );
    assert!(!stdout.contains("created"), "{stdout}");
    assert!(
        took < Duration::from_secs(60),
        "the child hung for {took:?}"
    );
    println!("test test_attack_t9_allocation_failure_aborts ... ok");
}

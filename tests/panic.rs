//! The panic boundary (PLAN 7.15, 12.4 E-95, E-106). The exit code of `run`
//! (E-105) is checked in `tests/exit.rs`, and `panic = "abort"` (E-96) by
//! `examples/panic_boundary.rs` in CI.
#![cfg(all(feature = "std", not(miri)))]

use std::panic::AssertUnwindSafe;

use report_err::{boundary, err, Category, Error, Kinds, Result, Severity, PANICS_CAUGHT};

#[test]
fn test_boundary_passes_results_through() {
    assert_eq!(boundary(|| Ok(1)).unwrap(), 1);
    let error = boundary(|| -> Result<()> { Err(err!("plain")) }).unwrap_err();
    assert!(!error.is_panic());
    assert_eq!(PANICS_CAUGHT, cfg!(panic = "unwind"));
}

#[test]
fn test_boundary_static_str_payload() {
    let line = line!() + 1;
    let error = boundary(|| -> Result<()> { panic!("static message") }).unwrap_err();
    assert_eq!(error.to_string(), "static message");
    assert!(error.is_panic());
    assert!(error.kinds().contains(Kinds::PANIC));
    assert_eq!(error.category(), Category::Internal);
    assert_eq!(error.severity(), Severity::FATAL);
    assert_eq!(error.exit_code(), 101);
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().file(), file!());
}

#[test]
fn test_boundary_string_payload_is_moved() {
    // A runtime argument, so the message is formatted into a `String`.
    let n = std::hint::black_box(7);
    let error = boundary(|| -> Result<()> { panic!("formatted {n}") }).unwrap_err();
    assert_eq!(error.to_string(), "formatted 7");
    assert_eq!(
        error.downcast_ref::<String>().map(String::as_str),
        Some("formatted 7")
    );
}

#[test]
fn test_boundary_custom_payload() {
    let error = boundary(|| -> Result<()> { std::panic::panic_any(42u32) }).unwrap_err();
    assert_eq!(error.to_string(), "panicked with a non-string payload");
    assert!(error.is_panic());
}

struct DropPanics;
impl Drop for DropPanics {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            panic!("payload drop panicked");
        }
    }
}

#[test]
fn test_boundary_payload_whose_drop_panics_is_contained() {
    let error = boundary(|| -> Result<()> { std::panic::panic_any(DropPanics) }).unwrap_err();
    assert_eq!(error.to_string(), "panicked with a non-string payload");
}

#[test]
fn test_boundary_nested() {
    let outer = boundary(|| {
        let inner = boundary(|| -> Result<()> { panic!("inner") }).unwrap_err();
        assert_eq!(inner.to_string(), "inner");
        Err::<(), _>(inner.context("outer layer"))
    })
    .unwrap_err();
    assert_eq!(format!("{outer:#}"), "outer layer: inner");
    assert!(outer.is_panic());
}

#[test]
fn test_boundary_inside_drop_during_unwinding() {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let inner = boundary(|| -> Result<()> { Err(err!("from a drop")) });
            assert!(inner.is_err());
        }
    }
    let error = boundary(|| -> Result<()> {
        let _guard = Guard;
        panic!("unwinding")
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "unwinding");
}

#[test]
fn test_boundary_with_assert_unwind_safe_state() {
    let mut count = 0;
    let result = boundary(AssertUnwindSafe(|| {
        count += 1;
        Ok(count)
    }));
    assert_eq!(result.unwrap(), 1);
}

#[test]
fn test_from_panic_on_a_real_join_error() {
    // E-106
    let handle = std::thread::spawn(|| -> Result<u8> { panic!("worker {} died", 3) });
    let error = handle.join().map_err(Error::from_panic).unwrap_err();
    assert_eq!(error.to_string(), "worker 3 died");
    assert!(error.is_panic());
    assert_eq!(error.exit_code(), 101);
    let fine = std::thread::spawn(|| -> Result<u8> { Ok(5) });
    assert_eq!(fine.join().map_err(Error::from_panic).unwrap().unwrap(), 5);
}

#[test]
fn test_panic_with_explicit_exit_code_and_code() {
    let error = boundary(|| -> Result<()> { panic!("x") })
        .unwrap_err()
        .with_exit_code(3)
        .coded(define_err::code!("APP", 1));
    assert_eq!(error.exit_code(), 3);
    assert_eq!(error.code(), Some(define_err::code!("APP", 1)));
    assert!(error.is_panic());
}

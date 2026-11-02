//! What a process sees: exit status and standard error (PLAN 12.4 E-69,
//! E-70, E-93, E-94). Runs `examples/exit_cases`, which `cargo test` builds,
//! and this test binary as a child for the backtrace variables, which std
//! reads once per process.
#![cfg(all(feature = "std", not(miri)))]

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn example() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(|deps| deps.parent()).unwrap();
    let path = dir
        .join("examples")
        .join(format!("exit_cases{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "build the examples first: {}",
        path.display()
    );
    path
}

fn run(case: &str) -> Output {
    Command::new(example())
        .arg(case)
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .output()
        .unwrap()
}

fn status(case: &str) -> i32 {
    run(case).status.code().unwrap()
}

#[test]
fn test_exit_status_for_each_case() {
    assert_eq!(status("ok"), 0);
    assert_eq!(status("plain"), 1);
    assert_eq!(
        status("io"),
        report_err::Category::NotFound.exit_code().into()
    );
    assert_eq!(
        status("coded"),
        report_err::Category::Unavailable.exit_code().into()
    );
    assert_eq!(status("zero"), 1);
    assert_eq!(status("max"), 255);
}

#[test]
fn test_ok_prints_nothing() {
    let output = run("ok");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn test_report_goes_to_standard_error_only() {
    let output = run("coded");
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.starts_with("Error: query timed out\n"), "{text}");
    assert!(text.contains("Code: DB-0001 (unavailable, "), "{text}");
}

#[test]
fn test_controls_and_bidi_are_escaped_and_blocks_cannot_be_forged() {
    let text = String::from_utf8(run("escape").stderr).unwrap();
    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(!text.contains('\u{202e}'), "{text:?}");
    assert!(text.contains("\\u{1b}[31m"), "{text:?}");
    assert!(text.contains("\\u{202e}"), "{text:?}");
    assert!(!text.contains("\nLocation: forged"), "{text:?}");
}

#[test]
fn test_converted_error_keeps_the_location_of_the_into() {
    let text = String::from_utf8(run("converted").stderr).unwrap();
    assert!(text.contains("exit_cases.rs:"), "{text}");
}

#[test]
fn test_huge_report_is_cut_with_a_marker() {
    let output = run("huge");
    assert_eq!(output.status.code(), Some(9));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.len() <= 1024 * 1024 + 32, "{}", text.len());
    assert!(text.ends_with("(report truncated)\n"));
}

#[test]
fn test_broken_pipe_still_exits_with_the_code() {
    // E-94: larger than any pipe buffer, so the child is writing when the
    // reader goes away.
    let mut child = Command::new(example())
        .arg("huge")
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut first = [0u8; 16];
    stderr.read_exact(&mut first).unwrap();
    drop(stderr);
    assert_eq!(child.wait().unwrap().code(), Some(9));
}

#[test]
fn test_closed_standard_error_still_exits_with_the_code() {
    // E-94: no reader at all from the start.
    let output = Command::new(example())
        .arg("coded")
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(report_err::Category::Unavailable.exit_code().into())
    );
}

// ---------------------------------------------------------------- backtraces

const CHILD: &str = "REPORT_ERR_BACKTRACE_CHILD";

/// Prints whether a new error has a backtrace, when run as the child below.
#[test]
fn child_backtrace_probe() {
    if std::env::var_os(CHILD).is_some() {
        let captured = report_err::err!("x").backtrace().is_some();
        println!("captured={captured}");
    }
}

fn child(vars: &[(&str, &str)]) -> bool {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child_backtrace_probe", "--nocapture"])
        .env(CHILD, "1")
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE");
    for (name, value) in vars {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}");
    text.contains("captured=true")
}

#[test]
fn test_backtrace_follows_the_environment() {
    // E-69
    assert!(!child(&[]));
    // E-70
    assert!(!child(&[
        ("RUST_BACKTRACE", "1"),
        ("RUST_LIB_BACKTRACE", "0")
    ]));
    assert!(child(&[("RUST_BACKTRACE", "1")]));
    assert!(child(&[("RUST_LIB_BACKTRACE", "1")]));
}

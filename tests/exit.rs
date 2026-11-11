//! What a process sees: exit status and standard error (PLAN 12.4 E-69,
//! E-70, E-93, E-94).
//!
//! This binary has its own `main` (`harness = false` in `Cargo.toml`) because
//! `ExitReport` only does its work when `main` returns it. Each check re-runs
//! this binary as a child with one case in the environment, so it needs
//! nothing else built and it runs however the tests are invoked. The backtrace
//! variables are tested the same way, since std reads them once per process.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};

use report_err::{err, Category, Error, ExitReport};

define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.")]
        #[category(Unavailable)]
        Timeout => "query timed out",
    }
}

const CASE: &str = "REPORT_ERR_EXIT_CASE";

// ---------------------------------------------------------------- the child

fn case(name: &str) -> report_err::Result<()> {
    match name {
        "ok" => Ok(()),
        "plain" => Err(err!("something failed")),
        "io" => Err(Error::new(io::Error::from(io::ErrorKind::NotFound))),
        "coded" => Err(Error::new(DbError::Timeout)),
        "zero" => Err(err!("x").with_exit_code(0)),
        "max" => Err(err!("x").with_exit_code(255)),
        "escape" => Err(err!(
            "a\u{1b}[31mb\u{202e}c\n\nLocation: forged\u{1b}]0;title\u{7}\rd\u{9b}2J"
        )
        .context("e\nError: fake")),
        "huge" => Err(err!("{}", "x".repeat(2 * 1024 * 1024)).with_exit_code(9)),
        other => Err(err!("unknown case {other}")),
    }
}

fn child(name: &str) -> ExitReport {
    match name {
        // The error is made by `into`, here, so this line is its location.
        "converted" => std::fs::read_to_string("no-such-file").map(drop).into(),
        "backtrace" => {
            println!("captured={}", err!("x").backtrace().is_some());
            Ok::<(), Error>(()).into()
        }
        "run_ok" => report_err::run(|| Ok(())),
        "run_err" => report_err::run(|| Err(err!("failed in app"))),
        "run_panic" => report_err::run(|| panic!("app panicked")),
        "report_unhandled" => {
            let delivered =
                report_err::report_unhandled(err!("lost\u{1b}[2J in drop").context("flushing"));
            println!("delivered={delivered}");
            Ok::<(), Error>(()).into()
        }
        other => case(other).into(),
    }
}

// ---------------------------------------------------------------- the parent

fn command(name: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .env(CASE, name)
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE");
    command
}

fn run(name: &str) -> Output {
    command(name).output().unwrap()
}

fn status(name: &str) -> i32 {
    run(name).status.code().unwrap()
}

fn exit_status_for_each_case() {
    assert_eq!(status("ok"), 0);
    assert_eq!(status("plain"), 1);
    assert_eq!(status("io"), Category::NotFound.exit_code().into());
    assert_eq!(status("coded"), Category::Unavailable.exit_code().into());
    assert_eq!(status("zero"), 1);
    assert_eq!(status("max"), 255);
}

fn ok_prints_nothing() {
    let output = run("ok");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

fn report_goes_to_standard_error_only() {
    let output = run("coded");
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.starts_with("Error: query timed out\n"), "{text}");
    assert!(text.contains("Code: DB-0001 (unavailable, "), "{text}");
}

fn controls_and_bidi_are_escaped_and_blocks_cannot_be_forged() {
    let text = String::from_utf8(run("escape").stderr).unwrap();
    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(!text.contains('\u{202e}'), "{text:?}");
    assert!(text.contains("\\u{1b}[31m"), "{text:?}");
    assert!(text.contains("\\u{202e}"), "{text:?}");
    assert!(!text.contains("\nLocation: forged"), "{text:?}");
    // T6: an OSC title sequence, a carriage return, a C1 control, and a
    // forged first line.
    for raw in ['\u{7}', '\r', '\u{9b}'] {
        assert!(!text.contains(raw), "{raw:?} in {text:?}");
    }
    assert!(
        text.contains("\\u{1b}]0;title\\u{7}\\u{d}d\\u{9b}2J"),
        "{text:?}"
    );
    assert!(!text.contains("\nError: fake"), "{text:?}");
    assert!(text.starts_with("Error: e\n    Error: fake\n"), "{text:?}");
}

fn converted_error_keeps_the_location_of_the_into() {
    let text = String::from_utf8(run("converted").stderr).unwrap();
    assert!(text.contains("exit.rs:"), "{text}");
}

fn huge_report_is_cut_with_a_marker() {
    let output = run("huge");
    assert_eq!(output.status.code(), Some(9));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.len() <= 1024 * 1024 + 32, "{}", text.len());
    assert!(text.ends_with("(report truncated)\n"));
}

fn broken_pipe_still_exits_with_the_code() {
    // E-94: larger than any pipe buffer, so the child is writing when the
    // reader goes away.
    let mut child = command("huge").stderr(Stdio::piped()).spawn().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut first = [0u8; 16];
    stderr.read_exact(&mut first).unwrap();
    drop(stderr);
    assert_eq!(child.wait().unwrap().code(), Some(9));
}

fn closed_standard_error_still_exits_with_the_code() {
    // E-94: no reader at all from the start.
    let output = command("coded").stderr(Stdio::null()).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(Category::Unavailable.exit_code().into())
    );
}

fn captured(vars: &[(&str, &str)]) -> bool {
    let mut command = command("backtrace");
    for (name, value) in vars {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}");
    text.contains("captured=true")
}

fn backtrace_follows_the_environment() {
    // E-69
    assert!(!captured(&[]));
    // E-70
    assert!(!captured(&[
        ("RUST_BACKTRACE", "1"),
        ("RUST_LIB_BACKTRACE", "0")
    ]));
    assert!(captured(&[("RUST_BACKTRACE", "1")]));
    assert!(captured(&[("RUST_LIB_BACKTRACE", "1")]));
}

fn run_exits_as_its_app_does() {
    // E-105
    assert_eq!(status("run_ok"), 0);
    assert_eq!(status("run_err"), 1);
    let panicked = run("run_panic");
    assert_eq!(panicked.status.code(), Some(101));
    let text = String::from_utf8(panicked.stderr).unwrap();
    assert!(text.contains("Error: app panicked\n"), "{text}");
}

fn report_unhandled_without_a_reporter_writes_one_line() {
    // E-92
    let output = run("report_unhandled");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "delivered=false\n"
    );
    let text = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        text,
        "Error (unhandled): flushing: lost\\u{1b}[2J in drop\n"
    );
    // Standard error closed: no panic.
    let closed = command("report_unhandled")
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(closed.status.code(), Some(0));
}

fn main() -> ExitReport {
    if let Ok(name) = std::env::var(CASE) {
        return child(&name);
    }
    // Miri does not run other processes. `--list` is a runner asking what is
    // here; nothing it can run on its own.
    if !cfg!(miri) && !std::env::args().any(|a| a == "--list") {
        let checks: [(&str, fn()); 11] = [
            ("run_exits_as_its_app_does", run_exits_as_its_app_does),
            (
                "report_unhandled_without_a_reporter_writes_one_line",
                report_unhandled_without_a_reporter_writes_one_line,
            ),
            ("exit_status_for_each_case", exit_status_for_each_case),
            ("ok_prints_nothing", ok_prints_nothing),
            (
                "report_goes_to_standard_error_only",
                report_goes_to_standard_error_only,
            ),
            (
                "controls_and_bidi_are_escaped_and_blocks_cannot_be_forged",
                controls_and_bidi_are_escaped_and_blocks_cannot_be_forged,
            ),
            (
                "converted_error_keeps_the_location_of_the_into",
                converted_error_keeps_the_location_of_the_into,
            ),
            (
                "huge_report_is_cut_with_a_marker",
                huge_report_is_cut_with_a_marker,
            ),
            (
                "broken_pipe_still_exits_with_the_code",
                broken_pipe_still_exits_with_the_code,
            ),
            (
                "closed_standard_error_still_exits_with_the_code",
                closed_standard_error_still_exits_with_the_code,
            ),
            (
                "backtrace_follows_the_environment",
                backtrace_follows_the_environment,
            ),
        ];
        for (name, check) in checks {
            print!("test {name} ... ");
            check();
            println!("ok");
        }
    }
    Ok::<(), Error>(()).into()
}

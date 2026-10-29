//! The 0.4 surface that needs no define-err types: sections, attachments,
//! the trail, exit codes, `Ok`, and the report blocks (PLAN 12.4 E-66 to
//! E-80).

use std::error::Error as StdError;
use std::fmt;

use report_err::{err, Context, Error, Result, SectionKind, StepKind};

#[derive(Debug)]
struct Leaf;
impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("leaf")
    }
}
impl StdError for Leaf {}

fn kinds(error: &Error) -> Vec<StepKind> {
    error.trail().map(|s| s.kind).collect()
}

// ---------------------------------------------------------------- trail

#[test]
fn test_trail_merges_contexts_and_points_in_order() {
    let error = err!("x")
        .trace()
        .context("a")
        .trace()
        .trace()
        .context("b")
        .trace();
    use StepKind::*;
    assert_eq!(
        kinds(&error),
        [Origin, Trace, Context, Trace, Trace, Context, Trace]
    );
    let contexts: Vec<String> = error
        .trail()
        .filter_map(|s| s.context.map(|c| c.to_string()))
        .collect();
    assert_eq!(contexts, ["a", "b"]);
}

#[test]
fn test_trail_point_counts_first_four_latest_eleven() {
    for (points, kept, omitted) in [
        (0u32, 0usize, 0u32),
        (4, 4, 0),
        (5, 5, 0),
        (15, 15, 0),
        (16, 15, 1),
        (1000, 15, 985),
    ] {
        let error = (0..points).fold(err!("x"), |e, _| e.trace());
        let trail = error.trail();
        assert_eq!(trail.omitted(), omitted, "points = {points}");
        assert_eq!(
            error.trail().filter(|s| s.kind == StepKind::Trace).count(),
            kept,
            "points = {points}"
        );
    }
}

#[test]
fn test_trail_order_survives_the_frame_budget() {
    // 70 layers, a trace point after layer 3 and after layer 69.
    let mut error = err!("x");
    for i in 0..70 {
        error = error.context(i);
        if i == 3 || i == 69 {
            error = error.trace();
        }
    }
    let steps: Vec<(StepKind, Option<String>)> = error
        .trail()
        .map(|s| (s.kind, s.context.map(|c| c.to_string())))
        .collect();
    assert_eq!(steps.len(), 1 + 64 + 2);
    let first_trace = steps
        .iter()
        .position(|(k, _)| *k == StepKind::Trace)
        .unwrap();
    assert_eq!(steps[first_trace - 1].1.as_deref(), Some("3"));
    assert_eq!(steps.last().map(|s| s.0), Some(StepKind::Trace));
    assert_eq!(steps[steps.len() - 2].1.as_deref(), Some("69"));
}

#[test]
fn test_trail_through_question_mark_on_error() {
    fn inner() -> Result<()> {
        Err(err!("deep"))
    }
    fn middle() -> Result<()> {
        inner().trace()?;
        Ok(())
    }
    let error = middle().unwrap_err();
    let trace = error.trail().find(|s| s.kind == StepKind::Trace).unwrap();
    assert_eq!(trace.location.file(), file!());
}

// ---------------------------------------------------------------- sections and attachments

#[test]
fn test_sections_print_in_order_under_the_causes() {
    let error = Error::new(Leaf)
        .context("loading")
        .note("first")
        .warning("second")
        .suggestion(format!("third {}", 3));
    let report = format!("{error:?}");
    let at = report.find("\n\nNote: first\nWarning: second\nSuggestion: third 3\n\nLocation: ");
    assert!(at.is_some(), "{report}");
    let sections: Vec<(SectionKind, &str)> = error.sections().map(|s| (s.kind, s.text)).collect();
    assert_eq!(
        sections,
        [
            (SectionKind::Note, "first"),
            (SectionKind::Warning, "second"),
            (SectionKind::Suggestion, "third 3")
        ]
    );
}

#[test]
fn test_sections_on_result_and_option() {
    let r: std::result::Result<(), Leaf> = Err(Leaf);
    let error = r
        .note("n")
        .with_warning(|| "w")
        .suggestion("s")
        .unwrap_err();
    assert_eq!(error.sections().count(), 3);
    assert!(error.is::<Leaf>());
    let error = None::<u8>.with_suggestion(|| "set it").unwrap_err();
    assert_eq!(error.to_string(), "no value");
    let error = Some(1u8).note("never").map(|v| v + 1);
    assert_eq!(error.ok(), Some(2));
}

#[test]
fn test_with_note_on_ok_never_runs() {
    let calls = std::cell::Cell::new(0);
    let r: std::result::Result<u8, Leaf> = Ok(1);
    let _ = r.with_note(|| {
        calls.set(1);
        "x"
    });
    assert_eq!(calls.get(), 0);
}

#[test]
fn test_attach_display_prints_and_opaque_does_not() {
    let error = err!("x")
        .attach_display("tenant 42")
        .attach("secret-token")
        .attach_display(7u8);
    let report = format!("{error:?}");
    assert!(
        report.contains("\nAttachments:\n    tenant 42\n    7"),
        "{report}"
    );
    assert!(!report.contains("secret-token"));
    assert_eq!(error.attachments::<&str>().count(), 2);
}

#[test]
fn test_attach_on_result() {
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct RequestId(u64);
    let r: std::result::Result<(), Leaf> = Err(Leaf);
    let error = r.attach(RequestId(9)).context("handling").unwrap_err();
    assert_eq!(error.attachment::<RequestId>().copied(), Some(RequestId(9)));
}

#[test]
fn test_omitted_line_counts_every_budget() {
    let mut error = err!("x");
    for i in 0..66 {
        error = error.context(i);
    }
    for i in 0..34u32 {
        error = error.attach(i);
    }
    for _ in 0..33 {
        error = error.note("n");
    }
    for _ in 0..17 {
        error = error.trace();
    }
    let report = format!("{error:?}");
    assert!(
        report.contains(
            "\nOmitted: 2 context layers, 2 attachments, 1 section, 2 trace points (budget)"
        ),
        "{report}"
    );
}

#[test]
fn test_section_text_over_the_byte_budget_is_cut() {
    let big = "é".repeat(40_000); // 80,000 bytes, more than 64 KiB
    let error = err!("x").note(big);
    let text = error.sections().next().unwrap().text;
    assert!(text.len() <= 64 * 1024);
    assert!(text.ends_with("...(truncated)"));
    assert!(format!("{error:?}").contains("1 text cut"));
}

// ---------------------------------------------------------------- exit codes and Ok

#[test]
fn test_exit_code_default_explicit_and_zero() {
    assert_eq!(err!("x").exit_code(), 1);
    assert_eq!(err!("x").with_exit_code(64).exit_code(), 64);
    assert_eq!(err!("x").with_exit_code(0).exit_code(), 1);
    assert_eq!(err!("x").with_exit_code(255).exit_code(), 255);
}

#[test]
fn test_ok_helper_infers_the_error_type() {
    let value = (|| {
        let n: u32 = "21".parse()?;
        report_err::Ok(n * 2)
    })();
    assert_eq!(value.ok(), Some(42));
}

// ---------------------------------------------------------------- backtraces

#[cfg(feature = "std")]
#[test]
#[cfg_attr(miri, ignore = "printing a backtrace reads the current directory, which Miri isolates")]
fn test_with_backtrace_forces_one_and_keeps_the_first() {
    let error = err!("x").with_backtrace();
    let first = error.backtrace().map(|b| b as *const _);
    let error = error.with_backtrace();
    assert_eq!(error.backtrace().map(|b| b as *const _), first);
    assert!(format!("{error:?}").contains("\n\nBacktrace:\n"));
}

#[cfg(feature = "std")]
#[test]
fn test_context_never_captures_a_backtrace() {
    let error = err!("x");
    let had = error.backtrace().is_some();
    let error = error.context("a").context("b");
    assert_eq!(error.backtrace().is_some(), had);
}

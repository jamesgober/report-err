//! Several causes, collections, suppressed errors, cleanup, searching the
//! tree, guarded rendering, and threads (PLAN 12.4 E-81 to E-91, E-102 to
//! E-104, E-107).

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use report_err::{err, CollectAll, Context, Error, Errors, RenderStyle, Result};

#[derive(Debug)]
struct Leaf(u32);
impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "leaf {}", self.0)
    }
}
impl StdError for Leaf {}

fn texts(errors: &[Error]) -> Vec<String> {
    errors.iter().map(ToString::to_string).collect()
}

// ---------------------------------------------------------------- groups

#[test]
fn test_group_with_no_children_and_empty_errors() {
    // E-81
    let error = Error::group("nothing", std::iter::empty());
    assert!(error.children().is_empty());
    assert_eq!(error.to_string(), "nothing");
    assert!(!format!("{error:?}").contains("Causes"));
    assert!(Errors::new().finish("x").is_ok());
    assert!(Errors::new().into_error("x").is_none());
}

#[test]
fn test_group_with_one_child() {
    // E-82
    let error = Error::group("one", [err!("only")]);
    assert_eq!(texts(error.children()), ["only"]);
    let text = format!("{error:?}");
    assert!(
        text.starts_with("one\n\nCauses (1):\n    0: only\n"),
        "{text}"
    );
}

#[test]
fn test_report_prints_the_tree_and_suppressed() {
    let inner = Error::group("shard 2", [err!("disk"), err!("net")]);
    let error = Error::group(
        "2 of 3 shards failed",
        [err!("shard 1").context("in a"), inner],
    )
    .suppress(err!("rollback failed"));
    let text = format!("{error:?}");
    let expected = "2 of 3 shards failed\n\nCauses (2):\n    0: in a: shard 1\n    1: shard 2\n        0: disk\n        1: net\n\nSuppressed:\n    rollback failed\n\nLocation: ";
    assert!(text.starts_with(expected), "{text}");
}

#[test]
fn test_collect_all_empty_ok_and_failures_in_order() {
    // E-83
    let empty: Result<Vec<u8>> = std::iter::empty::<std::result::Result<u8, Leaf>>().collect_all();
    assert!(empty.unwrap().is_empty());
    let mixed = [Ok(1u8), Err(Leaf(2)), Ok(3), Err(Leaf(4))];
    let error = mixed.into_iter().collect_all::<Vec<u8>>().unwrap_err();
    assert_eq!(error.to_string(), "2 of 4 items failed");
    assert_eq!(texts(error.children()), ["leaf 2", "leaf 4"]);
    assert!(error.children()[0].is::<Leaf>());
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.children()[0].location().file(), file!());
}

#[test]
fn test_collect_all_ten_million_failures_is_bounded() {
    // E-84 (T2): every item counted, the tree kept to `max_tree`.
    let count = if cfg!(miri) { 3_000 } else { 10_000_000 };
    let error = (0..count)
        .map(|i| Err::<(), _>(Leaf(i)))
        .collect_all::<Vec<()>>()
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("{count} of {count} items failed")
    );
    assert_eq!(error.children().len(), 1023);
    let text = format!("{error:?}");
    assert!(
        text.contains(&format!("Omitted: {} children (budget)", count - 1023)),
        "{}",
        &text[text.len() - 200..]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_collect_all_iterator_that_panics_propagates() {
    // E-85
    let result = std::panic::catch_unwind(|| {
        (0..10)
            .map(|i| {
                if i == 5 {
                    panic!("halfway")
                } else {
                    Err::<(), _>(Leaf(i))
                }
            })
            .collect_all::<Vec<()>>()
    });
    assert!(result.is_err());
}

#[test]
fn test_errors_len_is_exact_past_the_budget() {
    let mut errors = Errors::new();
    for i in 0..1500 {
        errors.push(Leaf(i));
    }
    assert_eq!(errors.len(), 1500);
    let error = errors.finish("too many").unwrap_err();
    assert_eq!(error.children().len(), 1023);
    assert!(format!("{error:?}").contains("Omitted: 477 children (budget)"));
}

#[test]
fn test_errors_push_result_keeps_values() {
    let mut errors = Errors::new();
    assert_eq!(errors.push_result(Ok::<u8, Leaf>(1)), Some(1));
    assert_eq!(errors.push_result(Err::<u8, _>(Leaf(2))), None);
    assert_eq!(errors.len(), 1);
    assert!(!errors.is_empty());
    assert!(format!("{errors:?}").starts_with("Errors { len: 1"));
}

#[test]
fn test_split_always_true_and_always_false() {
    // E-86
    let make = || Error::group("g", [err!("a"), err!("b")]).coded(define_err::code!("APP", 4));
    let (all, none) = make().split(|_| true);
    assert_eq!(all.unwrap().children().len(), 2);
    assert!(none.is_none());
    let (none, all) = make().split(|_| false);
    assert!(none.is_none());
    let all = all.unwrap();
    assert_eq!(all.children().len(), 2);
    assert_eq!(all.to_string(), "g");
    assert_eq!(all.code(), Some(define_err::code!("APP", 4)));
}

#[test]
fn test_split_keeps_head_on_both_halves() {
    let error = Error::group("g", [err!("a"), err!("b"), err!("c")])
        .note("both halves")
        .with_exit_code(9);
    let (left, right) = error.split(|e| e.to_string() == "b");
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_eq!(texts(left.children()), ["b"]);
    assert_eq!(texts(right.children()), ["a", "c"]);
    for half in [&left, &right] {
        assert_eq!(half.to_string(), "g");
        assert_eq!(half.exit_code(), 9);
        assert_eq!(half.sections().count(), 1);
    }
    assert_eq!(left.location(), right.location());
}

#[test]
fn test_split_of_a_leaf_tests_the_leaf() {
    let (yes, no) = err!("x").split(|_| true);
    assert!(yes.is_some() && no.is_none());
}

#[test]
fn test_flatten_at_max_depth() {
    // E-87
    let mut error = err!("leaf");
    for level in 0..16 {
        error = Error::group(format!("level {level}"), [error, err!("side")]);
    }
    let leaves = error.flatten();
    assert_eq!(leaves.len(), 17);
    assert_eq!(leaves[0].to_string(), "leaf");
    assert!(leaves[1..].iter().all(|e| e.to_string() == "side"));
}

#[test]
fn test_child_past_depth_is_refused_and_counted() {
    let mut error = err!("leaf");
    for _ in 0..16 {
        error = Error::group("level", [error]);
    }
    let mut top = err!("top");
    assert!(!top.push_child(error));
    assert!(top.children().is_empty());
    assert!(format!("{top:?}").contains("Omitted: 1 child (budget)"));
}

#[test]
fn test_child_whose_tree_does_not_fit_is_refused_whole() {
    // E-88
    let big = Error::group("big", (0..600).map(|i| err!("{i}")));
    let mut top = Error::group("top", (0..500).map(|i| err!("{i}")));
    assert!(!top.push_child(big));
    assert_eq!(top.children().len(), 500);
}

#[test]
fn test_suppressed_counts_against_the_tree() {
    // Depth 16: a deeper suppressed error is refused, so dropping one can
    // never recurse past the budget.
    let mut chained = err!("bottom");
    let mut wrapped = 0;
    for _ in 0..40 {
        let next = err!("handler").suppress(chained);
        if next.suppressed().is_empty() {
            break;
        }
        wrapped += 1;
        chained = next;
    }
    assert_eq!(wrapped, 16);
}

#[test]
fn test_suppressed_is_not_a_cause() {
    let error = err!("write failed").suppress(Error::new(Leaf(1)));
    assert!(error.find::<Leaf>().is_none());
    assert!(!error.is::<Leaf>());
    assert_eq!(error.code(), None);
    assert_eq!(
        error
            .suppressed()
            .iter()
            .find_map(|e| e.find::<Leaf>())
            .map(|l| l.0),
        Some(1)
    );
    assert_eq!(error.chain().count(), 1);
}

#[test]
fn test_suppressed_budget() {
    let mut error = err!("x");
    for i in 0..20 {
        error = error.suppress(Leaf(i));
    }
    assert_eq!(error.suppressed().len(), 16);
    assert!(format!("{error:?}").contains("4 suppressed errors"));
}

// ---------------------------------------------------------------- find

#[test]
fn test_find_searches_contexts_root_sources_and_children() {
    #[derive(Debug)]
    struct Outer(Leaf);
    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("outer")
        }
    }
    impl StdError for Outer {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&self.0)
        }
    }
    let error = Error::new(Outer(Leaf(1))).context("a");
    assert_eq!(error.find::<Leaf>().map(|l| l.0), Some(1));
    assert!(error.find::<Outer>().is_some());

    let group = Error::group(
        "g",
        [
            err!("x"),
            Error::group("inner", [Error::new(Leaf(7)), Error::new(Leaf(8))]),
            Error::new(Leaf(9)),
        ],
    );
    let found: Vec<u32> = group.find_all::<Leaf>().map(|l| l.0).collect();
    assert_eq!(found, [7, 8, 9]);
    assert_eq!(group.find::<Leaf>().map(|l| l.0), Some(7));
    assert!(group.find::<std::io::Error>().is_none());
}

#[test]
fn test_find_reaches_a_boxed_root() {
    let boxed: Box<dyn StdError + Send + Sync> = Box::new(Leaf(3));
    let error = Error::from_boxed(boxed).context("loading");
    assert_eq!(error.find::<Leaf>().map(|l| l.0), Some(3));
}

#[test]
fn test_find_in_a_context_value() {
    let error = err!("x").context(Leaf(5));
    assert_eq!(error.find::<Leaf>().map(|l| l.0), Some(5));
}

#[test]
fn test_chain_rev_is_the_chain_backwards() {
    let error = Error::new(Leaf(1)).context("a").context("b");
    let forward: Vec<String> = error.chain().map(|l| l.to_string()).collect();
    let mut backward: Vec<String> = error.chain().rev().map(|l| l.to_string()).collect();
    backward.reverse();
    assert_eq!(forward, backward);
    let mut both = error.chain();
    assert_eq!(both.len(), 3);
    assert_eq!(
        both.next_back().map(|l| l.to_string()).as_deref(),
        Some("leaf 1")
    );
    assert_eq!(both.len(), 2);
    assert_eq!(both.next().map(|l| l.to_string()).as_deref(), Some("b"));
    assert_eq!(both.next().map(|l| l.to_string()).as_deref(), Some("a"));
    assert!(both.next().is_none());
    assert!(both.next_back().is_none());
}

// ---------------------------------------------------------------- cleanup

#[test]
fn test_with_cleanup_four_cases() {
    // E-90
    let ok: std::result::Result<u8, Leaf> = Ok(1);
    let bad: std::result::Result<u8, Leaf> = Err(Leaf(1));
    let fine = || Ok::<(), Leaf>(());
    let fails = || Err::<(), _>(Leaf(2));
    assert_eq!(ok.with_cleanup(fine).unwrap(), 1);
    let main = bad.with_cleanup(fine).unwrap_err();
    assert_eq!(main.to_string(), "leaf 1");
    assert!(main.suppressed().is_empty());
    let ok: std::result::Result<u8, Leaf> = Ok(1);
    assert_eq!(ok.with_cleanup(fails).unwrap_err().to_string(), "leaf 2");
    let bad: std::result::Result<u8, Leaf> = Err(Leaf(1));
    let both = bad.with_cleanup(fails).unwrap_err();
    assert_eq!(both.to_string(), "leaf 1");
    assert_eq!(texts(both.suppressed()), ["leaf 2"]);
    assert_eq!(
        None::<u8>
            .with_cleanup(fails)
            .unwrap_err()
            .suppressed()
            .len(),
        1
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_with_cleanup_that_panics_propagates() {
    // E-91
    let result = std::panic::catch_unwind(|| {
        let bad: std::result::Result<u8, Leaf> = Err(Leaf(1));
        bad.with_cleanup(|| -> std::result::Result<(), Leaf> { panic!("cleanup") })
    });
    assert!(result.is_err());
}

// ---------------------------------------------------------------- render

#[cfg(feature = "std")]
#[derive(Debug)]
struct Panics;
#[cfg(feature = "std")]
impl fmt::Display for Panics {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("display panicked on purpose")
    }
}

#[derive(Debug)]
struct Fails;
impl fmt::Display for Fails {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Err(fmt::Error)
    }
}

fn rendered(error: &Error, style: RenderStyle) -> String {
    let mut text = String::new();
    error.render(&mut text, style).unwrap();
    text
}

#[cfg(feature = "std")]
#[test]
#[cfg_attr(miri, ignore)]
fn test_render_survives_a_display_that_panics() {
    let error = err!("root").context(Panics).context("outer");
    assert_eq!(
        rendered(&error, RenderStyle::Chain),
        "outer: <display panicked>: root"
    );
    let report = rendered(&error, RenderStyle::Report);
    assert!(report.contains("<display panicked>"), "{report}");
    let record = error.record();
    assert_eq!(record.chain[1].message, "<display panicked>");
}

#[test]
fn test_render_survives_a_display_that_fails() {
    let error = err!("root").context(Fails);
    assert_eq!(rendered(&error, RenderStyle::Message), "<display failed>");
    assert!(rendered(&error, RenderStyle::Report)
        .starts_with("<display failed>\n\nCaused by:\n    root"));
}

#[test]
fn test_render_matches_the_formats() {
    let error = Error::new(Leaf(1)).context("a").note("n");
    assert_eq!(rendered(&error, RenderStyle::Message), format!("{error}"));
    assert_eq!(rendered(&error, RenderStyle::Chain), format!("{error:#}"));
    assert_eq!(rendered(&error, RenderStyle::Report), format!("{error:?}"));
}

#[test]
fn test_render_stops_at_max_render() {
    let error = err!("{}", "x".repeat(2 << 20));
    let text = rendered(&error, RenderStyle::Report);
    assert!(text.len() <= (1 << 20) + 32);
    assert!(text.ends_with("(report truncated)"));
}

#[test]
fn test_render_reports_a_failing_writer() {
    struct Closed;
    impl fmt::Write for Closed {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    let error = err!("x").context(Fails);
    assert!(error.render(&mut Closed, RenderStyle::Chain).is_err());
}

// ---------------------------------------------------------------- std, threads

#[test]
fn test_into_box_walks_like_chain() {
    // E-102
    let error = Error::new(Leaf(1)).context("a").context("b");
    let expected: Vec<String> = error.chain().map(|l| l.to_string()).collect();
    let boxed: Box<dyn std::error::Error + Send + Sync> = error.into();
    let mut walked = vec![boxed.to_string()];
    let mut cur = boxed.source();
    while let Some(link) = cur {
        walked.push(link.to_string());
        cur = link.source();
    }
    assert_eq!(walked, expected);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_error_in_an_arc_from_sixteen_threads() {
    // E-103
    let error = Arc::new(Error::group("g", [Error::new(Leaf(1)), err!("b")]).context("outer"));
    let expected = format!("{error:?}");
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let error = Arc::clone(&error);
            std::thread::spawn(move || {
                (
                    format!("{error:?}"),
                    error.find::<Leaf>().map(|l| l.0),
                    error.chain().count(),
                )
            })
        })
        .collect();
    for thread in threads {
        let (text, found, links) = thread.join().unwrap();
        assert_eq!(text, expected);
        assert_eq!(found, Some(1));
        assert_eq!(links, 2);
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_drop_full_tree_at_max_depth_on_a_small_stack() {
    // E-104
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let mut error = Error::new(Leaf(0));
            for level in 0..16 {
                let wide = (0..20).map(|i| err!("{level}.{i}"));
                error = Error::group("level", std::iter::once(error).chain(wide));
            }
            assert!(error.tree_len_for_test() > 300);
            drop(error);
        })
        .unwrap();
    worker.join().unwrap();
}

trait TreeLen {
    fn tree_len_for_test(&self) -> usize;
}

impl TreeLen for Error {
    fn tree_len_for_test(&self) -> usize {
        1 + self
            .children()
            .iter()
            .map(|c| c.tree_len_for_test())
            .sum::<usize>()
    }
}

#[test]
fn test_every_public_call_on_every_constructor() {
    // E-107
    let boxed: Box<dyn StdError + Send + Sync> = Box::new(Leaf(9));
    let payload: Box<dyn std::any::Any + Send> = Box::new("p");
    let makers: Vec<Error> = vec![
        Error::new(Leaf(1)),
        Error::msg("m"),
        err!("lit"),
        err!("fmt {}", 1),
        Error::from_boxed(boxed),
        Error::from_info(Info::One),
        Error::from_panic(payload),
        Error::group("g", [err!("c")]),
        Errors::new()
            .into_error("never")
            .unwrap_or_else(|| err!("empty")),
        None::<u8>.context("none").unwrap_err(),
    ];
    for error in makers {
        let error = error
            .context("ctx")
            .note("n")
            .attach(1u8)
            .trace()
            .suppress(err!("s"))
            .with_kinds(report_err::Kinds::AUDIT);
        let _ = (
            error.to_string(),
            format!("{error:#}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        );
        let _ = (
            error.code(),
            error.severity(),
            error.category(),
            error.kinds(),
            error.public_message(),
        );
        let _ = (
            error.exit_code(),
            error.is_panic(),
            error.location(),
            error.root_cause().to_string(),
        );
        let _ = (
            error.chain().rev().count(),
            error.trail().count(),
            error.children().len(),
        );
        let _ = (
            error.find::<Leaf>(),
            error.suppressed().iter().find_map(|e| e.find::<Leaf>()),
            error.record(),
        );
        let _ = (
            error.attachment::<u8>(),
            error.sections().count(),
            error.info().is_some(),
        );
        let mut text = String::new();
        for style in [
            RenderStyle::Message,
            RenderStyle::Chain,
            RenderStyle::Report,
        ] {
            error.render(&mut text, style).unwrap();
        }
        let (a, b) = error.split(|_| true);
        let _ = a.map(Error::flatten);
        let _ = b.map(<Box<dyn std::error::Error + Send + Sync>>::from);
    }
}

define_err::define_errors! {
    namespace = "T";
    pub enum Info {
        #[code(1, help = "x")]
        One => "info one",
    }
}

//! Edge cases of the 0.3 surface (PLAN 12.4): construction, context, macros,
//! downcast and chain, formatting, and the `Deref` chain.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use report_err::{bail, ensure, err, Context, Error, Result};

fn walk(error: &(dyn StdError + 'static)) -> Vec<String> {
    let mut out = vec![error.to_string()];
    let mut cur = error.source();
    while let Some(link) = cur {
        out.push(link.to_string());
        cur = link.source();
    }
    out
}

fn chain(error: &Error) -> Vec<String> {
    error.chain().map(|l| l.to_string()).collect()
}

#[derive(Debug)]
struct Leaf;
impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("leaf")
    }
}
impl StdError for Leaf {}

// ---------------------------------------------------------------- traits

#[test]
fn test_traits_error_is_send_sync_unwind_safe_and_one_word() {
    fn assert_traits<
        T: Send + Sync + Unpin + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static,
    >() {
    }
    assert_traits::<Error>();
    assert_eq!(std::mem::size_of::<Error>(), std::mem::size_of::<usize>());
    assert_eq!(
        std::mem::size_of::<Result<()>>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<Option<Error>>(),
        std::mem::size_of::<usize>()
    );
}

// ---------------------------------------------------------------- construction

static DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct Counted;
impl fmt::Display for Counted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("counted")
    }
}
impl StdError for Counted {}
impl Drop for Counted {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_construction_counting_drop_runs_once_and_not_on_downcast() {
    let before = DROPS.load(Ordering::SeqCst);
    drop(Error::new(Counted).context("a").context("b"));
    assert_eq!(DROPS.load(Ordering::SeqCst) - before, 1);
    let before = DROPS.load(Ordering::SeqCst);
    let taken: Counted = Error::new(Counted).context("x").downcast().ok().unwrap();
    assert_eq!(
        DROPS.load(Ordering::SeqCst) - before,
        0,
        "moved out, not dropped"
    );
    drop(taken);
}

#[test]
fn test_construction_large_payload() {
    #[derive(Debug)]
    struct Big(Box<[u8; 1 << 20]>, [u8; 4096]);
    impl fmt::Display for Big {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "big {}", self.0.len() + self.1.len())
        }
    }
    impl StdError for Big {}
    let error = Error::new(Big(Box::new([1; 1 << 20]), [2; 4096])).context("ctx");
    assert_eq!(chain(&error), ["ctx", "big 1052672"]);
}

#[test]
fn test_construction_empty_display_and_control_characters() {
    let error = Error::msg("").context("");
    assert_eq!(error.to_string(), "");
    let text = "nul \0 esc \u{1b}[2J cr \r tab \t 😀 𝄞";
    let error = Error::msg(text);
    assert_eq!(error.to_string(), text, "Display stays raw for programs");
}

#[test]
fn test_construction_escaped_braces_literal() {
    assert_eq!(err!("{{}}").to_string(), "{}");
    assert!(
        err!("{{}}").is::<&str>(),
        "an escaped literal is still a literal"
    );
}

#[test]
fn test_construction_macro_picks_error_or_message() {
    let io = err!(io::Error::from(io::ErrorKind::NotFound));
    assert!(io.is::<io::Error>(), "an error value stays an error");
    let text = err!(String::from("owned"));
    assert!(text.is::<String>(), "any other value becomes a message");
    let existing = err!(err!("inner").context("ctx"));
    assert_eq!(
        chain(&existing),
        ["ctx", "inner"],
        "an Error passes through"
    );
    let boxed: Box<dyn StdError + Send + Sync> = Box::new(Leaf);
    let boxed = err!(boxed);
    assert!(
        boxed.root_cause().is::<Leaf>(),
        "a boxed error keeps its concrete root"
    );
}

#[test]
fn test_construction_from_many_threads() {
    let handles: Vec<_> = (0..64)
        .map(|i| std::thread::spawn(move || Error::new(Leaf).context(format!("t{i}")).to_string()))
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        assert_eq!(h.join().unwrap(), format!("t{i}"));
    }
}

// ---------------------------------------------------------------- context

#[test]
fn test_context_layer_counts_inline_spill_and_budget() {
    for layers in [1usize, 2, 5, 6, 63, 64] {
        let mut error = Error::new(Leaf);
        for i in 0..layers {
            error = error.context(format!("l{i}"));
        }
        assert_eq!(error.chain().len(), layers + 1, "layers = {layers}");
        assert_eq!(walk(&*error), chain(&error));
    }
    let mut error = Error::new(Leaf);
    for i in 0..65 {
        error = error.context(format!("l{i}"));
    }
    assert_eq!(error.chain().len(), 65, "64 layers kept and the root");
    assert!(format!("{error:?}").contains("Omitted: 1 context layer (frame budget)"));
}

#[test]
fn test_context_downcast_after_more_layers() {
    #[derive(Debug, PartialEq)]
    struct Request(u64);
    impl fmt::Display for Request {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "request {}", self.0)
        }
    }
    let error = Error::new(Leaf).context(Request(5));
    let error = (0..10).fold(error, |e, i| e.context(format!("outer {i}")));
    assert_eq!(error.downcast_ref::<Request>(), Some(&Request(5)));
    assert!(error.is::<Leaf>());
    assert_eq!(error.downcast::<Request>().ok(), Some(Request(5)));
}

#[test]
fn test_context_value_types_downcast() {
    let error = Error::new(Leaf)
        .context("lit")
        .context(String::from("owned"))
        .context(9i64);
    assert_eq!(error.downcast_ref::<&str>(), Some(&"lit"));
    assert_eq!(
        error.downcast_ref::<String>().map(String::as_str),
        Some("owned")
    );
    assert_eq!(error.downcast_ref::<i64>(), Some(&9));
}

#[test]
fn test_context_closure_that_panics_does_not_leak() {
    let result = std::panic::catch_unwind(|| {
        let r: std::result::Result<(), io::Error> = Err(io::Error::from(io::ErrorKind::Other));
        r.with_context(|| -> &'static str { panic!("in the closure") })
    });
    assert!(result.is_err());
}

// ---------------------------------------------------------------- macros

#[test]
fn test_macros_inline_captures_positional_named_width() {
    let name = "db";
    let n = 3;
    assert_eq!(err!("{name} x{n}").to_string(), "db x3");
    assert_eq!(err!("{} x{}", name, n).to_string(), "db x3");
    assert_eq!(err!("{a} x{b}", a = name, b = n).to_string(), "db x3");
    assert_eq!(err!("[{n:>3}]").to_string(), "[  3]");
    assert_eq!(report_err::format_err!("{name}").to_string(), "db");
}

#[test]
fn test_macros_ensure_condition_with_braces_is_literal() {
    fn f(s: &str) -> Result<()> {
        ensure!(s != "{}");
        Ok(())
    }
    assert_eq!(
        f("{}").unwrap_err().to_string(),
        "condition failed: s != \"{}\""
    );
    fn g(v: &[u8]) -> Result<()> {
        ensure!(v.len() > 2);
        Ok(())
    }
    assert_eq!(
        g(&[]).unwrap_err().to_string(),
        "condition failed: v.len() > 2"
    );
}

#[test]
fn test_macros_ensure_and_bail_with_error_value() {
    fn f() -> Result<()> {
        ensure!(false, io::Error::from(io::ErrorKind::TimedOut));
        Ok(())
    }
    assert!(f().unwrap_err().is::<io::Error>());
    fn g() -> Result<()> {
        bail!(Leaf);
    }
    assert!(g().unwrap_err().is::<Leaf>());
}

#[test]
fn test_macros_bail_in_closure_infers_result() {
    let check = |n: u8| -> Result<u8> {
        if n == 0 {
            bail!("zero");
        }
        Ok(n)
    };
    assert_eq!(check(0).unwrap_err().to_string(), "zero");
    assert_eq!(check(1).ok(), Some(1));
}

#[test]
fn test_macros_record_the_call_site() {
    let line = line!() + 1;
    let error = err!("here");
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().file(), file!());
    fn via_question_mark() -> Result<()> {
        let _: u8 = "x".parse()?;
        Ok(())
    }
    assert_eq!(
        via_question_mark().unwrap_err().location().file(),
        file!(),
        "the `?` is the origin"
    );
}

// ---------------------------------------------------------------- downcast and chain

#[test]
fn test_downcast_wrong_type_and_mut_root() {
    let mut error = Error::msg(String::from("a")).context("ctx");
    assert!(error.downcast_ref::<io::Error>().is_none());
    error.downcast_mut::<String>().unwrap().push('!');
    assert_eq!(format!("{error:#}"), "ctx: a!");
}

#[test]
fn test_downcast_literal_and_formatted_messages() {
    let x = 1;
    assert!(err!("literal").is::<&str>());
    assert!(err!("value {x}").is::<String>());
    assert!(!err!("value {x}").is::<&str>());
}

#[test]
fn test_chain_and_source_walk_agree_everywhere() {
    #[derive(Debug)]
    struct Wrap(Leaf);
    impl fmt::Display for Wrap {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("wrap")
        }
    }
    impl StdError for Wrap {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&self.0)
        }
    }
    for layers in [0, 1, 2, 5, 6, 12] {
        let error = (0..layers).fold(Error::new(Wrap(Leaf)), |e, i| e.context(format!("c{i}")));
        assert_eq!(walk(&*error), chain(&error));
        let as_ref: &(dyn StdError + Send + Sync + 'static) = error.as_ref();
        assert_eq!(walk(as_ref), chain(&error));
        let boxed = error.into_std();
        let from_box = walk(&*boxed);
        assert_eq!(from_box.len(), layers + 2);
    }
}

#[test]
fn test_deref_mut_then_read_again() {
    let mut error = Error::new(Leaf).context("a").context("b");
    {
        let top: &mut (dyn StdError + Send + Sync + 'static) = &mut *error;
        assert_eq!(top.source().map(|s| s.to_string()).as_deref(), Some("a"));
        assert!(
            top.downcast_mut::<Leaf>().is_none(),
            "the outermost layer is a context"
        );
    }
    assert_eq!(chain(&error), ["b", "a", "leaf"]);
}

#[test]
fn test_shared_error_read_from_many_threads() {
    let error = Arc::new(
        Error::new(Leaf)
            .context("a")
            .context(String::from("b"))
            .context("c"),
    );
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let error = Arc::clone(&error);
            std::thread::spawn(move || (format!("{error:#}"), walk(&**error)))
        })
        .collect();
    for h in handles {
        let (alt, links) = h.join().unwrap();
        assert_eq!(alt, "c: b: a: leaf");
        assert_eq!(links, ["c", "b", "a", "leaf"]);
    }
}

// ---------------------------------------------------------------- formatting

#[test]
#[cfg_attr(miri, ignore = "reads the file system, which Miri isolates")]
fn test_format_report_golden() {
    fn read_config() -> Result<String> {
        std::fs::read_to_string("no/such/app.conf").context("reading config")
    }
    fn start() -> Result<()> {
        let _text = read_config().context("starting server")?;
        Ok(())
    }
    let error = start().unwrap_err();
    let io_text = io::Error::from(io::ErrorKind::NotFound).to_string();
    let os_text = error.root_cause().to_string();
    assert!(!io_text.is_empty());
    assert_eq!(
        format!("{error:?}"),
        format!(
            "starting server\n\nCaused by:\n    0: reading config\n    1: {os_text}\n\nLocation: {}",
            error.location()
        )
    );
    assert_eq!(
        format!("{error:#}"),
        format!("starting server: reading config: {os_text}")
    );
    let structure = format!("{error:#?}");
    assert!(
        structure.starts_with("Error {\n    message: \"starting server\",\n"),
        "{structure}"
    );
    assert!(structure
        .contains("layers: [\n        \"starting server\",\n        \"reading config\",\n    ],"));
}

#[test]
fn test_format_multi_line_causes_are_indented() {
    let error = Error::msg("first\nsecond").context("top");
    assert_eq!(
        format!("{error:?}"),
        format!(
            "top\n\nCaused by:\n    first\n    second\n\nLocation: {}",
            error.location()
        )
    );
}

#[test]
fn test_format_display_through_main_shape() {
    // `fn main() -> Result<()>` prints "Error: " followed by `{:?}`.
    let error = err!("failed to start");
    let printed = format!("Error: {error:?}");
    assert!(printed.starts_with("Error: failed to start\n\nLocation: "));
}

// ---------------------------------------------------------------- attacks

/// T1: unbounded context growth stays bounded.
#[test]
fn test_attack_million_layers_stay_bounded() {
    let layers: u32 = if cfg!(miri) { 300 } else { 1_000_000 };
    let mut error = Error::new(Leaf);
    for i in 0..layers {
        error = error.context(i);
    }
    assert_eq!(error.chain().len(), 65);
    let report = format!("{error:?}");
    assert!(
        report.contains(&format!(
            "Omitted: {} context layers (frame budget)",
            layers - 64
        )),
        "{report}"
    );
    // The innermost eight and the newest survive.
    let kept: Vec<String> = error.chain().map(|l| l.to_string()).collect();
    assert_eq!(kept.last().map(String::as_str), Some("leaf"));
    assert_eq!(kept[kept.len() - 2], "0");
    assert_eq!(kept[0], (layers - 1).to_string());
}

/// T5: a `source()` that returns itself, and one that never ends.
#[test]
fn test_attack_cyclic_and_endless_chains_stop() {
    #[derive(Debug)]
    struct Cycle;
    impl fmt::Display for Cycle {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("cycle")
        }
    }
    impl StdError for Cycle {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(self)
        }
    }
    let error = Error::new(Cycle).context("ctx");
    assert!(error.chain().len() <= 128);
    assert!(format!("{error:#}").ends_with("(chain truncated)"));
    assert!(format!("{error:?}").contains("(chain truncated)"));
    assert_eq!(error.root_cause().to_string(), "cycle");
}

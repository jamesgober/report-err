//! Attack tests: one per threat of PLAN 10.2 that is not tested where its
//! code lives. The others: T1 and T5 in `core.rs`
//! (`test_attack_million_layers_stay_bounded`,
//! `test_attack_cyclic_and_endless_chains_stop`), T6 and T16 for
//! `ExitReport` in `exit.rs`, T9 in `oom.rs`, T13 and T14 in `report.rs`,
//! T15 in `limits.rs`, and T8 is Miri, AddressSanitizer, and the fuzz
//! targets.

use std::error::Error as StdError;
use std::fmt::{self, Write};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

use define_err::Secret;
use report_err::{err, Error, RenderStyle};

#[derive(Debug)]
struct Leaf(u32);
impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "leaf {}", self.0)
    }
}
impl StdError for Leaf {}

fn rendered(error: &Error, style: RenderStyle) -> String {
    let mut text = String::new();
    let _ = error.render(&mut text, style);
    text
}

const STYLES: [RenderStyle; 3] = [
    RenderStyle::Message,
    RenderStyle::Chain,
    RenderStyle::Report,
];

// ---------------------------------------------------------------- T2

#[test]
fn test_attack_t2_many_large_attachments_stay_bounded() {
    let mut error = err!("x");
    for i in 0..10_000u32 {
        error = error.attach([i as u8; 1024]);
    }
    assert_eq!(error.attachments::<[u8; 1024]>().count(), 32);
    assert!(format!("{error:?}").contains("9968 attachments"));
    // A value that owns a large buffer counts its heap bytes (D-054).
    let error = err!("x").attach(vec![0u8; 1 << 20]);
    assert_eq!(error.attachments::<Vec<u8>>().count(), 0);
}

// ---------------------------------------------------------------- T3

/// Holds the canary in a field; attached opaquely, never printed.
#[derive(Debug)]
struct Holder {
    #[allow(dead_code)]
    token: String,
}

#[test]
fn test_attack_t3_secret_canary_never_printed() {
    const CANARY: &str = "canary-7f3a";
    let secret = Secret::new(CANARY.to_string());
    let error = Error::new(Leaf(1))
        .context(format!("logging in with {secret}"))
        .attach(Secret::new(CANARY.to_string()))
        .attach(Holder {
            token: CANARY.to_string(),
        })
        .note(format!("token was {secret}"));
    let mut texts = vec![
        format!("{error}"),
        format!("{error:#}"),
        format!("{error:?}"),
        format!("{error:#?}"),
        format!("{:?}", error.record()),
    ];
    texts.extend(STYLES.iter().map(|s| rendered(&error, *s)));
    for text in &texts {
        assert!(!text.contains(CANARY), "{text}");
    }
    // The secret is still there for code that asks for it by type.
    assert!(error.attachment::<Holder>().is_some());
}

// ---------------------------------------------------------------- T4

/// A `Display` that misbehaves in one way.
#[derive(Debug, Clone, Copy)]
enum Bad {
    Panics,
    Fails,
    Huge,
    Forever,
}

static FOREVER_WRITES: AtomicUsize = AtomicUsize::new(0);

impl fmt::Display for Bad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bad::Panics => panic!("display panicked on purpose"),
            Bad::Fails => Err(fmt::Error),
            Bad::Huge => {
                // 100 MiB in 64 KiB writes.
                let chunk = "y".repeat(64 * 1024);
                for _ in 0..1600 {
                    f.write_str(&chunk)?;
                }
                Ok(())
            }
            Bad::Forever => loop {
                FOREVER_WRITES.fetch_add(1, Ordering::Relaxed);
                f.write_str("z")?;
            },
        }
    }
}
impl StdError for Bad {}

#[test]
fn test_attack_t4_misbehaving_display_in_every_guarded_path() {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for bad in [Bad::Panics, Bad::Fails, Bad::Huge, Bad::Forever] {
        // As the root, as a context layer, and as a section.
        let errors = [
            Error::new(bad).context("outer"),
            Error::new(Leaf(2)).context(bad).context("outer"),
            err!("x").note(format!("{}", Leaf(3))).context(bad),
        ];
        for error in &errors {
            for style in STYLES {
                let text = rendered(error, style);
                assert!(text.len() <= (1 << 20) + 32, "{bad:?} {}", text.len());
            }
            let record = error.record();
            assert!(record.chain.iter().all(|c| c.message.len() <= 64 * 1024));
        }
    }
    std::panic::set_hook(quiet);
    // The endless writer was stopped, not left running: it got as far as
    // the caps and no further.
    assert!(FOREVER_WRITES.load(Ordering::Relaxed) < 64 << 20);
    let panicked = rendered(&Error::new(Bad::Panics), RenderStyle::Report);
    assert!(panicked.starts_with("<display panicked>"), "{panicked}");
    let failed = rendered(&Error::new(Bad::Fails), RenderStyle::Chain);
    assert_eq!(failed, "<display failed>");
}

// ---------------------------------------------------------------- T7

#[test]
fn test_attack_t7_flood_from_many_threads() {
    // 64 threads creating, enriching, reporting, and dropping errors. The
    // full-size run (1,000,000 each) is the soak test; this proves the paths
    // share nothing that serialises or grows.
    let per_thread = if cfg!(miri) { 20 } else { 10_000 };
    let threads: Vec<_> = (0..64)
        .map(|t| {
            std::thread::spawn(move || {
                let mut made = 0usize;
                for i in 0..per_thread {
                    let error = Error::new(io::Error::from(io::ErrorKind::NotFound))
                        .context("a")
                        .context(format!("b {t} {i}"))
                        .attach(i)
                        .trace();
                    let _ = error.report();
                    made += usize::from(error.chain().count() == 3);
                }
                made
            })
        })
        .collect();
    let made: usize = threads.into_iter().map(|t| t.join().unwrap()).sum();
    assert_eq!(made, 64 * per_thread);
}

// ---------------------------------------------------------------- T10

#[derive(Debug)]
struct DropPanics;
impl fmt::Display for DropPanics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("drop panics")
    }
}
impl StdError for DropPanics {}
impl Drop for DropPanics {
    fn drop(&mut self) {
        panic!("root drop panicked on purpose");
    }
}

#[test]
fn test_attack_t10_root_whose_drop_panics() {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    // The panic leaves `drop` like any value's; the layers are freed first
    // and nothing is used after it is freed (Miri runs this test too).
    let error = Error::new(DropPanics).context("a").context("b").attach(7u8);
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(error)));
    // Through the boundary, a payload whose `Drop` panics is contained.
    let caught =
        report_err::boundary(|| -> report_err::Result<()> { std::panic::panic_any(DropPanics) });
    std::panic::set_hook(quiet);
    assert!(dropped.is_err());
    let caught = caught.unwrap_err();
    assert!(caught.is_panic());
}

// ---------------------------------------------------------------- T11

#[test]
fn test_attack_t11_full_tree_on_a_64_kib_stack() {
    // Build, render, search, record, and drop a tree at `max_depth` on a
    // small stack: every walk is iterative (E-104 covers the drop alone).
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let mut error = Error::new(Leaf(0)).context("root layer");
            for level in 0..16 {
                let wide = (0..20).map(|i| err!("{level}.{i}").context("layer"));
                error = Error::group("level", std::iter::once(error).chain(wide));
            }
            for style in STYLES {
                let mut sink = Sink(0);
                let _ = error.render(&mut sink, style);
                assert!(sink.0 > 0);
            }
            let _ = write!(Sink(0), "{error:?}");
            let _ = write!(Sink(0), "{error:#}");
            assert_eq!(error.find::<Leaf>().map(|l| l.0), Some(0));
            let _ = error.record();
            drop(error);
        })
        .unwrap();
    worker.join().unwrap();
}

struct Sink(usize);
impl fmt::Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

// ---------------------------------------------------------------- T12

define_err::define_errors! {
    namespace = "APP";
    pub enum AppError {
        #[code(7, help = "Retry later.")]
        #[category(Unavailable)]
        #[public("The service is busy.")]
        Busy { host: String } => "host {host} at /srv/private/path refused",
    }
}

#[test]
fn test_attack_t12_public_message_never_carries_private_text() {
    let error = Error::new(AppError::Busy {
        host: "db-7.internal".to_string(),
    })
    .context("loading the account of user 42");
    // With no catalog entry, the category's text; the record carries the
    // variant's own `#[public]` text.
    let public = error.public_message().unwrap();
    assert_eq!(
        public,
        define_err::Category::Unavailable.default_public_message()
    );
    let location = error.location().to_string();
    for private in [
        "db-7.internal",
        "/srv/private",
        "user 42",
        location.as_str(),
        file!(),
    ] {
        assert!(!public.contains(private), "{private}");
    }
    let record = error.record();
    assert_eq!(record.public, "The service is busy.");
    assert!(!record.public.contains("db-7.internal"));
    assert!(!record.public.contains(file!()));
    // An uncoded error has no public message at all, never its `Display`.
    assert_eq!(err!("secret detail").public_message(), None);
}

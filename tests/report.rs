//! Reporting, records, and the creation hook (PLAN 7.16, 12.4 E-92, E-97 to
//! E-99, E-101). The reporter and the hook are installed once per process,
//! so this binary holds one test that runs the steps in order.
#![cfg(not(miri))]

use std::cell::Cell;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use define_err::{
    Annotate, CreationHook, CreationInfo, ErrorRecord, Event, Kinds, Reporter, TrailStep,
};
use report_err::{err, Error};

#[derive(Default)]
struct Capture {
    records: Mutex<Vec<ErrorRecord>>,
}

impl Reporter for Capture {
    fn receive(&self, event: &Event<'_>) {
        if let Some(record) = event.record() {
            if let Ok(mut records) = self.records.lock() {
                records.push(record.clone());
            }
        }
    }
}

static CAPTURE: std::sync::LazyLock<Capture> = std::sync::LazyLock::new(Capture::default);

/// What the hook is asked to do.
static HOOK_PANICS: AtomicBool = AtomicBool::new(false);
static HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static HOOK_MAKES_AN_ERROR: Cell<bool> = const { Cell::new(false) };
}

struct Scope;

impl CreationHook for Scope {
    fn on_create(&self, info: &CreationInfo<'_>, out: &mut dyn Annotate) {
        HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
        if HOOK_PANICS.load(Ordering::SeqCst) {
            panic!("hook panicked on purpose");
        }
        if HOOK_MAKES_AN_ERROR.with(Cell::get) {
            // Not called again for this one.
            let _inner = err!("made inside the hook");
        }
        out.field("request_id", &"r-42");
        out.field("type", &info.type_name());
        out.kind(Kinds::SECURITY);
    }
}

static SCOPE: Scope = Scope;

struct Loud;
impl fmt::Display for Loud {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("loud")
    }
}

fn records() -> Vec<ErrorRecord> {
    std::mem::take(&mut *CAPTURE.records.lock().unwrap())
}

#[test]
fn test_reporting_and_the_hook_in_order() {
    // E-98: no reporter installed: `true` once, nothing printed.
    let error = err!("unreported");
    assert!(error.report());
    assert!(!error.report());
    assert!(error.is_reported());

    // Errors made while the hook is installed by another thread (E-99).
    let stop = Arc::new(AtomicBool::new(false));
    let makers: Vec<_> = (0..4)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut made = 0usize;
                loop {
                    drop(err!("busy"));
                    made += 1;
                    if stop.load(Ordering::Relaxed) {
                        break made;
                    }
                }
            })
        })
        .collect();
    define_err::set_creation_hook(&SCOPE).unwrap();
    define_err::set_reporter(&*CAPTURE).unwrap();
    stop.store(true, Ordering::Relaxed);
    for maker in makers {
        assert!(maker.join().unwrap() > 0);
    }

    // The hook's fields and kinds, on creation only.
    let calls = HOOK_CALLS.load(Ordering::SeqCst);
    let error = Error::new(std::io::Error::other("disk"))
        .context("a")
        .context("b");
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), calls + 1);
    assert!(error.kinds().contains(Kinds::SECURITY));
    let text = format!("{error:?}");
    assert!(
        text.contains("\nFields:\n    request_id = r-42\n    type = ")
            && text.contains("io::error::Error"),
        "{text}"
    );

    // With `std`: a hook that creates an error is not called again for it,
    // and a hook that panics leaves a field. Without `std` the hook's
    // contract forbids both.
    if cfg!(feature = "std") {
        guards();
    }

    report_and_records(&error);
}

fn guards() {
    HOOK_MAKES_AN_ERROR.with(|f| f.set(true));
    let calls = HOOK_CALLS.load(Ordering::SeqCst);
    let _made = err!("outer");
    assert_eq!(HOOK_CALLS.load(Ordering::SeqCst), calls + 1);
    HOOK_MAKES_AN_ERROR.with(|f| f.set(false));

    // A hook that panics: the error is still made, with the field.
    HOOK_PANICS.store(true, Ordering::SeqCst);
    let survived = err!("still here");
    HOOK_PANICS.store(false, Ordering::SeqCst);
    assert_eq!(survived.to_string(), "still here");
    assert!(format!("{survived:?}").contains("creation_hook = panicked"));
}

fn report_and_records(error: &Error) {
    // `report` delivers the record once.
    records();
    let line = line!() + 1;
    assert!(error.report());
    assert!(!error.report());
    let delivered = records();
    assert_eq!(delivered.len(), 1);
    let record = &delivered[0];
    let messages: Vec<&str> = record.chain.iter().map(|c| c.message.as_str()).collect();
    assert_eq!(messages, ["b", "a", "disk"]);
    assert_eq!(record.chain[0].type_name, "context");
    assert!(record
        .fields
        .iter()
        .any(|(k, v)| k == "request_id" && v == "r-42"));
    assert!(record.kinds.contains(Kinds::SECURITY));
    assert!(record.handled);
    let reported = record.trail.last().unwrap();
    assert_eq!(reported.step, TrailStep::Reported);
    assert_eq!(reported.line, line);
    assert_eq!(record.trail[0].step, TrailStep::Created);

    // E-97: 64 threads, exactly one `true`.
    let shared = Arc::new(err!("shared"));
    let threads: Vec<_> = (0..64)
        .map(|_| {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || shared.report())
        })
        .collect();
    let wins = threads
        .into_iter()
        .map(|t| t.join().unwrap_or(false))
        .filter(|won| *won)
        .count();
    assert_eq!(wins, 1);
    assert_eq!(records().len(), 1);

    // E-92: `unraisable` with a reporter: delivered as unhandled.
    assert!(report_err::unraisable(err!("lost in a drop").context(Loud)));
    let delivered = records();
    assert_eq!(delivered.len(), 1);
    assert!(!delivered[0].handled);

    // E-101: a record of an error at every budget stays within the record's.
    let mut big = Error::new(std::io::Error::other("x".repeat(10_000)));
    for i in 0..100 {
        big = big
            .context(format!("layer {i} {}", "y".repeat(500)))
            .trace();
    }
    for i in 0..40 {
        big = big.note(format!("note {i}"));
    }
    let record = big.record();
    let limits = define_err::RecordLimits::DEFAULT;
    assert!(record.chain.len() <= limits.max_chain);
    assert!(record
        .chain
        .iter()
        .all(|c| c.message.len() <= limits.max_message_bytes));
    assert!(record.fields.len() <= limits.max_fields);
    assert!(record.trail.len() <= limits.max_trail);
    assert!(record.truncated);
}

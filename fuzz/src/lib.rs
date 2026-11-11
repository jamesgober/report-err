//! The operation interpreter the fuzz targets share (PLAN 12.3, 12.6): the
//! input is read as a sequence of operations on one error, and after every
//! step the invariants of 12.3 are checked against the budgets in force.

use std::error::Error as StdError;
use std::fmt::{self, Write};
use std::io;

use report_err::{Error, Limits, RenderStyle};

/// Reads bytes from the input; past the end, zeros.
pub struct Input<'a> {
    data: &'a [u8],
}

impl<'a> Input<'a> {
    pub fn new(data: &'a [u8]) -> Input<'a> {
        Input { data }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn byte(&mut self) -> u8 {
        match self.data.split_first() {
            Some((b, rest)) => {
                self.data = rest;
                *b
            }
            None => 0,
        }
    }

    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.byte(), self.byte()])
    }

    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.byte(), self.byte(), self.byte(), self.byte()])
    }

    /// Text of up to 64 bytes from the input (any bytes, made UTF-8), or,
    /// for a length byte of 255, a long run of one character.
    pub fn text(&mut self) -> String {
        let len = self.byte();
        if len == 255 {
            let c = char::from_u32(u32::from(self.byte()) + 0x20).unwrap_or('x');
            return c.to_string().repeat(1 << 16);
        }
        let take = usize::from(len % 65).min(self.data.len());
        let (now, rest) = self.data.split_at(take);
        self.data = rest;
        String::from_utf8_lossy(now).into_owned()
    }
}

/// A root error type the fuzzer can find and downcast to.
#[derive(Debug)]
pub struct Leaf(pub u32);

impl fmt::Display for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "leaf {}", self.0)
    }
}

impl StdError for Leaf {}

/// A context value that is not a string.
#[derive(Debug)]
pub struct Value(pub u32);

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "value {}", self.0)
    }
}

/// Counts bytes written, keeps none.
pub struct Sink(pub usize);

impl Write for Sink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

const FZ: define_err::Namespace = match define_err::Namespace::try_new("FZ") {
    Ok(namespace) => namespace,
    Err(_) => panic!("a valid namespace"),
};

/// What the root of the current error is, while it is known.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Root {
    Leaf,
    Io,
    Message,
    Unknown,
}

fn fresh(input: &mut Input<'_>) -> (Error, Root) {
    match input.byte() % 3 {
        0 => (Error::new(Leaf(u32::from(input.byte()))), Root::Leaf),
        1 => (
            Error::new(io::Error::from(io::ErrorKind::NotFound)),
            Root::Io,
        ),
        _ => (Error::msg(input.text()), Root::Message),
    }
}

/// Runs up to 200 operations from `input` and checks the invariants after
/// each, under `limits`.
pub fn run(input: &mut Input<'_>, limits: &Limits) {
    let (mut error, mut root) = fresh(input);
    for _ in 0..200 {
        if input.is_empty() {
            break;
        }
        let op = input.byte() % 24;
        error = match op {
            0 => error.context("literal layer"),
            1 => error.context(input.text()),
            2 => error.context(Value(input.u32())),
            3 => error.attach(input.u32()),
            4 => error.attach_display(input.text()),
            5 => error.attach(vec![0u8; usize::from(input.byte()) * 1024]),
            6 => error.note(input.text()),
            7 => error.warning(input.text()),
            8 => error.suggestion(input.text()),
            9 => error.trace(),
            10 => match define_err::Code::new(FZ, u32::from(input.u16())) {
                Some(code) => error.coded(code),
                None => error,
            },
            11 => match define_err::Severity::new(input.byte()) {
                Some(severity) => error.with_severity(severity),
                None => error,
            },
            12 => error.with_kinds(define_err::Kinds::from_bits(u64::from(input.u32()))),
            13 => error.with_exit_code(input.byte()),
            14 => {
                let n = usize::from(input.byte() % 8);
                let others: Vec<Error> = (0..n).map(|_| fresh(input).0).collect();
                root = Root::Unknown;
                Error::group(input.text(), std::iter::once(error).chain(others))
            }
            15 => {
                let mut error = error;
                error.push_child(fresh(input).0);
                error
            }
            16 => error.suppress(fresh(input).0),
            17 => {
                let pick = input.byte();
                let (a, b) = error.split(|e| e.to_string().len() % 2 == usize::from(pick % 2));
                root = Root::Unknown;
                match (a, b) {
                    (Some(a), _) if pick & 2 == 0 => a,
                    (_, Some(b)) => b,
                    (Some(a), None) => a,
                    (None, None) => fresh(input).0,
                }
            }
            18 => {
                let mut parts = error.flatten();
                root = Root::Unknown;
                match parts.len() {
                    0 => fresh(input).0,
                    1 => parts.remove(0),
                    _ => Error::group("flattened", parts),
                }
            }
            19 => {
                // Round trip through the standard boxed form.
                let before = format!("{error:#}");
                let boxed: Box<dyn std::error::Error + Send + Sync> = error.into();
                let back = Error::from_boxed(boxed);
                assert_eq!(format!("{back:#}"), before);
                root = Root::Unknown;
                back
            }
            20 => {
                let _ = error.report();
                error
            }
            21 => {
                let _ = error.record();
                error
            }
            22 => {
                let _ = error.find::<Leaf>();
                let _ = error
                    .suppressed()
                    .iter()
                    .find_map(|e| e.find::<io::Error>());
                error
            }
            _ => {
                let (e, r) = fresh(input);
                root = r;
                e
            }
        };
        check(&error, root, limits);
    }
}

/// The invariants of 12.3 that hold after any operation.
pub fn check(error: &Error, root: Root, limits: &Limits) {
    // `{}` is the outermost link; `{:?}` begins with it.
    let outer = error
        .chain()
        .next()
        .map(|l| l.to_string())
        .unwrap_or_default();
    let display = format!("{error}");
    assert_eq!(display, outer);
    assert!(format!("{error:?}").starts_with(&display));
    let _ = write!(Sink(0), "{error:#}");
    let _ = write!(Sink(0), "{error:#?}");

    // Budgets.
    assert!(error.chain().count() <= limits.max_chain as usize + 1);
    assert!(error.sections().count() <= limits.max_sections as usize);
    let attached = error.attachments::<u32>().count()
        + error.attachments::<String>().count()
        + error.attachments::<Vec<u8>>().count();
    assert!(attached <= limits.max_attachments as usize);
    assert!(error.children().len() <= limits.max_tree as usize);
    assert!(error.suppressed().len() <= limits.max_suppressed as usize);
    let trace_points = error
        .trail()
        .filter(|s| s.kind == report_err::StepKind::Trace)
        .count();
    assert!(trace_points <= 15);

    // Guarded output is bounded.
    for style in [
        RenderStyle::Message,
        RenderStyle::Chain,
        RenderStyle::Report,
    ] {
        let mut sink = Sink(0);
        let _ = error.render(&mut sink, style);
        assert!(sink.0 <= limits.max_render as usize + 32);
    }

    // Downcasts answer for the root type exactly.
    match root {
        Root::Leaf => assert!(error.downcast_ref::<Leaf>().is_some()),
        Root::Io => assert!(error.downcast_ref::<io::Error>().is_some()),
        Root::Message => assert!(error.downcast_ref::<Leaf>().is_none()),
        Root::Unknown => {}
    }
}

//! `display_adversary` (PLAN 12.6): roots, context layers, attachments, and
//! suppressed errors whose `Display` panics, fails, writes huge output,
//! writes control characters, or never stops, at points the fuzzer picks;
//! rendered through every guarded path, which must neither panic nor grow
//! without bound.
#![no_main]

use std::error::Error as StdError;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

use libfuzzer_sys::fuzz_target;
use report_err::{Error, RenderStyle};
use report_err_fuzz::{Input, Sink};

/// Misbehaves at write number `at`, in the way `mode` says.
#[derive(Debug, Clone)]
struct Adversary {
    mode: u8,
    at: u16,
    chunk: String,
    writes: u32,
}

impl fmt::Display for Adversary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut n: u32 = 0;
        loop {
            if n == u32::from(self.at) {
                match self.mode % 4 {
                    0 => panic!("adversary panicked"),
                    1 => return Err(fmt::Error),
                    _ => {}
                }
            }
            // Mode 3 never stops on its own: only a failing writer ends it.
            if self.mode % 4 != 3 && n >= self.writes {
                return Ok(());
            }
            f.write_str(&self.chunk)?;
            n = n.wrapping_add(1);
        }
    }
}

impl StdError for Adversary {}

fn adversary(input: &mut Input<'_>) -> Adversary {
    // Terminal escapes, a fake report line, and bidirectional overrides are
    // as likely as the fuzzer's own text.
    let chunk = match input.byte() % 4 {
        0 => "\u{1b}]0;title\u{7}\r\u{9b}2J\u{202e}".to_string(),
        1 => "\nError: fake\n".to_string(),
        _ => input.text(),
    };
    Adversary {
        mode: input.byte(),
        at: input.u16() % 512,
        chunk: if chunk.is_empty() {
            "x".to_string()
        } else {
            chunk
        },
        writes: u32::from(input.u16()) * 16,
    }
}

fuzz_target!(|data: &[u8]| {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut input = Input::new(data);
    let mut error = Error::new(adversary(&mut input));
    for _ in 0..8 {
        if input.is_empty() {
            break;
        }
        error = match input.byte() % 5 {
            0 => error.context(adversary(&mut input)),
            1 => error.attach_display(adversary(&mut input)),
            2 => error.suppress(Error::new(adversary(&mut input))),
            // A section is formatted when it is added (D-059): a panic there
            // is the caller's own, so it is caught here, not by the library.
            3 => {
                let bad = adversary(&mut input);
                match catch_unwind(AssertUnwindSafe(|| Error::msg("noted").note(bad))) {
                    Ok(noted) => {
                        let kept: usize = noted.sections().map(|s| s.text.len()).sum();
                        assert!(kept <= report_err::limits().max_bytes as usize);
                        error.suppress(noted)
                    }
                    Err(_) => error,
                }
            }
            _ => Error::group("group", [error, Error::new(adversary(&mut input))]),
        };
    }
    let limit = report_err::limits().max_render as usize + 32;
    for style in [
        RenderStyle::Message,
        RenderStyle::Chain,
        RenderStyle::Report,
    ] {
        let mut sink = Sink(0);
        let outcome = catch_unwind(AssertUnwindSafe(|| error.render(&mut sink, style)));
        assert!(outcome.is_ok(), "a panic escaped render");
        assert!(sink.0 <= limit, "render wrote {} bytes", sink.0);
    }
    let record = catch_unwind(AssertUnwindSafe(|| error.record()));
    assert!(record.is_ok(), "a panic escaped record()");
    let reported = catch_unwind(AssertUnwindSafe(|| error.report()));
    assert!(reported.is_ok(), "a panic escaped report()");
    std::panic::set_hook(quiet);
});

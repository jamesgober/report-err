//! `ops` (PLAN 12.6): a sequence of operations with arbitrary text and
//! sizes, the 12.3 invariants checked after every step.
#![no_main]

use libfuzzer_sys::fuzz_target;
use report_err_fuzz::{run, Input};

fuzz_target!(|data: &[u8]| {
    run(&mut Input::new(data), report_err::limits());
});

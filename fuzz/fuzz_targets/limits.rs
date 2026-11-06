//! `limits` (PLAN 12.6): arbitrary `Limits` through `configure`, then an
//! operation sequence under whatever budgets are in force. Run in fork mode
//! (`-fork=N`) so that many processes each make a first `configure`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use report_err::{configure, ConfigureError, Limits};
use report_err_fuzz::{run, Input};

/// Each field's value with its allowed range, as `configure` documents them.
fn ranges(l: &Limits) -> [(&'static str, u64, u64, u64); 9] {
    [
        ("max_frames", l.max_frames.into(), 4, 4096),
        ("max_attachments", l.max_attachments.into(), 0, 1024),
        ("max_sections", l.max_sections.into(), 0, 1024),
        ("max_suppressed", l.max_suppressed.into(), 0, 1024),
        ("max_tree", l.max_tree.into(), 1, 1 << 20),
        ("max_depth", l.max_depth.into(), 1, 64),
        ("max_bytes", l.max_bytes.into(), 1024, 16 << 20),
        ("max_chain", l.max_chain.into(), 8, 4096),
        ("max_render", l.max_render.into(), 4096, 64 << 20),
    ]
}

/// A value at or next to a range edge, inside the range, or anywhere.
fn pick(input: &mut Input<'_>, min: u64, max: u64) -> u32 {
    let v = match input.byte() % 6 {
        0 => min.saturating_sub(1),
        1 => min,
        2 => max,
        3 => max + 1,
        4 => min + u64::from(input.u32()) % (max - min + 1),
        _ => u64::from(input.u32()),
    };
    u32::try_from(v).unwrap_or(u32::MAX)
}

fuzz_target!(|data: &[u8]| {
    let mut input = Input::new(data);
    let mut wanted = Limits::DEFAULT;
    let edges = ranges(&Limits::DEFAULT);
    let fields: [&mut u32; 9] = [
        &mut wanted.max_frames,
        &mut wanted.max_attachments,
        &mut wanted.max_sections,
        &mut wanted.max_suppressed,
        &mut wanted.max_tree,
        &mut wanted.max_depth,
        &mut wanted.max_bytes,
        &mut wanted.max_chain,
        &mut wanted.max_render,
    ];
    for (field, (_, _, min, max)) in fields.into_iter().zip(edges) {
        if input.byte() & 1 == 1 {
            *field = pick(&mut input, min, max);
        }
    }
    let bad = ranges(&wanted)
        .into_iter()
        .find(|(_, v, min, max)| v < min || v > max);
    match (configure(wanted), bad) {
        (Ok(()), None) => assert_eq!(report_err::limits(), &wanted),
        (
            Err(ConfigureError::OutOfRange {
                field,
                value,
                min,
                max,
            }),
            Some(_),
        ) => {
            assert!(
                value < min || value > max,
                "{field} = {value} in {min}..={max}"
            );
        }
        (Err(ConfigureError::AlreadyConfigured), _) => {}
        (got, bad) => panic!("configure gave {got:?} for {bad:?}"),
    }
    run(&mut input, report_err::limits());
});

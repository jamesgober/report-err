//! RD-14: would formatting `err!("value {x}")` into the error's own node (one
//! allocation, formatting twice) beat formatting into a `String` and boxing
//! it (two allocations, formatting once)? Both sides are modelled with the
//! same 80-byte header so only the difference is measured.

use std::fmt::{self, Write};
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

/// What the node holds besides the message.
const HEADER: usize = 80;

/// Today: the message in a `String`, the `String` in the node.
struct Node {
    _header: [u8; HEADER],
    _message: String,
}

#[inline(never)]
fn two_allocations(x: u32) -> Box<Node> {
    Box::new(Node {
        _header: [0; HEADER],
        _message: format!("value {x} is out of range"),
    })
}

/// Counts the bytes a format would write.
struct Count(usize);
impl Write for Count {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

/// Appends to a buffer with room already made.
struct Append<'a>(&'a mut Vec<u8>);
impl Write for Append<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// RD-14: measure, allocate the header and the text together, write.
#[inline(never)]
fn one_allocation(x: u32) -> Vec<u8> {
    let args = format_args!("value {x} is out of range");
    let mut count = Count(0);
    let _ = count.write_fmt(args);
    let mut node = Vec::with_capacity(HEADER + count.0);
    node.resize(HEADER, 0);
    let _ = Append(&mut node).write_fmt(args);
    node
}

fn rd14(c: &mut Criterion) {
    let mut g = c.benchmark_group("rd14");
    g.bench_function("B5 model/two allocations", |b| {
        b.iter(|| black_box(two_allocations(black_box(7))))
    });
    g.bench_function("B5 model/one allocation, two passes", |b| {
        b.iter(|| black_box(one_allocation(black_box(7))))
    });
    g.finish();
}

criterion_group!(benches, rd14);
criterion_main!(benches);

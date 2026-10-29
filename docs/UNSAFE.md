<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>report-err</b><br>
    <sub><sup>THE UNSAFE CORE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./API.md" title="API reference"><b>API</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>UNSAFE</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="../dev/PLAN.md" title="Plan"><b>PLAN</b></a>
    </sup>
</div>
<br>

The audit record of `src/raw.rs`, the only module of report-err that contains `unsafe` code. It lists the layout the module builds, every invariant the rest of the crate relies on, the test that checks each, and how Miri runs them. First written at the `0.2.0` design lock; the module landed in `0.3.0`. Every version that changes `src/raw.rs` updates this file in the same change.

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## Why there is `unsafe` code at all

An `Error` is one machine word, the same as `anyhow::Error`: `Result<(), Error>` and `Option<Error>` are 8 bytes on 64-bit targets, and a list of errors (children, suppressed errors, `Errors`) holds 8 bytes per entry. Safe Rust gives either two words (a `Box<dyn Error>` is a pointer and a vtable) or one word with a second allocation for every error (a box holding a box). The first doubles the size of the most common return type in application code; the second doubles the cost of creating an error. The measured comparison is in [`../dev/PLAN.md`](../dev/PLAN.md) 7.4 (decision RD-3). The technique, a hand-built vtable behind one thin pointer, is the one anyhow and eyre have shipped for years.

The crate root is `#![deny(unsafe_code)]`; only `mod raw` carries `#[allow(unsafe_code)]`. If the Miri job or the 1.0 audit ever finds a problem in this module that cannot be fixed, the crate moves to the two-word safe representation before 1.0, with no change to the public API.

## Layout

One heap allocation per error, made with `Box::new(Node<P>)` and kept as a pointer to its first field (`Box::leak`, then `NonNull`):

```text
 Error (8 bytes) ──▶ Node<P>, #[repr(C)]
                     ├─ Header (first field; 72 bytes on 64-bit in 0.3)
                     │    vtable    &'static VTable          one per payload type P
                     │    origin    &'static Location        where the error was created
                     │    ext       Option<NonNull<Ext>>     made on the second context layer
                     │    frames    u16                      context layers stored
                     │    values    u16                      of those, layers holding a non-string value
                     │    omitted   u32                      layers dropped at the frame budget
                     │    first     MaybeUninit<Frame>       the innermost layer (when frames >= 1)
                     └─ payload: P                           the root error, inline

 Frame (40 bytes [measured]): location, value (&'static str | String | Box<dyn Display>), link
 Ext (a second allocation): inline: [MaybeUninit<Frame>; 4], spill: Vec<Frame>
```

The payload `P` is the user's error `E`, or one of three `#[repr(transparent)]` wrappers: `MessageError<M>` (`Error::msg`, `err!`), `DisplayError<M>` (context on `None`), and `BoxedError` (`from_boxed`). The vtable holds, for `P`: `drop` (frees the node as a `Node<P>`), `payload` (a `NonNull<dyn Error + Send + Sync>` to the payload, made by projecting to the field; for a boxed root, to the boxed error), `take` (moves the payload out and frees the rest), and `type_id` (the type a caller downcasts to: `M` for a message wrapper, `Box<dyn Error + Send + Sync>` for a boxed root). Code, flags, severity, and exit fields join the header in `0.4` and `0.5`.

## Invariants

Every `unsafe` block carries a `// SAFETY:` comment naming the invariant it relies on; every `unsafe fn` has a `# Safety` section. The lints `unsafe_op_in_unsafe_fn`, `clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`, `clippy::ptr_as_ptr`, `clippy::cast_ptr_alignment`, and `clippy::borrow_as_ptr` are denied for the whole crate.

| Id | Invariant | Test |
|---|---|---|
| I-1 | `Header` is the first field of `#[repr(C)] Node<E>`, so the node pointer and the header pointer are the same address; a `const` assertion checks `offset_of!(Node<E>, header) == 0` for every payload wrapper. | compile-time assertion; `test_raw_header_offset_is_zero` |
| I-2 | A header's vtable was built for exactly the `E` of its node; it is written once in `RawError::new::<E>` and never changed. | review; vtables are `&'static` constants produced only inside `new` |
| I-3 | The stored pointer comes from `Box::into_raw` of the whole `Node<E>`, never from a reference to a field, so it carries provenance for the whole allocation. Payload access projects from that pointer to the payload field. | Miri with strict provenance, Stacked Borrows and Tree Borrows |
| I-4 | The allocation is freed exactly once, with `Box::from_raw` as a `Node<E>` in the vtable's `drop` for the same `E`. | Miri leak and double-free detection; `test_raw_drop_runs_payload_drop_once_and_take_moves_payload` |
| I-5 | `take` compares the `TypeId`, moves the payload out with `ptr::read`, drops the other header fields, and frees the allocation without dropping the payload again. | `test_raw_drop_runs_payload_drop_once_and_take_moves_payload` with drop counters |
| I-6 | Every cast from an erased pointer to `&T` or `&mut T` follows a `TypeId` comparison inside the same vtable function. | `test_raw_payload_ref_wrong_type_is_none` for every wrapper |
| I-7 | `Error: Send + Sync` because every constructor requires `E: Send + Sync + 'static` and every header field is `Send + Sync`; the `unsafe impl`s state this. | compile-fail doc tests in the crate root: a `!Send`, `!Sync`, or non-`'static` error does not convert |
| I-8 | `&mut Header` is produced only from `&mut RawError`, so it never coexists with a shared reference. | the borrow checker; Miri |
| I-9 | A zero-sized payload still gives a non-zero-sized node (the header), so the allocation is never zero-sized. | `test_raw_zero_sized_payload` |
| I-10 | Over-aligned payloads (64 and 4096 bytes) are placed correctly; the node's alignment is the larger of the header's and the payload's. | `test_raw_over_aligned_payload_64`, `_4096` |
| I-11 | A payload whose `Drop` panics: the panic propagates out of `drop(error)` like any value's, and Miri reports no use after free; the node is still freed: Miri reports neither a leak nor a use after free (0.3.0 result). | `test_raw_payload_drop_panics` (unwind builds) |
| I-12 | Every frame's link points to the next layer inward, or, with bit 0 set, to its own node; it is derived by raw projection from the pointer that layer's allocation was made with (the node pointer, the extension block's pointer, or the spill vector's buffer pointer), never from a reference. After any move of frames (a spill reallocation, a dropped middle layer), every link from that layer outward is rewritten before `&mut` access ends. A frame never outlives the node that holds it. | `test_raw_frame_source_walks_every_layer` (0 to 20 layers), `test_raw_spill_growth_keeps_links` (100 layers), `test_raw_max_frames_drops_oldest_middle_layer`, `test_raw_remove_inline_layer_pulls_spill_frame_in` |
| I-13 | Code reachable from `deref`, `deref_mut`, and a frame's `source()` never creates a reference covering a whole `Header`, `Node<P>`, or `Ext`; it projects to the field it reads and creates a reference only to that field or to the one frame it returns, so a `&mut` from `DerefMut` is never invalidated by a `source()` walk. | `test_raw_deref_mut_then_source_then_reuse`, `test_deref_mut_then_read_again` |

## The `source()` chain view

An `Error` dereferences to the outermost context layer as `&dyn Error`, and each layer's `source()` returns the next one inward, ending at the root error and its own sources (PLAN 7.6.1, decisions D-003 and D-011). The context layers are stored flat (the first in the header, the rest in the extension block), so a layer does not own the layer below it; it keeps a **link** to it.

- Each frame's `link` points to the next frame inward. The innermost frame's link is its node pointer with bit 0 set (`wrapping_byte_add(1)`, which keeps provenance; headers are 8-aligned); its `source()` clears the bit and asks the vtable for the payload. `source()` is one load and one branch.
- Links are derived only from allocation pointers by raw projection (I-12): the node pointer for the header's `first` slot, the extension block's own pointer for the inline slots (the block is made with `Box::leak` and kept raw, never turned back into a `Box` until it is freed), and the spill vector's buffer pointer (`Vec::as_ptr`, documented to make no reference to the elements).
- A frame moves only when the spill vector reallocates or when the frame budget drops a middle layer. Both happen under `&mut`; afterwards every link from the moved layer outward is rewritten. During development Miri caught one ordering bug here (after a reallocation the newest layer was relinked before it was counted, so its link pointed into the freed buffer); `test_raw_spill_growth_keeps_links` covers it.
- Stepping from `&self` to a neighbouring slot is not used: it is undefined behaviour under Stacked Borrows, and Miri rejects it ("that tag does not exist in the borrow stack for this location"); Tree Borrows happens to accept it, which is why both models run in CI.

Earlier designs kept the node pointer in each frame and found the neighbour by comparing addresses (no link to maintain), or kept each frame's index; the direct link was chosen by benchmark (PLAN 7.6.1: level with anyhow at one layer, 11% to 18% faster from three layers on).

## Running Miri

```text
rustup +nightly component add miri rust-src
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test
MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-tree-borrows" cargo +nightly miri test
```

CI runs both on every push and pull request (`cargo miri test --lib --tests`), with default features and with `--no-default-features`, and an AddressSanitizer job runs the same tests natively on Linux. Locally they run before any change to `src/raw.rs` is pushed. Excluded from Miri: the doc tests (they read files, which Miri isolates, and hold no `unsafe` code of their own), the allocation-count test binary (its global allocator forwards to `System`, which makes Miri interpret the platform allocator), and tests that read files.

## Changes by version

| Version | Change to `src/raw.rs` |
|---|---|
| 0.2.0 | none (the module does not exist yet); this file written; the chain view prototyped and checked under Miri |
| 0.3.0 | the module written: the node and its vtable, the three payload wrappers, the flat context store with direct links, invariants I-1 to I-13 each with a test; clean under Miri (strict provenance, Stacked Borrows and Tree Borrows) and AddressSanitizer |

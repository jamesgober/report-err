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

The audit record of `src/raw.rs`, the only module of report-err that contains `unsafe` code. It lists the layout the module builds, every invariant the rest of the crate relies on, the test that checks each, and how Miri runs them. First written at the `0.2.0` design lock, before the module exists (it lands in `0.3.0`); every version that changes `src/raw.rs` updates this file in the same change.

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## Why there is `unsafe` code at all

An `Error` is one machine word, the same as `anyhow::Error`: `Result<(), Error>` and `Option<Error>` are 8 bytes on 64-bit targets, and a list of errors (children, suppressed errors, `Errors`) holds 8 bytes per entry. Safe Rust gives either two words (a `Box<dyn Error>` is a pointer and a vtable) or one word with a second allocation for every error (a box holding a box). The first doubles the size of the most common return type in application code; the second doubles the cost of creating an error. The measured comparison is in [`../dev/PLAN.md`](../dev/PLAN.md) 7.4 (decision RD-3). The technique, a hand-built vtable behind one thin pointer, is the one anyhow and eyre have shipped for years.

The crate root is `#![deny(unsafe_code)]`; only `mod raw` carries `#[allow(unsafe_code)]`. If the Miri job or the 1.0 audit ever finds a problem in this module that cannot be fixed, the crate moves to the two-word safe representation before 1.0, with no change to the public API.

## Layout

One heap allocation per error, made with `Box::new(Node<E>)` and erased to a pointer to its first field:

```text
 Error (8 bytes) ──▶ Node<E>, #[repr(C)]
                     ├─ Header (first field, about 80 bytes on 64-bit [est.])
                     │    vtable    &'static VTable          one per root type E
                     │    origin    &'static Location        where the error was created
                     │    code      Option<Code>             define-err's NonZeroU64 niche
                     │    flags     AtomicU32                reported, panicked, ...
                     │    severity  u8, exit u8, spare u16
                     │    ext       Option<Box<Ext>>         allocated on first need
                     │    first     Option<Frame>            the innermost context layer
                     └─ payload: E                           the root error, inline

 Frame (40 bytes [measured]): location, value (&'static str | String | Box<dyn Display>), node pointer
 Ext: four inline frame slots, then a Vec of frames; attachments, sections, children,
      suppressed errors, trace points, hook fields, kinds, backtrace, budget counters
```

The vtable holds, for the root type `E`: `drop` (frees the node as a `Node<E>`), `payload_error` (a `NonNull<dyn Error + Send + Sync>` to the payload, made by projecting to the field), `payload_type` (the user-visible `TypeId`: `M` for a message root, not the wrapper), `payload_ptr` and `take` (typed access and move-out, each after a `TypeId` comparison), `type_name`, and, for errors built with `from_info`, `info`.

## Invariants

Every `unsafe` block carries a `// SAFETY:` comment naming the invariant it relies on; every `unsafe fn` has a `# Safety` section. The lints `unsafe_op_in_unsafe_fn`, `clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`, `clippy::ptr_as_ptr`, `clippy::cast_ptr_alignment`, and `clippy::borrow_as_ptr` are denied for the whole crate.

| Id | Invariant | Test |
|---|---|---|
| I-1 | `Header` is the first field of `#[repr(C)] Node<E>`, so the node pointer and the header pointer are the same address; a `const` assertion checks `offset_of!(Node<E>, header) == 0` for every payload wrapper. | compile-time assertion; `test_raw_header_offset_is_zero` |
| I-2 | A header's vtable was built for exactly the `E` of its node; it is written once in `RawError::new::<E>` and never changed. | review; vtables are `&'static` constants produced only inside `new` |
| I-3 | The stored pointer comes from `Box::into_raw` of the whole `Node<E>`, never from a reference to a field, so it carries provenance for the whole allocation. Payload access projects from that pointer to the payload field. | Miri with strict provenance, Stacked Borrows and Tree Borrows |
| I-4 | The allocation is freed exactly once, with `Box::from_raw` as a `Node<E>` in the vtable's `drop` for the same `E`. | Miri leak and double-free detection; `test_raw_drop_runs_payload_drop_once` |
| I-5 | `take` compares the `TypeId`, moves the payload out with `ptr::read`, drops the other header fields, and frees the allocation without dropping the payload again. | `test_raw_take_moves_payload_and_frees_rest` with drop counters |
| I-6 | Every cast from an erased pointer to `&T` or `&mut T` follows a `TypeId` comparison inside the same vtable function. | `test_raw_payload_ref_wrong_type_is_none` for every wrapper |
| I-7 | `Error: Send + Sync` because every constructor requires `E: Send + Sync + 'static` and every header field is `Send + Sync`; the `unsafe impl`s state this. | compile-fail tests: a `!Send` or `!Sync` error does not convert |
| I-8 | `&mut Header` is produced only from `&mut RawError`, so it never coexists with a shared reference. | the borrow checker; Miri |
| I-9 | A zero-sized payload still gives a non-zero-sized node (the header), so the allocation is never zero-sized. | `test_raw_zero_sized_payload` |
| I-10 | Over-aligned payloads (64 and 4096 bytes) are placed correctly; the node's alignment is the larger of the header's and the payload's. | `test_raw_over_aligned_payload_64`, `_4096` |
| I-11 | A payload whose `Drop` panics: the panic propagates out of `drop(error)` like any value's, and Miri reports no use after free; whether the allocation is freed follows `Box`'s behaviour and is recorded here from the test result. | `test_raw_payload_drop_panics` (unwind builds) |
| I-12 | Every frame's node pointer is a copy of its own error's node pointer, written when the frame is created and only read afterwards; a frame never outlives the node that holds it. A frame finds its neighbours by comparing addresses, never by pointer arithmetic from `&self`. | `test_raw_frame_source_walks_every_layer` (1, 2, 5, and `max_frames` layers, after a spill, after a middle layer was dropped) |
| I-13 | Code reachable from `deref`, `deref_mut`, and a frame's `source()` never creates a reference covering a whole `Header`, `Node<E>`, or `Ext`; it projects to the field it reads, so a `&mut` from `DerefMut` is never invalidated by a `source()` walk. | `test_raw_deref_mut_then_source_then_reuse` |

## The `source()` chain view

An `Error` dereferences to the outermost context layer as `&dyn Error`, and each layer's `source()` returns the next one inward, ending at the root error and its own sources (PLAN 7.6.1, decision D-003). The context layers are stored flat (the first in the header, the rest in the extension block), so a layer does not own the layer below it; it finds it.

Two ways to find it were considered, and the one that looks simplest is unsound:

- **Pointer arithmetic from `&self`** (step from one slot to the previous one, or back to the header). Inside `source(&self)`, `self` is a reference whose permission covers only the frame's own bytes. Stepping outside them is undefined behaviour under Stacked Borrows; Miri rejects it ("that tag does not exist in the borrow stack for this location"). Tree Borrows happens to accept it, which is why both models run in CI.
- **A stored node pointer and address comparison** (the design). Each frame keeps a copy of the pointer made by `Box::into_raw` (I-3, I-12), which carries permission for the whole node and, through the header's `ext` box, for the extension block. The frame compares its own address with the addresses of the slots (comparing addresses reads no memory and needs no permission) to learn its position, then reads the previous slot through projections from the node pointer (I-13).

The prototype of this design (2026-10-23) ran clean under Miri with `-Zmiri-strict-provenance`, under both aliasing models: walks of 0 to 20 layers, after a spill from the inline slots to the vector, after a middle layer was dropped, a `DerefMut` borrow used again after a `source()` walk, a moved error, and a walk on another thread.

## Running Miri

```text
rustup +nightly component add miri rust-src
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test
MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-tree-borrows" cargo +nightly miri test
```

CI runs both on every push and pull request, with default features and with `--no-default-features`. Locally they run before any change to `src/raw.rs` is pushed. Tests that start processes are excluded from Miri runs.

## Changes by version

| Version | Change to `src/raw.rs` |
|---|---|
| 0.2.0 | none (the module does not exist yet); this file written; the chain view prototyped and checked under Miri |

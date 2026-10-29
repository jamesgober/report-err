<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>CHANGELOG</b>
</h1>
<p>
  All notable changes to <code>report-err</code> will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

---

## [Unreleased]

---

## [0.3.0] - 2026-10-29

The anyhow surface.

### Added

- `Error`: one type for any error, one machine word (`Result<(), Error>` is
  8 bytes on 64-bit targets), `Send + Sync + Unpin + UnwindSafe +
  RefUnwindSafe + 'static`. `Error::new`, `Error::msg`, `From<E>` for every
  `E: Error + Send + Sync + 'static` (what `?` calls), each recording the
  caller's location (`location()`).
- `Error::context`: the first layer costs no allocation, layers two to five
  share one block, a `&'static str` is stored as is and a `String` is moved
  in. At 64 layers the oldest middle layer is dropped and counted.
- `Deref`, `DerefMut`, `AsRef<dyn Error + Send + Sync>`, and
  `AsRef<dyn Error>` for `Error`: the outermost layer, whose `source()`
  chain walks every context layer, the root, and the root's own sources.
- `downcast_ref`, `downcast_mut`, `downcast`, `is`: the context values
  (newest first) and the root, as with anyhow; a message root answers to its
  message type.
- `chain()` (`Chain`: `ExactSizeIterator`, `FusedIterator`, `Clone`) and
  `root_cause()`, bounded to 128 links.
- `Context` on `Result<T, E>`, `Result<T, Error>`, and `Option<T>`:
  `context` and `with_context`. Sealed.
- `err!`, `format_err!`, `bail!`, `ensure!`. A literal with no arguments
  allocates nothing beyond the error; `ensure!(cond)` says
  `condition failed: <cond>`.
- Formatting: `{}` the outermost message, `{:#}` the chain on one line,
  `{:?}` anyhow's report followed by `Location:`, `{:#?}` a structural view;
  a layer is skipped when the one before it already ends with its text. None
  of them allocates.
- `into_std`, `from_boxed`, and `From<Error>` for `Box<dyn Error + Send +
  Sync>`, `Box<dyn Error + Send>`, and `Box<dyn Error>`; a box made by
  `into_std` gives the original error back.
- `Result<T, E = Error>` and `prelude`.
- Tests: unit tests for every module, the edge cases of the 0.3 surface, the
  T1 (a million context layers) and T5 (a cyclic `source()`) attack tests,
  exact allocation counts, compile-fail cases, and a parity suite of 27
  programs run against anyhow 1.0.104 with identical `{}`, `{:#}`, chain,
  root cause, `source()` walk, and downcast results.
- Benchmarks against anyhow (`benches/`, criterion), examples
  (`quick_start`, `inspect`, `migrate_from_anyhow`), `docs/MIGRATING.md`.
- CI: an AddressSanitizer job and a benchmark build job.

### Changed

- `dev/PLAN.md`: the `source()` mechanism chosen by benchmark (a direct link
  from each layer to the next, D-011), the chain walk bounded by its budget
  alone (D-020), measured numbers (section 11.4), decisions D-020 to D-026.
- `docs/UNSAFE.md`: the module as built; invariant I-11 recorded from its
  test (a payload whose `Drop` panics still frees the node).
- `docs/API.md`: measured costs replace estimates for the 0.3 items.

---

## [0.2.0] - 2026-10-29

The design lock. No functional code yet.

### Added

- `docs/API.md`: the public API of 1.0 (items ROOT, ERR, CTX, MAC, CHN, ATT,
  GRP, SUP, TRC, CFG, MAIN, BND, HOOK, STD, TST) with signatures, behaviour,
  costs, examples, and the version each item arrives in. New item ERR-19:
  `Error` dereferences to `dyn Error` with a `source()` chain through every
  context layer.
- `docs/UNSAFE.md`: the layout, invariants I-1 to I-13 with the test for
  each, the `source()` chain view, and how Miri runs.

### Changed

- `dev/PLAN.md`: the `source()` chain view measured on a prototype (size and
  allocation counts unchanged; Miri-clean under Stacked Borrows and Tree
  Borrows), define-err's answers to the four interface requests, decisions
  D-009 to D-015, a new defence idea; James's decisions on the one-word
  representation (D-016) and the macro name `err!` with `format_err!` as an
  alias (D-017); every other open decision accepted (D-018).
- define-err `0.2` is a dependency, by published version, with
  `default-features = false` and `alloc`; report-err's `std` feature turns
  on define-err's (D-019). Its types are first used in `0.4.0`.

---

## [0.1.0] - 2026-10-23

The scaffold and the plan. No functional code yet.

### Added

- Crate manifest (edition 2021, MSRV 1.81, no dependencies; features `std`,
  on by default, and `testing`), `Apache-2.0 OR MIT` license, README, REPS,
  `deny.toml`, `clippy.toml`, `rustfmt.toml`, `.editorconfig`,
  `.gitattributes`.
- `VERSION` constant and the crate-root lint set: `no_std` with `alloc`,
  `unsafe_code` denied (the `unsafe` core will be confined to one module),
  `missing_docs`, the documentation lints for `unsafe` blocks, and
  `unwrap`/`expect`/`panic`/indexing in library code denied. Targets without
  pointer-sized atomic compare-and-swap are refused with a clear build error.
- CI on Linux, macOS, and Windows, each on x86_64 and aarch64, on stable and
  1.81; a `no_std` build for `thumbv7em-none-eabihf`; Miri with strict
  provenance under Stacked Borrows and Tree Borrows; loom; documentation built
  with default, all, and no default features; `cargo package`; `cargo audit`
  and `cargo deny`.
- `dev/PLAN.md` (purpose, scope, requirements, the one-word representation and
  its `unsafe` core, the context store and its `source()` chain, the formatting
  rules, the API by tier and grade, the parity matrix, threats T1 to T16 with
  their tests and costs, performance targets, the testing plan with 107 edge
  cases, open decisions, and the decision log D-001 to D-008),
  `dev/DIRECTIVES.md`, and `dev/ROADMAP.md`.

[Unreleased]: https://github.com/jamesgober/report-err/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/jamesgober/report-err/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/report-err/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/report-err/releases/tag/v0.1.0

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

## [0.6.0] - 2026-11-06

Optimisation. Measured against anyhow, eyre, error-stack, and
`Box<dyn Error>` on Linux x86_64, Windows x86_64, and Linux aarch64;
`docs/PERFORMANCE.md` has every number.

### Changed

- The extras (attachments, sections, trace points, the backtrace, children,
  suppressed errors, fields) are a block of their own, made on first use.
  An error with only context layers now allocates 192 bytes for layers two
  and up instead of 656, so dropping one with three layers takes 0.58 to
  0.72 of anyhow's time (it was 1.00 to 1.30), and creating one 0.64 to
  0.91. An error with extras and fewer than two layers makes two blocks
  instead of one.
- The report (`{:?}`) writes the root's text from the buffer the repeat
  check filled instead of formatting it again, and writes locations and
  trace numbers without the formatter's integer path; a chain of one link
  is not buffered at all.
- An attachment that is a `String`, `Vec<u8>`, `Box<str>`, or `Box<[u8]>`
  counts its heap bytes against `max_bytes`, not only its own size.

### Performance

- Benchmarks B1 to B18 (PLAN 11.1) in `benches/`, run in CI on three
  platforms by the `Benchmarks` workflow. Each pull request measures the
  base branch on the same runner and fails when one of report-err's times
  is more than 5% and more than 1 ns slower; `benches/baseline.json` keeps
  the 0.6.0 ratios.
- Missed targets, each on the list for 0.9: `{:#}` (1.33 to 1.55 times
  anyhow) and `{:?}` (1.30 to 1.31), creation on aarch64 (8% to 26%
  slower), and the 64-thread run on two of the four platform and allocator
  pairs.
- One-allocation formatted messages (RD-14) measured and not adopted: the
  gain fell short of the 20% required on every platform.

---

## [0.5.0] - 2026-11-03

Feature complete. Requires define-err 0.5.

### Added

- Several causes: `Error::group`, `children`, `push_child`, `split`,
  `flatten`; `Errors` (gather, then `finish`); `CollectAll::collect_all`
  (every failure of an iterator, as `"{failed} of {total} items failed"`).
  The report prints them under `Causes (n):` as a tree.
- `find`, `find_all`, and `find_in_suppressed`: depth first over the tree,
  iterative and bounded, no allocation.
- Suppressed errors: `suppress`, `suppressed`, `Context::with_cleanup`, and
  `unraisable`; printed under `Suppressed:`.
- Budgets: `Limits`, `configure` (once, every field checked first), `limits`,
  and `ConfigureError::OutOfRange`.
- The panic boundary: `boundary`, `run`, `PANICS_CAUGHT`, `Error::from_panic`,
  `is_panic`. A panic error has `Kinds::PANIC`, category `Internal`,
  severity `FATAL`, and exit code 101.
- The creation hook: report-err calls define-err's hook for every error it
  creates, guarded against recursion and panics; its fields print under
  `Fields:`.
- Reporting: `report` (once, from any thread), `is_reported`, `record`.
- `render` and `RenderStyle`: guarded output, for paths that must not fail.
- `Chain` is a `DoubleEndedIterator`.
- `testing`: `assert_chain_contains`, `assert_code`, `assert_section`.
- `CollectAll` in the prelude.
- `docs/ARCHITECTURE.md`, `docs/PLATFORM-NOTES.md`; examples
  `panic_boundary`.

### Changed

- `ExitReport`, `record`, `report`, `render`, and `unraisable` write every
  `Display` through a guard: `<display panicked>` or `<display failed>`,
  and the rest goes on.
- `{:#?}` also shows the code and the counts of children, suppressed errors,
  and attachments.
- The `Caused by:` de-duplication now works across the 4,096 links that
  `max_chain` allows (it covered 128).
- Requires define-err 0.5 (from 0.4).

### Security

- Suppressed errors count toward `max_tree` and `max_depth`, so no error
  nests without bound through `suppress` and dropping one cannot overflow
  the stack.
- A panic payload whose `Drop` panics is contained, and the payload of that
  second panic is dropped too instead of leaked.

---

## [0.4.0] - 2026-11-02

Codes and reports. Requires define-err 0.4.

### Added

- Codes: `Error::code` (set by hand, else the innermost coded error in the
  chain, read with define-err's `innermost_code`), `Error::coded`, and
  `Context::coded` on `Result` and `Option`. On `None`, the message is the
  code's catalog title, or the code itself.
- Code-first arms of `err!`, `format_err!`, `bail!`, and `ensure!`:
  `err!(DbError::TIMEOUT, "after {ms} ms")`. A first argument that is not a
  `Code` is a type error that says so.
- `severity`, `category`, `kinds`, `with_severity`, `with_kind`. An
  `io::Error` root gets its category from its kind; a defined error with no
  code keeps its category.
- `public_message`: the catalog entry, the `from_info` root, or the
  category's default text; `None` without a code; never any `Display`.
- `configure_catalogs` and `ConfigureError`: optional, once per process.
- `Error::from_info` and `Error::info`: keeps per-value metadata the identity
  probe cannot carry, such as a `retry_after` field.
- `Code`, `Severity`, `Category`, and `Kinds` re-exported from define-err.
- Attachments: `attach`, `attachment`, `attachments`, `attach_display`, up to
  32, stored in the extension block.
- Sections: `note`, `warning`, `suggestion`, on `Error` and on `Context`, and
  `sections`, `Section`, `SectionKind`, up to 32 and 64 KiB in total.
- The trail: `trace` on `Error` and `Context`, and `trail`, `Trail`, `Step`,
  `StepKind`: the first 4 and latest 11 trace points merged with the contexts
  in order. No allocation past the first.
- Backtraces, captured when the error is made, as std's environment variables
  say, and never when a context layer is added: `backtrace`.
- `ExitReport`: `main` exits with a code chosen from the error. It escapes
  terminal and bidirectional controls, indents a message's continuation
  lines, cuts the report at 1 MiB, and ignores a closed or broken standard
  error.
- `exit_code` and `with_exit_code`: explicit, then the category's exit code,
  then 1. 0 becomes 1.
- `Ok`, as in anyhow.
- The report's blocks: sections, `Code:`, `Trace:`, `Attachments:`,
  `Omitted:`, and `Backtrace:`.
- Examples `codes`, `exit_report`, and `attachments`.

### Changed

- The header is 80 bytes on 64-bit targets (from 72): it holds the code and
  the severity and exit overrides, so setting them allocates nothing. The
  omitted-layer count moved to the extension block. `Error` is still one word.
- Requires define-err 0.4 (from 0.2).

### Security

- Invariant I-14 covers the new write-once slot; `src/raw.rs` is clean under
  Miri with strict provenance, Stacked Borrows and Tree Borrows.

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

[Unreleased]: https://github.com/jamesgober/report-err/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/jamesgober/report-err/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/report-err/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/report-err/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/report-err/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/report-err/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/report-err/releases/tag/v0.1.0

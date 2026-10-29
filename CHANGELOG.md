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

[Unreleased]: https://github.com/jamesgober/report-err/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jamesgober/report-err/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/report-err/releases/tag/v0.1.0

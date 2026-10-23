# report-err - Roadmap

> Path from scaffold to a stable 1.0. Each phase has hard exit criteria and ends with the release routine (bump, CHANGELOG, release notes, local gate, push, green CI, tag, GitHub release, `cargo doc`, `cargo publish`).
> Design, API item ids (ERR-, CTX-, MAC-, ...), requirements (FR-, NFR-), threats (T1 to T16), and edge cases (E-1 to E-107): [`PLAN.md`](./PLAN.md). Section 14 of the plan holds the full content of each phase; this file tracks it.
>
> **Anti-deferral rule:** no listed task moves to a later phase unless this file records the move and the reason, with a decision-log entry in `PLAN.md`. P0 items ship in the version listed and never move.
>
> **define-err dependency:** report-err names define-err's types from 0.4 on (`PLAN.md` section 3.4). The 0.2 design lock follows define-err's own 0.2 design lock; 0.4 waits for a published define-err with the interface locked. When a phase is blocked, record the blocker here and continue with the unblocked items.

## v0.1.0 - Scaffold
Exit: CI green on every job; the crate published to claim the name.
- [x] Repository `jamesgober/report-err` created and cloned to `error-collection/report-err` (name decided by James, D-001).
- [x] Manifest (edition 2021, MSRV 1.81, features `std` (default) and `testing`, no dependencies), README, CHANGELOG, REPS, dual license, `deny.toml`, `clippy.toml`, `rustfmt.toml`, `.editorconfig`, `.gitattributes`.
- [x] `src/lib.rs`: crate docs, `no_std` with `alloc`, `deny(unsafe_code)`, `deny(missing_docs)`, the `unsafe` lint set for the future `raw` module, the atomic compare-and-swap gate (D-007), `VERSION` (ROOT-01), one test.
- [x] `dev/PLAN.md` with James's two decisions applied (`Error`, D-002; a real `source()` chain, D-003), `dev/DIRECTIVES.md`, this file.
- [x] CI: Linux, macOS, Windows on x86_64 and aarch64, stable and 1.81; `no_std` build; Miri (Stacked Borrows, Tree Borrows, strict provenance); loom; package; security (audit, deny) (D-006).

## v0.2.0 - Design lock
Exit: James approves `docs/API.md`; the define-err interface (X-1 to X-12) is locked.
Requires: define-err's own 0.2 design lock (its API approved), so the names report-err uses from it are final.
- [ ] `docs/API.md` for every item in `PLAN.md` section 8 (signatures, behaviour, errors, costs, examples), each marked with the version it arrives in.
- [ ] `docs/UNSAFE.md` first version: layout and invariants I-1 to I-12.
- [ ] The chain view (`PLAN.md` 7.6.1, D-003) prototyped with the real `raw` layout, Miri-clean under both aliasing models, its cost measured and recorded; James gets the numbers and options first if one-word size or an allocation count would have to change.
- [ ] The define-err interface X-1 to X-12 agreed with define-err's plan and recorded in both plans; the four requests (X-1 niche, X-7 builders, X-8 `report_record`, X-9 creation-hook slot) answered, each with its fallback where declined.
- [ ] Every open decision in `PLAN.md` section 17 answered by James or accepted as recommended, recorded in the decision log; RD-3 (one word) and RD-18 (macro name) settled.
- [ ] The simplicity review: every Tier 1 use in `PLAN.md` 8.2 is one or two lines.
- [ ] One new defence idea recorded in `PLAN.md` section 19.

## v0.3.0 - Core: anyhow parity
API: ROOT-02, ROOT-03, ERR-01 to ERR-10, ERR-17, CTX-01, CTX-02, MAC-01 to MAC-03, CHN-01, CHN-02, MAIN-01, STD-01 to STD-03; `Deref`, `DerefMut`, and `AsRef` to `dyn Error` (D-003).
Exit: a kraken branch migrated by the `PLAN.md` 8.14.1 pass compiles and passes its tests (trial; the merge happens at 0.8); edge cases E-1 to E-49d and E-57 to E-65 that apply to the 0.3 surface pass.
- [ ] `src/raw.rs` with every invariant test; Miri clean under both aliasing models; ASan job active.
- [ ] Flat frames with the header slot and the chain view; formatting rules for message, causes, location; de-duplication rule.
- [ ] Counting-allocator tests for every 7.5 row that exists at 0.3; the anyhow parity suite for the 0.3 surface, including hand-written `source()` walks; the `no_std` build and tests.
- [ ] `docs/MIGRATING.md`, `docs/UNSAFE.md` updated.

## v0.4.0 - Extended: codes, sections, attachments, trace, exit codes
API: ROOT-04, ERR-11 to ERR-15, ERR-18, CTX-03 to CTX-06, MAC-04, ATT-01 to ATT-07, TRC-01 to TRC-05, CFG-05, MAIN-02, MAIN-03, HOOK-05.
Requires: define-err published (0.3 or later) with its interface locked at its 0.2.
Exit: a CLI (cli-forge or hive-exec, James's choice) builds against 0.4 with `fn main() -> ExitReport` and category exit codes.
- [ ] define-err added by published version; codes through `identify` and `innermost_code`; catalogs; `from_info`.
- [ ] `ExitReport` with exit-code selection and terminal escaping; backtraces; trace and trail; attachments and sections; the remaining `{:?}` blocks.
- [ ] Edge cases E-50 to E-56, E-66 to E-80, E-93, E-94.

## v0.5.0 - Feature complete
API: ERR-16, CTX-07, CHN-03 to CHN-06, GRP-01 to GRP-07, SUP-01 to SUP-04, CFG-01 to CFG-04, MAIN-04, BND-01 to BND-03, HOOK-01 to HOOK-04, STD-04, TST-01.
Exit: every P0 to P2 item implemented and tested on Linux, macOS, and Windows; L7's prototype can install a creation hook and receive reports.
- [ ] Several causes and collections, suppressed errors, budgets and `configure`, guarded rendering, the panic boundary and the abort subprocess test, the creation hook and reporting through define-err.
- [ ] Edge cases E-81 to E-107.
- [ ] `docs/ARCHITECTURE.md`, `docs/PLATFORM-NOTES.md`.

## v0.6.0 - Optimisation
Exit: every `PLAN.md` 11.1 target measured; each met or its reason recorded and reviewed.
- [ ] Benchmarks B1 to B18 against anyhow, eyre, error-stack, and `Box<dyn Error>` on Windows, Linux, and aarch64.
- [ ] RD-14 decided with numbers; the success-path finding re-measured on aarch64.
- [ ] `docs/PERFORMANCE.md`; criterion baselines; the 5% regression gate.

## v0.7.0 - Hardening
Exit: no open finding; every threat has a passing attack test.
- [ ] Fuzzing 24 hours per target; attack tests T1 to T16; `docs/SECURITY.md`.
- [ ] Soak: 8 hours on Windows and Linux, flat memory; the 64 KiB stack test.

## v0.8.0 - Alpha
Exit: both migrations green; every surprise fixed or recorded in `docs/MIGRATING.md`.
- [ ] kraken migrated for real on a branch James reviews; `kraken-lsp`'s unused anyhow dependency removed.
- [ ] One more application migrated (a Hive DB tool or WASP boot code) using codes, `ExitReport`, and `report()`.

## v0.9.0 - Beta and release candidates
Exit: two weeks without an API change; every document checked against the build; James's review done.
- [ ] **James's pre-1.0 review** of the API and anything he wants added before 1.0.
- [ ] API frozen; `docs/STABILITY-1.0.md` drafted with the requirement-to-test map.

## v1.0.0 - Stable
Exit: the audit passes, the definition of done in `DIRECTIVES.md` and `PLAN.md` section 16 holds, and James approves.
- [ ] Never published without James's explicit go.

# report-err - Roadmap

> Path from scaffold to a stable 1.0. Each phase has hard exit criteria and ends with the release routine (bump, CHANGELOG, release notes, local gate, push, green CI, tag, GitHub release, `cargo doc`, `cargo publish`).
> Design, API item ids (ERR-, CTX-, MAC-, ...), requirements (FR-, NFR-), threats (T1 to T16), and edge cases (E-1 to E-107): [`PLAN.md`](./PLAN.md). Section 14 of the plan holds the full content of each phase; this file tracks it.
>
> **Anti-deferral rule:** no listed task moves to a later phase unless this file records the move and the reason, with a decision-log entry in `PLAN.md`. P0 items ship in the version listed and never move.
>
> **define-err dependency:** report-err names define-err's types from 0.4 on (`PLAN.md` section 3.4). The 0.2 design lock follows define-err's own 0.2 design lock; 0.4 waits for a published define-err with the interface locked. When a phase is blocked, record the blocker here and continue with the unblocked items.

## v0.1.0 - Scaffold (DONE, 2026-10-23)
Exit: CI green on every job; the crate published to claim the name.
- [x] Repository `jamesgober/report-err` created and cloned to `error-collection/report-err` (name decided by James, D-001).
- [x] Manifest (edition 2021, MSRV 1.81, features `std` (default) and `testing`, no dependencies), README, CHANGELOG, REPS, dual license, `deny.toml`, `clippy.toml`, `rustfmt.toml`, `.editorconfig`, `.gitattributes`.
- [x] `src/lib.rs`: crate docs, `no_std` with `alloc`, `deny(unsafe_code)`, `deny(missing_docs)`, the `unsafe` lint set for the future `raw` module, the atomic compare-and-swap gate (D-007), `VERSION` (ROOT-01), one test.
- [x] `dev/PLAN.md` with James's two decisions applied (`Error`, D-002; a real `source()` chain, D-003), `dev/DIRECTIVES.md`, this file.
- [x] CI: Linux, macOS, Windows on x86_64 and aarch64, stable and 1.81; `no_std` build; Miri (Stacked Borrows, Tree Borrows, strict provenance); loom; package; security (audit, deny) (D-006).

## v0.2.0 - Design lock (DONE, 2026-10-29)
Exit: James approves `docs/API.md`; the define-err interface (X-1 to X-12) is locked.
Requires: define-err's own 0.2 design lock (its API approved), so the names report-err uses from it are final: approved by James on 2026-10-29.
- [x] `docs/API.md` for every item in `PLAN.md` section 8 (signatures, behaviour, errors, costs, examples), each marked with the version it arrives in (ERR-19 added for `Deref`, D-014).
- [x] `docs/UNSAFE.md` first version: layout, invariants I-1 to I-13, the chain view, Miri commands (2026-10-23).
- [x] The chain view (`PLAN.md` 7.6.1, D-003) prototyped, Miri-clean under both aliasing models with strict provenance, its cost measured and recorded (D-009): one word and every allocation count unchanged, so nothing to escalate.
- [x] The define-err interface X-1 to X-12 agreed with define-err's plan and recorded in both plans: all four requests accepted (D-012); James approved define-err's `docs/API.md` on 2026-10-29; define-err `0.2` is a dependency by version (D-019).
- [x] Every open decision in `PLAN.md` section 17 answered by James or accepted as recommended, recorded in the decision log: RD-3 and RD-18 decided by James on 2026-10-29 (D-016, D-017); the rest accepted (D-018).
- [x] The simplicity review: every Tier 1 use in `PLAN.md` 8.2 and in `docs/API.md` "Tier 1 on one screen" is one or two lines, with the names anyhow users already write.
- [x] One new defence idea recorded in `PLAN.md` section 19 (public text never falls back to `Display`, D-015).

## v0.3.0 - Core: anyhow parity (DONE, 2026-10-29)
API: ROOT-02, ROOT-03, ERR-01 to ERR-10, ERR-17, CTX-01, CTX-02, MAC-01 to MAC-03, CHN-01, CHN-02, MAIN-01, STD-01 to STD-03; `Deref`, `DerefMut`, and `AsRef` to `dyn Error` (D-003).
Exit: a kraken branch migrated by the `PLAN.md` 8.14.1 pass compiles and passes its tests (trial; the merge happens at 0.8); edge cases E-1 to E-49d and E-57 to E-65 that apply to the 0.3 surface pass.
- [x] `src/raw.rs` with every invariant test (I-1 to I-13); Miri clean under both aliasing models with strict provenance; ASan job active.
- [x] Flat frames with the header slot and the chain view, the per-link mechanism chosen by benchmark (D-011); formatting rules for message, causes, location; de-duplication rule.
- [x] Counting-allocator tests for every 7.5 row that exists at 0.3; the anyhow parity suite for the 0.3 surface, including hand-written `source()` walks; the `no_std` build and tests.
- [x] `docs/MIGRATING.md`, `docs/UNSAFE.md` updated.
- [x] kraken trial migration (2026-10-29): 2 manifest lines and 36 source lines changed by the find-and-replace pass, no hand edits; 1,974 tests passed with report-err as with anyhow, and the same 3 tests failed with both (they link a native program; "Linking failed" on the machine used). Details in `docs/MIGRATING.md`.
- [x] Benchmarks against anyhow (PLAN 11.4). One measured exception recorded for 0.6: `{:#}` (D-022).

## v0.4.0 - Extended: codes, sections, attachments, trace, exit codes (DONE, 2026-11-02)

Requires: define-err 0.4 (codes, severity, categories, kinds, the identity probe).
Exit: a CLI builds against 0.4 with `fn main() -> ExitReport` and category exit codes. Met on a copy of cli-forge (2026-11-02): it builds with `fn main() -> ExitReport`, exits 0 on success and 66 on a missing file. The trial found one bug, a location lost in `ExitReport::from`, fixed before the release and covered by `test_converted_error_keeps_the_location_of_the_into`.
- [x] define-err 0.4 by published version; codes through `identify` and `innermost_code`; catalogs (CFG-05); `from_info` (ERR-18, HOOK-05); code-first macro arms (MAC-04); `coded` on `Result` and `Option` (CTX-03); severity, category, kinds, public message (ERR-11 to ERR-15); the category step of the exit code; the `Code:` block of the report.
- [x] Attachments and `attach_display` (ATT-01 to ATT-04), sections (ATT-05 to ATT-07, CTX-04), `attach` and `trace` on `Context` (CTX-05, CTX-06), trace points and the trail (TRC-01 to TRC-03), lazy backtraces captured at the root only (TRC-04, TRC-05), `ExitReport` with terminal and bidirectional-control escaping and a 1 MiB cap (MAIN-02), `exit_code` and `with_exit_code` (MAIN-03), `Ok` (ROOT-04); the report's sections, `Trace:`, `Attachments:`, `Omitted:`, and `Backtrace:` blocks.
- [x] Edge cases E-50 to E-56 (`tests/codes.rs`, `tests/catalog.rs`), E-66 to E-80 (`tests/extended.rs`), E-69, E-70, E-93, E-94 as subprocesses (`tests/exit.rs`).

## v0.5.0 - Feature complete (DONE, 2026-11-03)
API: ERR-16, CTX-07, CHN-03 to CHN-06, GRP-01 to GRP-07, SUP-01 to SUP-04, CFG-01 to CFG-04, MAIN-04, BND-01 to BND-03, HOOK-01 to HOOK-04, STD-04, TST-01.
Exit: every P0 to P2 item implemented and tested on Linux, macOS, and Windows; L7's prototype can install a creation hook and receive reports. Met: `tests/report.rs` installs a creation hook and a reporter through define-err 0.5 and receives the records; L7 does not exist yet, so its prototype is that test (D-045).
- [x] Several causes and collections, suppressed errors, budgets and `configure`, guarded rendering, the panic boundary and the abort check (CI job `panic = "abort"`), the creation hook and reporting through define-err.
- [x] Edge cases E-81 to E-107 (`tests/groups.rs`, `tests/panic.rs`, `tests/report.rs`, `tests/limits.rs`, `tests/exit.rs`).
- [x] `docs/ARCHITECTURE.md`, `docs/PLATFORM-NOTES.md`.

## v0.6.0 - Optimisation (DONE, 2026-11-06)
Exit: every `PLAN.md` 11.1 target measured; each met or its reason recorded and reviewed.
- [x] Benchmarks B1 to B18 against anyhow, eyre, error-stack, and `Box<dyn Error>` on Windows, Linux, and aarch64, in CI (D-051; Apple silicon replaced by GitHub's Linux aarch64 runner).
- [x] RD-14 decided with numbers: not adopted (D-053); the success path re-measured on aarch64: level (1.03).
- [x] `docs/PERFORMANCE.md`; ratios stored in `benches/baseline.json`; the 5% gate active, against the base branch on the same runner (D-056).
- [x] From 0.4.0 (D-038): B9 fixed (0.58 to 0.72 times anyhow, D-052); B6 met on x86_64 and missed on aarch64; B12 and B13 measured, missed, and listed for 0.9.x below with their reasons.
- [x] 10.3's 0.6 idea: heap bytes of buffer attachments count against `max_bytes` (D-054).

## v0.7.0 - Hardening
Exit: no open finding; every threat has a passing attack test.
- [x] Fuzzing 24 hours per target (`ops`, `display_adversary`, `limits`) in WSL (D-057); a 60-second run of each in CI. The first run (from 2026-11-06 04:52 UTC) stopped early: each target hit its 30-second timeout on input the harness re-formats at every step, and a deletion of `/tmp` by other work ended the `limits` run (D-060). Restarted 2026-11-06 11:49 UTC with the D-059 fix, a 300-second timeout, and libFuzzer's temporary files in the home directory; that run also stopped early, on timeouts from a starved scheduler, not slow input (D-062), so all three run 24 hours again on the release code. The third run (from 2026-11-07 18:49 UTC) was interrupted after 66 minutes, with nothing found, when WSL was restarted; the fourth (from 2026-11-08 06:40 UTC) and fifth (from 07:47 UTC) were stopped, with nothing found, when the benchmark fix changed the format code again (D-063); the sixth started 2026-11-08 10:18 UTC on the final code (233122d; nothing in `src/` changed after it). The sixth found nothing: `display_adversary` ran 24 hours (1.5 million runs) and `limits` 24 hours (5.7 million runs); `ops` stopped at 22 hours 11 minutes on a spurious timeout when the WSL clock stepped back, and ran its remaining time from its corpus on 2026-11-09 from 10:30 UTC, 24 hours 12 minutes in all (182 thousand runs; each `ops` run re-renders the error at every step) (D-066).
- [x] Attack tests T1 to T16, each named in `docs/SECURITY.md`; T4's test found two hangs, fixed (D-055), and a third in the byte budget for section text (D-059); T6 extended to OSC, carriage return, C1, and a forged first line; T9 in a child process.
- [x] `docs/SECURITY.md`.
- [x] Soak: 8 hours on Windows and Linux, flat memory. The first run passed on Linux and failed on Windows at +1.75% resident memory in the plain half, the system heap settling over the first hour rather than a leak (D-061). Rerun from 2026-11-06 17:13 UTC on both, with live heap bytes counted exactly: passed on both (Windows 19.4 billion rounds, Linux 3.0 billion; live heap back within 7.7 KB of its start).
- [x] The 64 KiB stack test: build, render, search, record, and drop a full tree (`test_attack_t11_full_tree_on_a_64_kib_stack`; 10.3's 0.7 idea).

## v0.8.0 - Alpha
Exit: both migrations green; every surprise fixed or recorded in `docs/MIGRATING.md`.
- [ ] kraken migrated for real on a branch James reviews; `kraken-lsp`'s unused anyhow dependency removed. Rehearsed on 2026-11-08 against the 0.7 code: the find-and-replace pass alone, 2,803 tests passed and the same 4 native-linking tests failed as with anyhow.
- [ ] One more application migrated (a Hive DB tool or WASP boot code) using codes, `ExitReport`, and `report()`: hive-system's `hive-admin`, rehearsed on 2026-11-08 (an `HADM` catalog, `run`, a reporter that logs; exit codes 64, 65, 70, 73 as designed).
- [ ] The report prints a code's help (`Help:` under `Code:`) when its catalog entry is known; `docs/MIGRATING.md` says to call `configure_catalogs` for it. Found by the `hive-admin` rehearsal.
- [ ] The `Code:` line names the severity (`warning`), not its number.
- [ ] Ask define-err for a way to exit 74 (I/O) at normal severity: only `DataLoss` maps to 74 today. Found by the `hive-admin` rehearsal.
- [ ] Decide whether `category` reads an `io::Error` source below a coded wrapper, not only an `io::Error` root.

## v0.9.0 - Beta and release candidates
Exit: two weeks without an API change; every document checked against the build; James's review done; the pre-1.0 polish below complete.
- [ ] **James's pre-1.0 review** of the API and anything he wants added before 1.0.
- [ ] API frozen; `docs/STABILITY-1.0.md` drafted with the requirement-to-test map.

## v0.9.x - Pre-1.0 polish (before any `1.0.0-rc`)
James's standing rule (2026-10-29): "make sure to go back and fix any issues / perf tune / etc before we get close to release." Nothing accepted earlier in the series becomes final silently. This milestone runs inside 0.9.x and must be complete before the first `1.0.0-rc`; whatever is still open afterwards goes, with its reason and numbers, into the 1.0 readiness note for James (`docs/STABILITY-1.0.md`, section "Open items").
Exit: every item below is fixed, or measured again and listed in the readiness note with its reason; the performance pass and the security and `unsafe` review are recorded with dates, hardware, and results.
- [ ] **Revisit every accepted gap, deferred item, known issue, and missed target.** The list as of 2026-10-29 (add to it as the series goes; `PLAN.md` decision log is the source):
  - D-022: `{:#}` was 2.0 times slower than anyhow at 0.3 (the de-duplication buffer). Target: at least level with anyhow.
  - B13: `{:?}` was 16% slower than anyhow at 0.3 (within the 20% allowed, but recorded). Re-measure and tune.
  - D-011: the hand-written `source()` walk. Met at 0.3 (level at one layer, 11% to 18% faster from three); re-measure on Linux and aarch64 and keep the within-5% target as a regression gate.
  - D-020: the chain has no repeat check, only the 128-link budget. Look again for a deterministic way to stop a cycle early.
  - D-021: compile-fail cases are doc tests, not trybuild (no checked error text). Decide whether the messages themselves need a check.
  - D-024: what Miri does not run (doc tests, the allocation-count binary, file-reading tests). Find another way to cover each, or record why none is needed.
  - D-026: a boxed root answers `downcast_ref` only as `Box<dyn Error>`. Revisit once the API is otherwise frozen.
  - D-027 (`Trace:` printed only when the error travelled) and D-028 (`None` with a section becomes `no value`): confirm with real use from the 0.8 migrations.
  - RD-14 (one-allocation formatted messages): not adopted at 0.6 (D-053); measure again if the formatting path changes.
  - The 11.1 targets 0.6.0 missed (`docs/PERFORMANCE.md`), each to be fixed or measured again with its reason:
    - B12 `{:#}`: 1.33 / 1.55 / 1.51 times anyhow (target at least 20% faster). The de-duplication buffer (D-022) is the cost; look for a check that needs no copy of the root's text and does not format it twice (the bufferless matcher tried at 0.6 formatted it twice and was slower, D-052).
    - B13 `{:?}`: 1.30 / 1.31 times anyhow (allowance 20%); 4.0 to 4.2 times for a one-layer error, which prints `Location:` where anyhow prints nothing. Profile the report's writes; consider one pass that both de-duplicates and writes.
    - B9 drop and B8 create: met at 0.6 (0.58 to 0.72, 0.64 to 0.91); keep both under the gate and confirm on a quiet run.
    - B3 to B6 on aarch64: 1.11, 1.09, 1.08, 1.26 against "within 5%"; B8 with 3 layers on aarch64 0.91 against "at least 20% faster". Look at the creation path's code generation on aarch64 (the location store and the backtrace check).
    - B18 under contention: 1.09 on Linux x86_64 with the system allocator, 1.13 on aarch64 with mimalloc. Measure again with longer runs before treating as real.
    - One-layer `source()` walk and `chain()`: 7% to 39% slower than anyhow (2 to 3 ns), faster from three layers.
  - Every edge case or threat test that a version marked as partial or skipped.
- [ ] **Fresh performance tuning pass** over every benchmark of `PLAN.md` 11.1 against anyhow, eyre, and error-stack, on Windows, Linux, and aarch64, with criterion baselines refreshed; each regression or missed target fixed or listed.
- [ ] **Security and `unsafe` review, again**: `src/raw.rs` line by line against `docs/UNSAFE.md` (every `SAFETY:` comment still true, I-1 to I-13 and any later invariant still tested), Miri under both models and AddressSanitizer on the whole suite, fuzzing re-run, and every threat T1 to T16 re-checked against the code as it stands.
- [ ] The 1.0 readiness note lists every item still open, with its numbers and reason, for James's decision.

## v1.0.0 - Stable
Exit: the audit passes, the definition of done in `DIRECTIVES.md` and `PLAN.md` section 16 holds, and James approves.
- [ ] Never published without James's explicit go.

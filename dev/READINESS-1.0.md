# report-err - 1.0 readiness

> Where the crate stands against the definition of done (`PLAN.md` section 16), and everything accepted, deferred, or missed in the 0.x series that is still open. Nothing listed here becomes final silently. Last updated 2026-11-18, at 0.9.0.

## Decisions for James

| # | Decision | Recommendation |
|---|---|---|
| J-1 | The two real migrations (ROADMAP 0.10): the compiler on a branch you review, and a database admin tool using codes, `ExitReport`, and `report()`. Both are changes in other repositories. | Go. Both were rehearsed on 2026-11-08 against 0.7 (the compiler: 2,803 tests passed and the same 4 native-linking tests failed as with anyhow; the admin tool: exit codes 64, 65, 70, 73 as designed). Run them on 0.9 with define-err 0.10, together with the crates that move to define-err 0.10 at the same time. |
| J-2 | The pre-1.0 review of the API (ROADMAP 0.11). | Review `docs/API.md` as it stands at the last 0.x release candidate. The 0.8 simplicity audit (`dev/SIMPLICITY-AUDIT.md`) left nothing open; the pairs it kept are the anyhow surface or do different things, each with its reason there. |
| J-3 | The go for 1.0.0. | Only after define-err 1.0 (report-err re-exports its types, so it cannot be stable first), and after every line below is done or carries a reason you accept. |
| J-4 | The support window in `docs/STABILITY-1.0.md`. It is a public commitment. | The same as define-err's: security and soundness fixes for 1.x for 12 months after 2.0.0, on the 1.x minimum Rust version. |
| J-5 | Yank 0.8.0. It has the `DerefMut` soundness bug fixed in 0.8.1 (`docs/SECURITY.md` already marks it unsupported). | Yank it. 0.8.1 is a drop-in replacement. |
| J-6 | `REPS.md` is a byte-identical copy of the portfolio standard, and two of its lines do not follow the wording rule for public files. | Change it in the portfolio's copy, then copy it here again. |

## Definition of done, line by line

| Line (`PLAN.md` 16) | State |
|---|---|
| 1. P0 to P2 items implemented, documented, tested on Linux, macOS, Windows, stable and 1.81 | Done at 0.5; re-checked in the 0.8 simplicity pass. |
| 2. Every FR and NFR has a named test | Open: the tests exist, but no requirement-to-test map is written. It goes in `docs/STABILITY-1.0.md`. |
| 3. E-1 to E-107 pass | Done for the tests; E-50 to E-56, E-66 to E-107 carry their ids in the tests, E-1 to E-49 and E-57 to E-65 are tested by name only. The map in line 2 lists them. |
| 4. T1 to T16 attack tests, costs stated | Done at 0.7; T6 and T12 extended in 0.8.1 (control characters in `{:?}` and `render`, public text never from a probe). |
| 5. Fuzz, 24 hours per target, no open finding | Done at 0.7. Since then the fuzz invariant checks that `{:?}` holds no control character (0.8.1), with short runs per change; a fresh 24-hour run belongs to the 0.11 polish pass. |
| 6. Counting-allocator proofs; 8-hour soak without growth | Done at 0.7 (D-061); re-run with the 0.11 polish pass. |
| 7. `docs/PERFORMANCE.md`; each target met or a reviewed reason | Partly: the misses below are open for the tuning pass. |
| 8. Zero third-party runtime dependencies; deny and audit clean | Done; checked by CI on every push. |
| 9. `unsafe` only in `src/raw.rs`, invariants tested, Miri and ASan clean | Done: I-1 to I-15 (I-15 added in 0.8.1 for the boxed root's `DerefMut`); Miri under both models and ASan run on every push. A line-by-line pass over `src/raw.rs` is on the 0.11 list. |
| 10. `Result<(), Error>` is one word | Done; asserted at compile time. |
| 11. `no_std` + `alloc` on `thumbv7em-none-eabihf` | Done; built in CI on stable and 1.81. |
| 12. The compiler migrated and green; anyhow parity suite green | Parity suite green. The migration waits on J-1. |
| 13. CI green on every platform and mode | Done for every release so far. The benchmark gate builds base and head from one path since 2026-11-12 (D-086). |
| 14. Documents complete, no banned words or private paths | Done at 0.9.0, except J-6. `docs/STABILITY-1.0.md` is not written yet. |
| 15. Simplicity at the 1.0 audit | Passed at 0.8 (`dev/SIMPLICITY-AUDIT.md`); repeated at the 1.0 audit after J-2. |
| 16. Release routine for 1.0.0 | After J-3. |

## Open items from the 0.x series

| Item | Source | State |
|---|---|---|
| `Help:` line under `Code:` when the catalog entry is known | ROADMAP 0.10 | Open; small, report format only. |
| The `Code:` line names the severity (`warning`), not its number | ROADMAP 0.10 | Open; small, report format only. |
| Exit 74 (I/O) at normal severity: only `DataLoss` maps to 74 | ROADMAP 0.10 | A request for define-err; report-err follows its answer. |
| `category()` reading an `io::Error` below a coded wrapper, not only an `io::Error` root | ROADMAP 0.10 | Open decision; recommendation: no, the code's category wins, as today. |
| Facts computed once per record (five chain walks more than needed) | ROADMAP 0.10, D-079 | Open; needs its own benchmark run. |
| A creation hook turns 1 allocation into 5 | ROADMAP 0.10 | Open; changes `Extras`'s layout, so the allocation and size tables are measured again. |
| A code or severity read from a probed error, about 10 to 15 ns slower | D-082 | Kept: define-err's `Display` check is what refuses forged probes on Rust 1.81 to 1.89. A set code is unchanged. |
| B12 `{:#}` 1.33 to 1.55 times anyhow (target at least 20% faster) | `docs/PERFORMANCE.md`, D-022 | Open for the tuning pass: the de-duplication buffer is the cost. |
| B13 `{:?}` 1.30 times anyhow (allowance 20%) | `docs/PERFORMANCE.md` | Open: it also prints `Location:` and `Trace:`; level with 0.8.0 after the 0.8.1 escaping (D-073). |
| B3 to B6 on aarch64, 8% to 26% slower than anyhow | `docs/PERFORMANCE.md` | Open for the tuning pass. |
| B18 under contention, 9% to 13% slower on two platforms | `docs/PERFORMANCE.md` | Open: measure again with longer runs before treating as real. |
| One-layer `source()` walk and `chain()`, 2 to 3 ns slower than anyhow | `docs/PERFORMANCE.md` | Open; faster from three layers on. |
| A boxed root answers `downcast_ref` only as `Box<dyn Error>` | D-026 | Open; revisit once the API is otherwise frozen (J-2). |
| No repeat check in the chain, only the 128-link budget | D-020 | Kept unless a deterministic early stop is found. |
| What Miri does not run (doc tests, the allocation-count binary, file-reading tests, two multi-megabyte render tests) | D-024, D-073 | Each covered natively and under ASan; listed in `docs/UNSAFE.md`. |
| Without `std`, a creation hook that creates an error recurses | `docs/API.md` (the creation hook) | Documented; a process-wide reentrancy flag would skip the hook for other threads. Kept. |
| A define-err value given to `.context(..)` keeps no code | `docs/API.md` | Documented: `ok_or` keeps it. |
| define-err 1.0 and the semver trick | D-076, D-078 | report-err follows: one define-err per build, enforced by define-err's `links` and report-err's CI check. |

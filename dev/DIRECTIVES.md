# report-err - Engineering Directives

> Engineering standards and the definition of done for this project. Read alongside `REPS.md` (root, authoritative), [`PLAN.md`](./PLAN.md) (design and decisions), and [`ROADMAP.md`](./ROADMAP.md) (current phase). If anything here conflicts with `REPS.md`, `REPS.md` wins, except where `PLAN.md` records an approved exception (no async runtime, PLAN 6.8; a confined `unsafe` core, PLAN 6.9).

---

## 0. Philosophy

This crate is built and maintained to a production standard and treated as a flagship piece of work. Plan the full path, then build one verified step at a time. "Good enough" is treated as a defect. It replaces `anyhow` in every application of the portfolio, so it must be at least as small, as fast, and as easy to use, and it must never turn a report into a second failure.

---

## 1. What this is

report-err gives application code one error type, `report_err::Error`, that holds any error, gathers context as it travels up the stack, and prints a clear report when it reaches `main`. On top of `anyhow`'s API it adds error codes from define-err, a propagation trace, typed attachments, sections, several causes, suppressed errors, exit codes, and a panic boundary. See `PLAN.md` sections 2 to 4.

---

## 2. Engineering law (non-negotiable)

- **The anyhow API is the floor.** Every name anyhow users write (`Error`, `Result`, `Context`, `bail!`, `ensure!`, `chain`, `root_cause`, `downcast*`, `Deref` to `dyn Error` with a working `source()` chain) behaves as in anyhow unless `PLAN.md` 8.14.2 lists the difference and its reason.
- **One machine word.** `size_of::<Error>()`, `size_of::<Result<(), Error>>()`, and `size_of::<Option<Error>>()` equal `size_of::<usize>()`, asserted at compile time.
- **Allocation counts are contracts.** Every count in `PLAN.md` 7.5 and every "no allocation" claim in 12.5 has a counting-allocator test.
- **Unsafe.** `#![deny(unsafe_code)]` at the crate root; `unsafe` only in `src/raw.rs` (`#[allow(unsafe_code)]` on that module alone). Every block has a `// SAFETY:` comment naming an invariant from `PLAN.md` 7.3; every `unsafe fn` has a `# Safety` section. Miri runs on every change to `raw.rs`, with strict provenance, under Stacked Borrows and Tree Borrows.
- **Bounded.** Every growable part of an error has a budget (`PLAN.md` 7.18), checked before the allocation or the work; at a budget, data is dropped and counted, never printed unbounded, and nothing fails or panics.
- **Guarded rendering.** A user `Display` that panics, fails, or never stops writing must not turn a report into a second failure on the guarded paths.
- **Dependencies.** No third-party runtime dependency. define-err by published version only, never `path`. No async runtime.
- **`no_std` + `alloc`.** Everything in `PLAN.md` 7.19's `no_std` column builds and is tested without `std`.
- **Secrets.** Attachments are opaque unless attached for display; `Display` of an error is a private (log) message; `public_message()` is the only client-facing text.
- **Cross-platform.** Linux, macOS, and Windows on x86_64 and aarch64, stable and MSRV 1.81, in CI.
- **Production-ready.** `#![deny(missing_docs)]`; no `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!`/`unreachable!`/`dbg!`/indexing that can panic in library code; every public item has rustdoc with a runnable example; every claim in the docs maps to a test.
- **Performance.** No claim without criterion numbers; the targets in `PLAN.md` 11.1 are measured and published.

---

## 3. Definition of done

1. Compiles clean on Linux, macOS, and Windows (x86_64 and aarch64), stable and MSRV 1.81, and builds for `thumbv7em-none-eabihf` without `std`.
2. `fmt`, `clippy -D warnings` (all targets; default, all, and no default features), tests in the same three modes, `cargo doc -D warnings` with default, all, and no default features, and `cargo package` in a fresh target directory: clean on Windows and in WSL Ubuntu, stable and 1.81.
3. Miri clean (strict provenance; Stacked Borrows and Tree Borrows) on every test that touches `raw`; loom models pass.
4. `cargo audit` and `cargo deny check` pass; `cargo tree` shows no third-party runtime dependency.
5. A Tier 1 API exists and headlines the docs (`use report_err::{Context, Result};`, `bail!`, `ensure!`, `err!`, `?`).
6. Every threat in `PLAN.md` section 10 has an attack test that must fail to get through; every edge case in 12.4 that applies to the version has a test.
7. Hot-path changes carry benchmarks; no regression over 5%.
8. `README.md`, `docs/API.md`, and `CHANGELOG.md` updated in the same change; the matching `docs/release/vX.Y.Z.md` written before the tag; every version string bumped.
9. `PLAN.md` decision log updated for every design decision made during the work; at least one new defence idea per version in `PLAN.md` section 19.
10. v1.0.0 only after the full audit (API, docs, tests, benchmarks, security, the `unsafe` core line by line) is recorded in `docs/STABILITY-1.0.md` and James approves it.

---

## 4. Project-specific invariants

- An `Error` is one machine word and one allocation when it carries only its cause and location.
- The success path runs no report-err code.
- `source()` from `&*error` visits exactly the links `chain()` visits, in the same order.
- `{}` and `{:#}` never allocate and never resolve a backtrace.
- A budget is never exceeded; every dropped item is counted.
- `report()` delivers an error at most once.
- Nothing in report-err keeps a security-relevant error local: `report()` always goes through define-err's facade.

---

## 5. Writing style

- Plain, specific, human prose in every shipping file. No marketing voice.
- Banned in shipping files: "comprehensive", "robust", "seamless", "leverage", the em-dash character (U+2014) in prose, and emoji.
- Every security measure states its cost (latency, memory); estimates are labelled as estimates, measurements as measured.
- Release notes in the past tense.
- Commits: imperative present tense, descriptive subject line.

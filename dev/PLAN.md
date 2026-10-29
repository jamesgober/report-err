# report-err - Project Plan

> The design, strategy, and decision record for `report-err`. A living document: every design decision, idea, or open question about the crate is written here or in [`ROADMAP.md`](./ROADMAP.md) before it is acted on.
>
> **Read order:** [`../REPS.md`](../REPS.md), then [`DIRECTIVES.md`](./DIRECTIVES.md), then **this file**, then [`ROADMAP.md`](./ROADMAP.md).
>
> **Status:** `0.3.0`: the anyhow surface is built, with the `source()` chain of 7.6.1 measured against anyhow (11.4) and a trial migration of kraken that left its test results unchanged. `0.4.0` (codes, sections, attachments, the trace, exit codes) is next. The public API is in [`../docs/API.md`](../docs/API.md) and the `unsafe` core's audit in [`../docs/UNSAFE.md`](../docs/UNSAFE.md). Numbers marked [est.] are estimates; numbers marked [measured] were measured on the machine named in 11.4.
>
> **Two decisions by James override the original design** (2026-10-22): the main type is named `Error`, like `anyhow::Error`, so migrating from anyhow is a change of crate name (D-002, reversing RD-2's `Fault`); and an `Error` dereferences to a `&dyn Error` whose `source()` walks every context layer and then the root cause, as anyhow's does (D-003, reversing RD-4's trade-off). This plan is written with both applied; section 7.6 describes how the flat context store keeps its allocation counts while every layer becomes a link of a real `source()` chain.

---

## 1. Header

| Field | Value |
|---|---|
| Crate | `report-err` (import path `report_err`), named by James (D-001) |
| Library role | L3 "report" of the error collection: functionality F8 (dynamic errors), F9 (chain tools), F10 (error collections), F11 (lazy backtraces), plus research additions R2, R3, R7 (rules), R11, R12, R13 (data), R14, R15, R24, R25 |
| Collection folder | `error-collection/report-err` |
| Repository | [`github.com/jamesgober/report-err`](https://github.com/jamesgober/report-err) (public) |
| Current state | `0.1.0` scaffold: manifest, lint set, CI, this plan. No functional code. |
| Depends on | L1 "definitions" (`define-err`) by published version, for `Code`, `Severity`, `Kinds`, `Category`, `ErrorRecord`, and the reporting facade, from `0.4.0`. Section 3.4 lists the exact interface requested from L1. |
| Replaces | `anyhow` (dynamic errors, context, `bail!`, `ensure!`, downcasting, the `Result` alias); the useful parts of `eyre`/`color-eyre` (sections), `error-stack` (attachments read back by type, several causes), and `snafu`'s `Report` for `main` |
| Classification | A library (toolbox of types and functions). No threads, no timers, no I/O except the report written to standard error by `ExitReport` when `main` returns. |
| MSRV | 1.81, the same as L1 (define-err OD-2), so kraken (1.80) moves by one minor version when it migrates; 1.85 if L1 moves to 1.85 (RD-9, D-005) |
| Edition | 2021 (follows the MSRV; 2024 if the MSRV becomes 1.85) |
| License | `Apache-2.0 OR MIT` |
| Owner | James Gober `<me@jamesgober.com>` |

**Read order** for anyone building from this file:

1. Sections 2 to 5 (what it is, where it sits, scope, requirements), then section 6 (standards).
2. Section 7 (design), especially 7.3, 7.4, and 7.6: the one-word representation, its `unsafe` core, and the chain view are the riskiest part.
3. Section 8 (API by tier and grade) and 8.14 (migrating from anyhow, with kraken as the worked case).
4. Sections 10 to 12 (threats, performance, tests). Every threat and edge case there has a test.
5. Sections 14 to 16 (roadmap, release routine, definition of done).
6. Section 17 (open decisions): the recommendation stands unless James overrides it. Section 19 (ideas) and section 20 (decision log) record what has been decided.

**Item ids.** API items keep the ids of the original design, except the `Error` items, which were `FLT-` (for `Fault`) and are now `ERR-` with the same numbers (D-002).

---

## Contents

1. [Header](#1-header)
2. [Purpose](#2-purpose)
3. [Position in the stack](#3-position-in-the-stack)
4. [Scope](#4-scope)
5. [Requirements](#5-requirements)
6. [Standards](#6-standards)
7. [Design and architecture](#7-design-and-architecture)
8. [API specification](#8-api-specification)
9. [Parity matrix](#9-parity-matrix)
10. [Security](#10-security)
11. [Performance](#11-performance)
12. [Testing plan](#12-testing-plan)
13. [Documentation deliverables](#13-documentation-deliverables)
14. [Version roadmap](#14-version-roadmap)
15. [Release routine](#15-release-routine)
16. [Definition of done](#16-definition-of-done)
17. [Open decisions](#17-open-decisions)
18. [References](#18-references)
19. [Ideas](#19-ideas)
20. [Decision log](#20-decision-log)

---

## 2. Purpose

### 2.1 What it is

report-err gives application code one error type, `Error`, that can hold any error, gather context as it travels up the stack, and print a clear report when it reaches `main`. Code written for anyhow reads the same:

```rust
use report_err::{bail, Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {path}"))?;
    if text.is_empty() {
        bail!("config file {path} is empty");
    }
    let config: Config = parse(&text).context("parsing config")?;
    Ok(config)
}

fn main() -> Result<()> {
    let config = load_config("app.noml")?;
    run(config)
}
```

When `main` returns the error, the process prints:

```text
Error: reading config file app.noml

Caused by:
    No such file or directory (os error 2)

Location: src/main.rs:5:10
```

On top of what anyhow does, an `Error` can carry:

- an error **code** from L1 (`DB-0042`), found automatically in the chain or set with `.coded(...)`, which gives it a severity, a category, and an exit code;
- a **propagation trace**: the file and line of every `.context()` and `.trace()` it passed through, in a fixed buffer (Zig's error return traces);
- **typed attachments** read back by type (`error.attachment::<RequestId>()`), on stable Rust;
- **sections**: notes, warnings, and suggestions shown under the report (color-eyre, Python `add_note`);
- **several causes** (a validation run that failed in twelve places) with tree search, `split`, `flatten`, and `collect_all()`;
- **suppressed errors**: a cleanup failure kept beside the error that caused the cleanup (Java `addSuppressed`);
- an **exit code** chosen from the category when `main` returns through `ExitReport`;
- a **panic boundary** that turns a caught panic into an `Error`.

All of it costs nothing until used: an `Error` that only carries an error and its location is one heap allocation and one machine word on the stack, the same as anyhow.

### 2.2 What it is not

- **Not for library APIs.** Libraries declare typed errors with L1 (and L2's derive) so callers can match on them. `Error` is for applications, binaries, tests, and the top layers of services, exactly where anyhow is used today.
- **Not a renderer.** Colour, JSON, logfmt, and RFC 9457 problem details live in L4. report-err's `Display` and `Debug` follow fixed plain-text rules (R7) and nothing more.
- **Not an error service.** Panic hooks, crash capture, listeners, flood control, event ids, fingerprints, scopes, and breadcrumbs live in L7. report-err only hands records to the L1 reporting facade.
- **Not a retry library.** Retry and backoff live in throttle-net and task-io.
- **Not a way to see every error.** Errors are values; see 7.22 for what can never be observed.

---

## 3. Position in the stack

### 3.1 Who uses it

| User | How |
|---|---|
| kraken (compiler, LSP) | replaces `anyhow` in `compiler/src/main.rs` and `compiler/src/modules/loader.rs` (section 8.14) |
| CLIs: cli-forge, mod-cli, hive-exec, Hive DB tools | `fn main() -> ExitReport`, context on every I/O call, exit codes from categories |
| WASP and Hive DB services | application-layer code above the typed library errors; `Error::report()` at request and task boundaries |
| the error suite | re-exports `Error`, `Result`, the macros, and `ExitReport` in its prelude |
| L7 error service (optional dependency) | builds owned `ErrorRecord`s from an `Error` with `Error::record()`; installs the creation hook (X-9, RD-17) |
| L4 render | does **not** depend on report-err: it walks causes through `std::error::Error` or reads an `ErrorRecord`. `Error::into_std()` and `Error::record()` are the bridges |
| tests in every portfolio crate | `fn test_x() -> Result<()>` with `?` in tests |

### 3.2 What it uses

| Dependency | Why | Kind |
|---|---|---|
| L1 `define-err` | `Code`, `Severity`, `Kinds`, `Category`, `ErrorInfo`, `CodeInfo`, `identify`/`innermost_code`, `ErrorRecord`, the reporting facade (`report_with`, `is_installed`), `Secret<T>` | runtime, published version, `default-features = false` (plus its `alloc` feature) |
| `core`, `alloc` | everything that does not need the OS | always |
| `std` (feature `std`, default) | backtraces, `catch_unwind`, `Termination`, `ExitCode`, standard error output, the thread-local reentrancy guard | default feature |

No third-party runtime dependency. Dev-dependencies: criterion, proptest, trybuild, and the competitor crates for benchmarks and parity tests (anyhow, eyre, color-eyre, error-stack, snafu).

### 3.3 Where it sits in WASP

```
 WASP application code (handlers, jobs, boot)         ─▶ report-err  (Error, context, ExitReport)
 WASP libraries (gateway, gate, web, db adapters)      ─▶ L1 typed errors (no report-err in public APIs)
 WASP error pipeline (L7 + L4 problem details)         ◀─ Error::report() through the L1 facade
 dispatch-io / relay-io (the coms bridge)              ◀─ L7 forwards records; report-err never talks to them directly
```

Every `Error` that is reported goes through the L1 facade, so the error core, and through it the coms bridge, hears about it. report-err never decides that an error stays local (James's rule: security objects are never isolated; see 10.1 T14).

### 3.4 Interface required from L1 (cross-crate request)

report-err names these L1 items. L1's plan (define-err's, written in parallel and read on 2026-10-22 before this section was finalised) already provides most of them; the "Status" column says which, with L1's own section and API ids. Rows marked **request** were asks to L1, settled at L1's 0.2 design lock, before report-err 0.4 (which adds codes). define-err's draft 0.2 API (2026-10-23, waiting for James's approval) accepts all four; the cells below say so and become final when that API is approved. Each request has a fallback, so report-err is never blocked. Where L1 renames an item, report-err follows L1 and this table is updated.

| Id | Item in L1 | What report-err needs | Status (L1 spec, 2026-10-22) | Fallback if L1 declines |
|---|---|---|---|---|
| X-1 | `Code` | `Copy`, `Eq`, `Hash`, `Ord`, `Display` as `DB-0042`, `const` constants; **a non-zero niche so `Option<Code>` is 8 bytes** | provided (CODE-01 to CODE-08); the niche is **accepted** in define-err's draft 0.2 API (`Code` holds a `NonZeroU64`; the first character of a namespace is a letter, so the value is never zero) | the header stores the code as a raw `u64` with 0 meaning none (no size change) |
| X-2 | `Severity` | `Copy`, `Ord`, OpenTelemetry 1-24, `ERROR` = 17, `FATAL` = 21 | provided (7.7) | none needed |
| X-3 | `Category` | `exit_code()`, `is_transient()`, `from_io_kind()` (std), `Internal`, `Unknown`, `Cancelled` | provided (7.8: sysexits table, `Unknown` and `Internal` 70, `Cancelled` 130) | none needed |
| X-4 | `Kinds` | bit set with `ERROR`, `SECURITY`, and `PANIC` (bit 10, "set by L3 and L7") | provided (7.9) | none needed |
| X-5 | `ErrorInfo`, `CodeInfo`, `catalog::find` | the object-safe metadata trait (for `Error::from_info`, ERR-18), the catalog entry type, and lookup of a code in caller-supplied catalogs (for explicit codes, 7.17) | provided (7.10, 7.12, INF-01 to INF-05) | none needed |
| X-6 | `identify`, `innermost_code`, `Identity` | a stable-Rust way to read code, severity, category, kinds, and type name of an L1-generated error seen only as `&dyn Error` | provided (7.11, PRB-01, PRB-02; L1 OD-5 chose the `description()` probe) | none needed |
| X-7 | `ErrorRecord` | an owned snapshot of an error. With the real `source()` chain (D-003), `ErrorRecord::from_error(&**error)` already sees every context layer, so the original request (`ErrorRecord::new` plus `push_chain`) is **reduced to an optional request**: an empty constructor and `push_chain` would let report-err set the code, severity, and category it resolved (an explicit code, 7.17) without the record re-deriving them from the chain | record, limits, `from_error`, and `push_field`/`push_trail`/`set_backtrace` provided (7.15, REC-01 to REC-05); `ErrorRecord::new` and `push_chain` **accepted** in define-err's draft 0.2 API (REC-07, define-err 0.5) | `ErrorRecord::from_error(&**error)`, then overwrite the public fields (`code`, `severity`, `category`, `kinds`; they are `pub`) and add fields and the trail with `push_field` and `push_trail` |
| X-8 | Reporting facade | deliver an error to the reporter. With the real `source()` chain (D-003), `report_with(&**error, severity, kinds)` already gives the reporter every context layer, so it is the main path; the **request** that remains is a verb that delivers an owned record, `report_record(&ErrorRecord)` (the event carries the record), so the reporter also sees sections, attachments shown with `attach_display`, and the trail | `report(&dyn Error)`, `report_with(&dyn Error, Severity, Kinds)`, `is_installed()` provided (7.14, FAC-04, FAC-11, FAC-14); `report_record` **accepted** in define-err's draft 0.2 API (the event carries the record: `Event::record()`), so it is the main path of `report()` (D-012) | `report()` checks `is_installed()` (one load), then calls `report_with(&**error, severity, kinds)` with the error's resolved severity and kinds: no allocation at all; the reporter reads sections and the trail only through `record()` |
| X-9 | Creation hook slot (R25) | a write-once slot for a hook that runs when a **dynamic** error (`Error`) is created: `set_creation_hook(&'static dyn CreationHook)`, `creation_hook()`, `trait CreationHook { fn on_create(&self, info: &CreationInfo<'_>, out: &mut dyn Annotate); }`, `trait Annotate { fn field(&mut self, key: &'static str, value: &dyn Display); fn kind(&mut self, kinds: Kinds); }` | **accepted** in define-err's draft 0.2 API exactly as requested (FAC-21, define-err 0.5; `InstallError { slot: Slot::CreationHook }` on a second install); generated typed errors never call it | report-err hosts the same slot and traits itself (`report_err::set_creation_hook`); L7 then depends on report-err optionally (RD-17) |
| X-10 | `Secret<T>` | `Display` and `Debug` print `[redacted]` | provided (7.16) | none needed |
| X-11 | Build profile | `no_std` + `alloc`, zero dependencies, MSRV not above report-err's | provided: L1 is `no_std` with an `alloc` feature, MSRV 1.81, edition 2021 (L1 OD-2) | none needed |
| X-12 | Stability | X-1 to X-10 frozen from L1 0.2; probe format frozen at L1 1.0 | provided (L1 5.3, 14) | none needed |

---

## 4. Scope

### 4.1 In scope

- `Error`: one type for any error, one machine word, `Send + Sync + 'static`.
- Construction from any `core::error::Error + Send + Sync + 'static` (through `?`), from messages, from boxed errors, from panic payloads.
- Context on `Result` and `Option` (`context`, `with_context`), with the caller's location.
- `err!`, `bail!`, `ensure!`, a `Result<T, E = Error>` alias.
- Downcasting by value, by reference, and by mutable reference, to the root error or to a context value.
- Chain tools (F9): `chain()`, `root_cause()`, `find::<T>()` across the whole tree, the innermost code.
- Codes, severity, category, kinds from L1 (looked up, never invented).
- Formatting rules R7: `{}`, `{:#}`, `{:?}`, `{:#?}`.
- Propagation trace (R11), lazy backtraces (F11), origin location.
- Typed attachments (R12) and sections (R13, data only; plain-text printing in `Debug`).
- Several causes and collections (R14, F10): groups, `Errors`, `collect_all()`, `split`, `flatten`.
- Suppressed errors (R15) and `unraisable()` (handed to the facade).
- `main` integration (R2): `Termination` through `Result`, and `ExitReport` with category exit codes.
- Panic boundary (R24) and panic payload conversion.
- Creation hook (R25) and `report()` through the L1 facade; `record()` building an `ErrorRecord`.
- Budgets on everything that can grow (frames, attachments, sections, children, suppressed, text bytes, chain walks, render output), configured once.
- Guarded rendering: a `Display` that panics or fails never turns a report into a second failure.
- `no_std` + `alloc` for everything except what needs the OS (7.19).

### 4.2 Out of scope, and where each item lives instead

| Item | Why not here | Lives in |
|---|---|---|
| Typed error definitions, codes, severity, categories, catalog, `Secret<T>` | what an error *is*, used by every library | L1 `define-err` |
| `#[derive(Error)]`, `#[error_boundary]`, `#[report_errors]` | proc macros must be their own crate | L2 derive |
| Colour, JSON, logfmt, RFC 9457, OpenTelemetry field names, accessible output, cause de-duplication beyond the simple rule in 7.8 | showing errors | L4 render |
| Source spans, snippets, carets | a different job | L5 diagnostics |
| Panic hook, crash capture, flush deadline | process-wide state with a lifecycle | L7 error service |
| Event id, fingerprint, timestamps, "reported once" across layers beyond the per-`Error` flag, scopes, breadcrumbs, resource attributes, sampling | the error service's job | L7 |
| Delivery to listeners, flood control, cross-process relay | the coms bridge | dispatch-io, relay-io |
| Warnings, notices, `soft(err)?`, `trigger` | reporting verbs | L1 facade, L7 policy |
| Retry, backoff, circuit breakers | behaviour, not errors | throttle-net, task-io |
| `tracing` span traces, `serde` | third-party; collection rule 10.4 | not adopted |
| Nightly `Error::provide` / `request_ref` | unstable | not adopted (attachments are read back by type without it) |
| Custom `?` types (`Try`, `FromResidual`) | unstable | not adopted; `ExitReport` is built with `.into()` |
| Async-specific error traits | errors are plain `Send + Sync` data | not needed |

---

## 5. Requirements

### 5.1 Functional requirements

| Id | Requirement |
|---|---|
| FR-1 | `Error` holds any `core::error::Error + Send + Sync + 'static` and converts from one through `?` with no explicit call. |
| FR-2 | `Error` is exactly one machine word: `size_of::<Error>() == size_of::<usize>()`, `size_of::<Result<(), Error>>() == size_of::<usize>()`, `size_of::<Option<Error>>() == size_of::<usize>()` (decision RD-3 / E-2). |
| FR-3 | `Error: Send + Sync + Unpin + UnwindSafe + RefUnwindSafe + 'static`, checked at compile time. |
| FR-4 | `Error` dereferences (`Deref`, `DerefMut`, `AsRef`) to `dyn Error + Send + Sync + 'static`, whose `source()` chain walks every context layer, then the root error, then the root's own sources, as `anyhow::Error` does (D-003). `Error` itself does not implement `std::error::Error` (it could not and still convert from every error through `?`); `into_std()` and `From<Error> for Box<dyn Error + Send + Sync>` give an owned standard error with the same chain. |
| FR-5 | Context can be added to `Result<T, E>` (any standard error), `Result<T, Error>`, and `Option<T>`, eagerly (`context`) or lazily (`with_context`); the lazy form runs its closure only on the error path. |
| FR-6 | Every creation and every context records the caller's file, line, and column (`#[track_caller]`) at no allocation. |
| FR-7 | `err!`, `bail!`, and `ensure!` accept a string literal, a format string with arguments (inline `{name}` captures included), an error value, any `Display + Debug` value, and an optional leading L1 `Code`. A literal with no arguments allocates no string. |
| FR-8 | `ensure!(condition)` with no message produces `condition failed: <the condition's source text>`. |
| FR-9 | Downcasting finds the root error or any context value by type (`downcast_ref`, `downcast_mut`, `downcast`, `is`), as anyhow does. |
| FR-10 | `chain()` yields the outermost message first, then every context layer, then the root error, then the root's `source()` chain, as `&(dyn Error + 'static)`, bounded by the chain-walk budget. |
| FR-11 | `find::<T>()` searches the whole tree (root, context values, source chains, children) depth first, bounded by the depth and chain budgets. |
| FR-12 | `code()` returns the explicitly set code if any, else the innermost coded error in the chain (through L1's `identify`, X-6), else `None`. `severity()`, `category()`, and `kinds()` come from that error's identity, from configured catalogs for an explicit code, or from overrides (7.17). |
| FR-13 | Formatting: `{}` prints the outermost message only; `{:#}` prints the chain on one line joined by `": "`, skipping a layer whose text the previous layer already ends with; `{:?}` prints the full report (7.8); `{:#?}` prints a structural view for developers. |
| FR-14 | The propagation trace keeps the origin and up to 15 more locations (the first 4 after the origin and the latest 11), with a count of locations dropped in between. |
| FR-15 | Backtraces are captured only at the root creation, only when `RUST_BACKTRACE` or `RUST_LIB_BACKTRACE` enables them (std rules) or when `with_backtrace()` forces one, and symbols are resolved only when printed. |
| FR-16 | Any `Send + Sync + 'static` value can be attached and read back by type (latest first, or all); attachments are never printed unless attached with `attach_display`. |
| FR-17 | Notes, warnings, and suggestions are stored as bounded text and printed in `{:?}` after the cause list, in the order added. |
| FR-18 | An `Error` can hold child errors (several causes); `Errors` gathers failures and `collect_all()` collects every failure of an iterator of results, not only the first. `split` and `flatten` work on the tree. |
| FR-19 | An `Error` can hold suppressed errors that are shown but are not causes; `with_cleanup` runs a cleanup step and keeps its failure as suppressed when the main operation also failed. |
| FR-20 | `fn main() -> Result<()>` prints `Error: ` followed by the `{:?}` report and exits with code 1 (std behaviour). `fn main() -> ExitReport` prints the same report and exits with the category's exit code. |
| FR-21 | `boundary(f)` returns `Err(Error)` for a panic inside `f` when the panic strategy is unwind, with the panic message as the root message; under `panic = "abort"` it calls `f` and the process aborts on panic, as documented. `PANICS_CAUGHT` states which applies, at compile time. |
| FR-22 | When L1's creation hook is installed, it runs once per `Error` creation (not per context), may add fields and kinds, and cannot recurse into itself on the same thread. |
| FR-23 | `report()` hands the error to the L1 facade once; later calls on the same error return `false` and do nothing. |
| FR-24 | `record()` builds an L1 `ErrorRecord` with every field X-7 lists, within L1's size caps. |
| FR-25 | Every budget (7.18) has a safe default, is configurable once per process, and on overflow drops data and counts what was dropped; nothing in report-err fails or panics because a budget was reached. |
| FR-26 | Guarded rendering (`ExitReport`, `report()`, `record()`, `render()`) survives a `Display` that panics (unwind builds), returns `fmt::Error`, or writes without end, and says so in the output. |
| FR-27 | Everything in 7.19's `no_std` column builds and passes its tests with `default-features = false` on a `no_std` target with `alloc`. |
| FR-28 | `Error::from_panic(payload)` turns a `JoinHandle::join` or `catch_unwind` payload into an `Error`, and dropping a panic payload that itself panics never escapes. |
| FR-29 | The `testing` feature provides assertions on an error's chain, code, and sections for use in tests. |

### 5.2 Non-functional requirements

| Id | Requirement | Measured by |
|---|---|---|
| NFR-1 | No panic reachable from any public function, other than a panic raised by caller-supplied code (`Display`, `Drop`, closures) outside guarded rendering. | `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::indexing_slicing` denied; proptest "never panics"; fuzzing |
| NFR-2 | Creating an `Error` from a typed error: one allocation (equal to anyhow). | counting-allocator tests |
| NFR-3 | Each context after the first adds no allocation by report-err itself up to 4 layers, and amortised growth after that (anyhow: one per layer). | counting-allocator tests |
| NFR-4 | The success path through `?` costs the same as anyhow (identical layout); no code runs on `Ok`. | criterion, assembly review of a 5-deep chain |
| NFR-5 | Where report-err does less work than anyhow (context chains, downcasting deep contexts, drop), it is at least 20% faster; where both are bound by one allocation, it is within 5% (11.1). | criterion against anyhow 1.0.104, eyre 0.6.14, error-stack 0.8.0, snafu 0.9.2 |
| NFR-6 | `unsafe` only in `src/raw.rs`; every block has a `SAFETY:` comment; Miri clean on every test that touches it, under Stacked Borrows and Tree Borrows, with strict provenance. | CI Miri job, `clippy::undocumented_unsafe_blocks` |
| NFR-7 | Zero third-party runtime dependencies. | `cargo tree`, `cargo deny` |
| NFR-8 | Memory per `Error` bounded by the budgets in 7.18 regardless of how many times context, attachments, or children are added. | attack tests T1, T2 |
| NFR-9 | The `no_std` + `alloc` build passes on `thumbv7em-none-eabihf` (build) and on the host (tests with `--no-default-features`). | CI |
| NFR-10 | Linux, macOS, Windows first class in CI on stable and MSRV, x86_64 and aarch64. | CI matrix |
| NFR-11 | Deterministic: the same error prints the same text on every platform (paths in locations are the compiler's, see 10.1 T12). | golden tests |
| NFR-12 | The creation hook costs at most 1 ns [est.] when no hook is installed (one acquire load). | criterion |
| NFR-13 | Clean build adds less than 1.5 seconds of compile time and less than 60 KiB to a release binary that uses Tier 1 only [est. targets]. | measured per release |
| NFR-14 | Every public item has a doc comment and a runnable example; Tier 1 is documented first. | `#![deny(missing_docs)]`, review |

### 5.3 Enterprise and distributed

report-err is **stateless** in the sense of the library standard 2.16: an `Error` is a value owned by one call path, and the only process-wide items are the write-once budget configuration and L1's hook and reporter slots (owned by L1). The stateless row applies; how each point of 2.16 is met:

| Point | How report-err meets it |
|---|---|
| Correct under concurrency and heavy load | `Error` is `Send + Sync`; the only shared mutation is the per-error `reported` flag (an atomic, `loom` tested) and the write-once configuration (`loom` tested). Nothing grows without a budget (7.18). Sustained-load test: 64 threads creating, enriching, reporting, and dropping errors for 8 hours with flat memory. |
| Observable | `report()` and the creation hook feed the L1 facade, which costs one atomic load when unused; codes are stable L1 codes; dropped-data counters are part of every record (`omitted` fields). |
| Operable | every limit configurable once with a safe default (7.18); behaviour at a limit is defined (drop and count); capacity numbers published in `docs/PERFORMANCE.md` (bytes per error at each budget). |
| Secure and compliant | rules of 2.4 applied in section 10; attachments never printed unless chosen; `Secret<T>` from L1; terminal-control escaping in `ExitReport`; no secret in `Debug` of the crate's own types. |
| Stable for long-lived systems | semantic versioning; deprecation before removal; MSRV policy; `docs/STABILITY-1.0.md`; the `Debug` report layout is documented and changes only in a minor release with a note (tests that compare output get warning). |
| Distributed | deterministic output; no process-local assumptions in the value itself; `record()` produces the owned, size-capped `ErrorRecord` that L7 and relay-io carry across process and machine boundaries; codes (not type names or addresses) are the identity that crosses a boundary. An `Error` itself is never sent across a process boundary (it holds trait objects and `'static` locations of one binary). |

---

## 6. Standards

Copied from the library standard section 2, adapted only where report-err genuinely differs. Each adaptation is marked **Adapted**. The numbering follows section 2 (6.n is 2.n); 6.16 comes after 6.15 here although it comes before it in the standard.

### 6.1 The bar

- **Best of its kind.** Measured against the best existing libraries in any language: at least **20% faster** than the best Rust competitor at **equal or stronger security**, with the numbers measured and published, never asserted. **Adapted:** the competitors are anyhow, eyre (with color-eyre), error-stack, and snafu's `Whatever`/`Report`. Some operations are bound by one heap allocation in every library (creating an error from a typed error); there the target is "no slower than 5%", and the 20% bar applies to every operation where report-err's design does less work (context chains, deep downcasts, drop, formatting). Section 11.1 names which is which, and every exception is recorded with its reason, as the standard allows.
- **Production-ready from the first line.** No MVPs, no "good enough", no placeholders in shipped code, no deferring a hard task to a later version unless the roadmap records the move and the reason.
- **Push for perfection.** "Flawless" is not attainable; the standard is to make whole classes of failure impossible by design, and to prove the rest by tests.

### 6.2 Performance

- No allocation on hot paths; prove it with a counting allocator in tests. **Adapted:** the hot path is the success path (no code runs on `Ok`) and the propagation of an existing `Error` through `?` (no allocation). Creating an error allocates by nature; the allocation counts per operation are part of the contract (7.5) and each is proven by a counting-allocator test.
- Zero-copy wherever the data allows (borrowed views into input buffers). **Adapted:** string literals passed to `err!`, `bail!`, `ensure!`, `context`, and the section methods are stored as `&'static str`, never copied.
- Bounded work per call; no quadratic behaviour on any input. Chain walks, tree searches, and rendering are bounded by the budgets in 7.18.
- No locks on hot paths; where shared state is unavoidable, read-mostly snapshots, atomics, or sharding, measured. **Adapted:** there are no locks anywhere; the only shared state is write-once.
- Benchmarks (criterion) for every hot path, with baselines; a regression over 5% blocks the change.
- `#[inline]` only where measured; no premature micro-optimisation that costs clarity without a number.

### 6.3 Efficiency

- Zero CPU when idle: no polling loops, no busy waiting, no timers that fire for nothing.
- Memory bounded by configuration, never by input. Every growable part of an `Error` has a budget (7.18).
- Buffers and objects reused (pools) where measured to matter. **Adapted:** no pools; errors are cold and short-lived, and a pool would add shared state. Revisit only if 0.6 measurements show allocator contention under error floods.
- Lowest energy per operation is a stated goal: fewer instructions, fewer wake-ups, fewer system calls. **Adapted:** the extension block is allocated only when first needed, so a plain error is one allocation; backtraces are captured only when enabled and resolved only when printed.

### 6.4 Security

- **Fail closed**: when a check cannot run, the answer is no. **Adapted:** report-err makes no allow/deny decisions. The equivalent rule: when a budget cannot be checked or a renderer fails, data is dropped and counted, never printed unbounded.
- Every budget (bytes, counts, depth, time) is checked **before** allocation or work.
- Every parser and decoder has a fuzz target; every threat in the plan has an attack test that must fail to get through. **Adapted:** report-err has no parser. Its fuzz targets drive arbitrary sequences of operations (create, context, attach, group, suppress, split, flatten, render, downcast, drop) and adversarial `Display` implementations (12.6).
- Constant-time comparison for anything secret; secrets wiped after use; no secret in `Debug`, `Display`, errors, or logs. **Adapted:** report-err holds no secret of its own. It cannot know whether a caller's context string contains one; the defences are L1's `Secret<T>`, opaque attachments, and documentation (10.1 T3).
- Checked or saturating arithmetic on every value derived from untrusted input. **Adapted:** all counters (omitted frames, bytes, children) saturate.
- "A security component that cannot name the attack it stops, and prove it with a test, does not ship."
- Every security measure in the documentation states its cost (latency, memory), estimates labelled as estimates.
- At least one new defence idea per version, recorded as hypothesis, method, and expected cost in the plan's ideas section.

### 6.5 Stability and durability

- No panics in library code: `Result` everywhere; no `unwrap`, `expect`, `todo!`, `unimplemented!`, `unreachable!` outside tests. **Adapted:** most report-err operations cannot fail and return plain values; the few that can (`configure`) return `Result`. Allocation failure aborts the process as in the rest of `alloc` (10.1 T9).
- Every error variant documented, tested, and actionable ("say what to do"); error enums `#[non_exhaustive]`. The crate's own error type is `ConfigureError` (8.12).
- Deterministic behaviour: the same input gives the same output on every platform.
- Graceful degradation under overload: refuse cleanly, never fall over. **Adapted:** under an error flood each error stays bounded; flood control itself is L7's job.
- Where state is persisted: crash-safe writes, checksums, tested by killing the process mid-write. **Adapted: not applicable.** report-err persists nothing.

### 6.6 Concurrency and heavy load

- Types are `Send` and `Sync` where that makes sense, checked at compile time.
- No global mutable state. **Adapted:** two write-once globals exist: the budget configuration (`configure`, set once at start, read with one acquire load) and, under `std`, a thread-local reentrancy flag for the creation hook (7.16). L1 owns the hook and reporter slots.
- Contended paths are lock-free or sharded; concurrency primitives are tested with `loom`. **Adapted:** the per-error `reported` flag and the write-once configuration are tested with `loom`.
- Bounded queues everywhere; back-pressure is explicit; nothing grows without a limit. **Adapted:** report-err has no queues; every collection inside an error is bounded.
- Sustained-load and soak tests (hours) before 1.0; no memory growth over the soak.

### 6.7 Cross-platform

- Linux, macOS, and Windows are first class, on stable and the MSRV, in CI. FreeBSD where the crate touches OS event mechanisms. **Adapted:** report-err touches no OS event mechanism; FreeBSD is not in CI.
- Every `#[cfg(unix)]` path has a Windows counterpart or a documented, tested reason why not. **Adapted:** report-err has no `cfg(unix)` path. Its only platform-dependent behaviour is std's (backtrace support, the panic strategy), documented in `docs/PLATFORM-NOTES.md`.
- `no_std` (with or without `alloc`) for cores that do not need the OS, checked in CI. **Adapted:** `no_std` **with** `alloc` (an `Error` lives on the heap); section 7.19 lists exactly what is available.

### 6.8 Dependencies and runtime

- **Zero third-party runtime dependencies.** Exceptions, each recorded with its reason: audited cryptography reached through `crypt-io`; `rustls` inside `tls-net` (TLS is never written in-house). **Adapted:** neither exception applies; report-err has no third-party runtime dependency at all.
- First-party crates are used by **published version**, never `path = "../..."` (each crate is its own repository and CI checks it out alone). **Adapted:** the L1 dependency is added only once L1 is published (14: 0.3 waits for an L1 release).
- **No async runtime** (no tokio, no async-std) in any crate; cores are sans-IO and work with any driver. The REPS clause "MUST use tokio" is a known mistake and is recorded as an approved exception in each crate's plan. **Adapted:** report-err works unchanged under any runtime, including kraken's `#[tokio::main] async fn main() -> Result<()>` (8.14).
- Optional adapters for other ecosystems only behind non-default features, and never required.
- Dev-dependencies (criterion, proptest) are allowed. **Adapted (added):** trybuild for compile-fail tests; anyhow, eyre, color-eyre, error-stack, and snafu as dev-dependencies of the benchmarks and parity tests only.

### 6.9 Unsafe code

- `#![forbid(unsafe_code)]` unless the crate must call the operating system where `std` has no API. **Adapted: this crate uses `unsafe` for a different reason**, recorded as decision RD-3 (ERROR-SUITE E-2): the one-word type-erased representation (7.3, argued with numbers in 7.4). If James chooses two words, the crate root becomes `#![forbid(unsafe_code)]` and this adaptation is removed; the public API does not change.
- Such crates confine `unsafe` to one `sys` module per platform, every block with a `// SAFETY:` comment, every `unsafe fn` with a `# Safety` section, `deny(unsafe_op_in_unsafe_fn)`, and tests (Miri where it can run). **Adapted:** the one module is `src/raw.rs` (not per platform: the code is platform-independent). Miri runs on all of it.
- Crate root: `#![deny(unsafe_code)]` (not `forbid`, which cannot be lifted locally), with `#[allow(unsafe_code)]` only on `mod raw`. Also denied: `clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`, `clippy::ptr_as_ptr`, `clippy::cast_ptr_alignment`, `clippy::borrow_as_ptr`.
- `docs/UNSAFE.md` lists every invariant the module relies on and the test that checks each (7.3).

### 6.10 API design

- **MUST: a simplified API, easy to use and easy to read.** This is a hard requirement from James (stated three times, 2026-10-22), not a preference. A design whose common uses need more than a line or two of obvious code, or whose names a newcomer cannot read without the docs, fails review. Simplicity is checked at the 0.2 design lock and again at the 1.0 audit. **Adapted (how report-err meets it):** the whole of Tier 1 is one import (`use report_err::prelude::*;` or the anyhow-shaped `use report_err::{Context, Result};`), one type, three macros, and two methods; section 8.2 shows every common use as one or two lines.
- **Tier 1 is mandatory**: free functions or one-line constructors that cover about 80% of uses with no builder, no configuration object, and no type parameters the caller must name. Documented first, in the README and in rustdoc.
- **Tier 2**: builders and configuration for the next 15%.
- **Tier 3**: the full, generic, allocation-free surface for the last 5% and for other libraries.
- Extensible enums and structs are `#[non_exhaustive]` from the first release; `#[must_use]` where dropping a value is a bug; `pub(crate)` by default. **Adapted:** `Error` and every method returning an `Error` or a `Result` are `#[must_use]`.
- Names are plain words; no abbreviations a reader must decode.
- Every public item has a doc comment with at least one runnable example.

### 6.11 Testing

- Unit tests for every function and every `Ok` and `Err` path, at the bottom of each file; integration tests in `tests/`.
- Test names: `test_<subject>_<condition>_<expected>`.
- Edge cases for every input: empty, one, maximum, maximum plus one, zero, negative where possible, boundaries (0, 1, n-1, n, n+1), invalid encodings, truncated input, input split at every byte boundary (for streaming parsers), concurrent use, failure in the middle of an operation, corrupted data. **Adapted:** the inputs are error values, `Display` implementations, budgets, and operation sequences (12.4 lists 100 cases).
- Property tests (proptest) for every invariant (round trips, "never panics", "never exceeds budget").
- Fuzz targets (cargo-fuzz) for every parser and decoder, with an out-of-repo corpus. **Adapted:** operation-sequence and adversarial-`Display` targets (12.6).
- Conformance suites and RFC test vectors where they exist. **Adapted:** no RFC applies; the conformance suite is the anyhow parity suite (12.7): the same programs written against anyhow and report-err give the same `{}`, `{:#}`, and downcast results.
- Counting-allocator tests for every "no allocation" claim.
- `loom` for every concurrency primitive.
- CI: Linux, macOS, Windows on stable and MSRV; `no_std` build where applicable.

### 6.12 Documentation

- README from the README template: header image, badges including MSRV, pitch, features, install, quick start (Tier 1 first), status, documentation links, license, copyright footer. Never remove the header image, the MSRV badge, or the footer.
- `docs/API.md` in the format of `https://github.com/jamesgober/metrics-lib/blob/main/docs/API.md`: nested table of contents, every public item, description, parameters, errors, several examples per use case.
- By 1.0: `docs/ARCHITECTURE.md`, `docs/PERFORMANCE.md` (measured numbers, method, hardware), `docs/SECURITY.md` (threat model), `docs/PLATFORM-NOTES.md`, `docs/STABILITY-1.0.md`. **Adapted (added):** `docs/UNSAFE.md` (the audit of `src/raw.rs`) and `docs/MIGRATING.md` (from anyhow, eyre, and snafu's `Whatever`).
- `CHANGELOG.md` in Keep a Changelog 1.1.0 format, updated in the same change as the code.
- Release notes per version in `docs/release/vX.Y.Z.md`.
- Writing: plain, specific, human prose; past tense in release notes; every claim matches the build; none of the four marketing words banned in the library standard 2.12; no em dash (U+2014); no emoji; never a reference to `.dev/` in public files.

### 6.13 Repository and tooling

- The portfolio's repository layout: `.claude/CLAUDE.md` and `.dev/` (both gitignored), `.editorconfig`, `.gitattributes` (`* text=auto eol=lf`), `.github/workflows/ci.yml`, `.gitignore` (also `Cargo.lock`), `benches/`, `examples/`, `tests/`, `Cargo.toml`, `CHANGELOG.md`, `clippy.toml`, `deny.toml`, `rustfmt.toml`, `dev/DIRECTIVES.md`, `dev/PLAN.md`, `dev/ROADMAP.md` (all committed), `docs/`, `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`, `REPS.md` (byte-identical copy), `src/`. **Adapted (added):** `fuzz/` (cargo-fuzz targets; corpus out of repo), `tests/ui/` (trybuild compile-fail cases), `tests/abort/` (a small binary crate built with `panic = "abort"` and run as a subprocess, 12.8).
- `Cargo.toml` in the portfolio layout: edition 2024, MSRV 1.85 unless the plan justifies more, `Apache-2.0 OR MIT`, author `James Gober <me@jamesgober.com>`, release profile `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`. **Adapted:** MSRV **1.81** and edition **2021**, the same as L1 (define-err's plan OD-2): L1 caps every error crate's MSRV, and kraken (1.80), the first migration target, then moves by one minor version instead of five (RD-9). **Adapted (note):** the release profile of the crate itself does not affect users; but the portfolio's applications use `panic = "abort"`, so `boundary` catches nothing in their release builds. The README says so beside the `boundary` example (7.15).
- Crate root: `#![forbid(unsafe_code)]` (unless 2.9 applies), `#![deny(missing_docs)]`, `#![cfg_attr(docsrs, feature(doc_cfg))]`, `pub const VERSION`. **Adapted:** `#![deny(unsafe_code)]` with `#[allow(unsafe_code)]` on `mod raw` only (6.9); `#![no_std]` with `extern crate alloc;` and `#[cfg(feature = "std")] extern crate std;`; `#![deny(unsafe_op_in_unsafe_fn)]`.
- CI as in the portfolio template: matrix Linux/macOS/Windows × stable/MSRV; fmt; clippy with default, all, and no default features under `-D warnings`; tests in the same three modes; examples build; `cargo doc` under `RUSTDOCFLAGS=-D warnings`; a security job with `cargo audit` and `cargo deny check`. **Adapted (added):** aarch64 runners; a Miri job (`cargo +nightly miri test`, with `-Zmiri-strict-provenance`, once with Stacked Borrows and once with `-Zmiri-tree-borrows`); a `loom` job; a `no_std` job (`cargo build --no-default-features --target thumbv7em-none-eabihf`); the `panic = "abort"` subprocess test; an AddressSanitizer job on Linux (nightly).
- Local gate before every push, on Windows and in WSL Ubuntu, stable and MSRV: fmt, clippy (all three feature modes), tests, `cargo doc` with default **and** all features, `cargo package` in a fresh target directory. Never pipe check commands through `tail`. **Adapted (added):** Miri on the `raw` tests locally before any change to `src/raw.rs` is pushed.

### 6.14 Versions

| Version | Milestone |
|---|---|
| 0.1 | scaffold: repository, plan, CI green |
| 0.2 | design lock: API written in `docs/API.md` and approved |
| 0.3 | core |
| 0.4 | extended |
| 0.5 | feature complete |
| 0.6 | optimisation: benchmarks against competitors |
| 0.7 | hardening: fuzzing, attack tests, soak |
| 0.8 | alpha |
| 0.9 | beta and release candidates |
| 1.0 | stable: API frozen until 2.0, full audit passed |

No series is skipped. An MSRV raise is a major change after 1.0. **Adapted:** 0.3 "core" is the anyhow-parity surface, so kraken can migrate at 0.3 (section 14).

### 6.15 Git and authorship

- Commits as `James Gober <me@jamesgober.com>` (`git -c user.name=... -c user.email=...`). No co-author lines; never "Claude", "AI", "generated", or "Anthropic" anywhere.
- Stage explicit paths, never `git add -A`. Check `git log origin/main..HEAD --format='%an <%ae> | %B'` before every push.
- Never `--no-verify`, never force-push, never delete branches or tags without asking.
- Every design decision goes into `dev/PLAN.md`'s decision log in the same change.

### 6.16 Enterprise-ready and distributed by default

James's rule (2026-10-22): **every library, module, service, package, suite, component, and project is planned, designed, built, and engineered for enterprise use and for distributed deployment, unless James states otherwise.** Never plan a basic, local-only, or "standard edition" version.

What that means for every crate:

- **Correct under concurrency and heavy load**: thread-safe where shared, bounded everywhere, no hidden global state, sustained-load and soak tests.
- **Observable**: hooks for metrics, events, and errors that cost nothing when unused; stable error codes; health where the crate has state or runs.
- **Operable**: every limit configurable with a safe default; behaviour under overload defined (refuse cleanly); documented limits and capacity numbers.
- **Secure and compliant**: the security rules of 2.4, audit hooks where actions matter, secrets never exposed.
- **Stable for long-lived systems**: semantic versioning honoured, deprecation before removal, an MSRV policy, a written support story in `docs/STABILITY-1.0.md`.

What "distributed" means depends on whether the crate holds state:

| Crate | Distributed-ready means |
|---|---|
| **Stateless** (parsers, codecs, protocol engines: parse-net, http-net) | deterministic, no process-local assumptions, safe to run the same version on many nodes; nothing more is needed |
| **Stateful** (sessions, bans, rate limits, caches, stores, queues, registries, the kernel) | state lives behind a trait so it can be in memory, durable, shared, or replicated; no design that only works in one process; clock skew tolerated; operations idempotent where retries happen; behaviour during a network partition defined (usually: each node keeps protecting itself locally); multi-node tests or simulation before 1.0 |

**Adapted:** report-err is in the stateless row; section 5.3 states how each point is met. Health does not apply (no running state).

---

## 7. Design and architecture

### 7.1 Module tree

```
src/
  lib.rs          crate docs (Tier 1 first), lint attributes, VERSION, re-exports, prelude
  raw.rs          THE unsafe core: Node<E>, Header, VTable, RawError (the only module with unsafe)
  error.rs        Error: constructors, accessors, Drop, Send/Sync assertions (safe, over raw)
  frame.rs        context layers: Frame, FrameValue, moving C into a frame without boxing
  ext.rs          the lazily allocated extension block: frames, trace points, attachments,
                  sections, children, suppressed, hook fields, kinds, backtrace, budgets used
  context.rs      the sealed Context trait for Result<T, E>, Result<T, Error>, Option<T>
  macros.rs       err!, bail!, ensure!, and their private dispatch helpers
  chain.rs        Chain iterator, root_cause, find, find_all, code lookup through L1
  format.rs       Display and Debug rules (7.8), the de-duplication rule, guarded rendering,
                  the bounded writer, terminal-control escaping
  group.rs        children, Errors, CollectAll, split, flatten
  trace.rs        trace points and the Trail iterator
  limits.rs       Limits, configure(), ConfigureError, budget accounting helpers
  hook.rs         creation hook (X-9), report(), unraisable(), record() (X-7, X-8)
  exit.rs         ExitReport, Termination, exit-code selection            (feature std)
  boundary.rs     boundary(), Error::from_panic, PANICS_CAUGHT              (from_panic: always;
                                                                             boundary: feature std)
  backtrace.rs    lazy backtrace capture and access                        (feature std)
  std_interop.rs  into_std(), From<Error> for Box<dyn Error>, from_boxed
  testing.rs      assertions for tests                                     (feature testing)
```

Every module except `raw` is safe Rust. `raw` exposes a small `pub(crate)` surface (7.3) and nothing else in the crate touches pointers.

### 7.2 Memory layout

An `Error` is a non-null pointer to one heap allocation that holds a fixed header followed by the root error itself:

```
 stack              heap: one allocation, Layout::new::<Node<E>>()
 ┌─────────┐        ┌────────────────────────────────────────────────────────────────┐
 │ Error   │──────▶ │ Header (#[repr(C)], first field, 80 bytes on 64-bit [est.])    │
 │ 8 bytes │        │   vtable    &'static VTable          8   one per payload type  │
 └─────────┘        │   origin    &'static Location        8   #[track_caller]       │
                    │   code      Option<Code>             8   needs L1 X-1 niche    │
                    │   flags     AtomicU32                4   reported, panicked... │
                    │   severity  u8 (0 = from code)       1                         │
                    │   exit      u8 (0 = from category)   1                         │
                    │   spare     u16                      2                         │
                    │   ext       Option<Box<Ext>>         8   allocated on demand   │
                    │   first     Option<Frame>           40   innermost context     │
                    ├────────────────────────────────────────────────────────────────┤
                    │ payload: E (the root error, inline, at its own alignment)      │
                    └────────────────────────────────────────────────────────────────┘

 Ext (second allocation, only when needed; about 400 bytes [est.]):
   frames      context layers 2..n, 4 inline then a Vec       (budget max_frames)
   trace       15 trace points: first 4 + latest 11, dropped count
   attachments Vec<Attachment>                                (budget max_attachments)
   sections    Vec<Section>                                   (budget max_sections)
   children    Vec<Error>                                     (budget max_tree)
   suppressed  Vec<Error>                                     (budget max_suppressed)
   fields      Vec<(&'static str, Box<str>)> from the creation hook
   kinds       Kinds
   backtrace   Option<std::backtrace::Backtrace>              (feature std)
   used        bytes and tree size charged against the budgets
   omitted     saturating counters of everything dropped at a budget
```

Why the first context layer lives in the header: anyhow stores `.context()` on a typed error as a single node holding both the context and the error, so the most common line in application code (`fs::read(p).context("...")?`) costs anyhow one allocation. A design that always put context in a second block would cost two and lose to anyhow on its most common operation. With `first` inline, the first context on any error is free, whether it is added on the typed error or later on the `Error`.

Sizes [measured, rustc 1.95.0, x86_64-pc-windows-msvc]: `size_of::<Option<std::backtrace::Backtrace>>()` is 48 bytes, so anyhow's header (vtable plus `Option<Backtrace>`) is 56 bytes before the error; report-err's header was 72 bytes [measured] and is 80 bytes with the node pointer each frame carries for the `source()` chain (7.6.1) [est.]; it carries the first context, the code, the origin location, and the flags without a further allocation, and moves the backtrace into `Ext` where it costs nothing when backtraces are off.

### 7.3 The unsafe core (`src/raw.rs`)

The technique is anyhow's (and eyre's): a `#[repr(C)]` struct whose first field is a header holding a hand-built vtable, erased behind one thin pointer. It is confined to one module of about 250 lines [est.].

```rust
// Sketch of the private surface; not public API.
#[repr(C)]
pub(crate) struct Node<E> {
    header: Header,      // must stay the first field
    payload: E,
}

pub(crate) struct VTable {
    pub(crate) drop: unsafe fn(NonNull<Header>),
    pub(crate) payload_error: unsafe fn(NonNull<Header>) -> NonNull<dyn Error + Send + Sync + 'static>,
    pub(crate) payload_type: fn() -> TypeId,        // the user-visible type (E, or M for messages)
    pub(crate) info: Option<unsafe fn(NonNull<Header>) -> NonNull<dyn ErrorInfo>>, // only for from_info (ERR-18)
    pub(crate) payload_ptr: unsafe fn(NonNull<Header>, TypeId) -> Option<NonNull<()>>,
    pub(crate) take: unsafe fn(NonNull<Header>, TypeId, out: NonNull<()>) -> bool,
    pub(crate) type_name: fn() -> &'static str,
}

pub(crate) struct RawError(NonNull<Header>);

impl RawError {
    pub(crate) fn new<E: Error + Send + Sync + 'static>(payload: E, header: Header) -> RawError;
    pub(crate) fn header(&self) -> &Header;
    pub(crate) fn header_mut(&mut self) -> &mut Header;
    pub(crate) fn payload_error(&self) -> &(dyn Error + Send + Sync + 'static);
    pub(crate) fn payload_ref<T: 'static>(&self) -> Option<&T>;
    pub(crate) fn payload_mut<T: 'static>(&mut self) -> Option<&mut T>;
    pub(crate) fn take<T: 'static>(self) -> Result<T, RawError>;
}
impl Drop for RawError { /* calls (vtable.drop)(self.0) */ }
```

Payload wrappers are `#[repr(transparent)]` so a pointer to the wrapper is a pointer to the user's value: `Message<M>` (for `err!("...")` and `Error::msg`, implementing `Error` with `Display` and `Debug` of `M`), `Boxed` (for `from_boxed`), `Panic` (for panic payloads), and `Group` (the root of a several-causes error). `payload_type` reports the user-visible type (`M`, not `Message<M>`), so `downcast_ref::<&str>()` works on `err!("literal")` as it does on anyhow.

**Invariants**, each with the test that checks it (all run under Miri):

| Id | Invariant | Test |
|---|---|---|
| I-1 | `Header` is the first field of `#[repr(C)] Node<E>`, so the node pointer and the header pointer are the same address. A `const` assertion checks `offset_of!(Node<E>, header) == 0` for every instantiated payload wrapper. | compile-time assertion; `test_raw_header_offset_is_zero` |
| I-2 | A header's vtable was built for exactly the `E` of the node it sits in; it is written once in `RawError::new::<E>` and never changed. | code review; vtables are `&'static` constants per `E` produced only inside `new` |
| I-3 | The stored pointer is derived from `Box::into_raw` of the whole `Node<E>`, never from a reference to the header field, so it carries provenance for the whole allocation. `&Header` is created only to read header fields; payload access goes through the raw pointer cast back to `Node<E>`. | Miri with `-Zmiri-strict-provenance` and both Stacked Borrows and Tree Borrows |
| I-4 | The allocation is freed exactly once, with `Layout::new::<Node<E>>()`, through `Box::from_raw` in the vtable's `drop` for the same `E`. | Miri leak and double-free detection; `test_raw_drop_runs_payload_drop_once_and_take_moves_payload` |
| I-5 | `take` checks the `TypeId`, moves the payload out with `ptr::read`, drops the remaining header fields (the `Ext` box and the inline frame), and frees the allocation without dropping the payload again. | `test_raw_drop_runs_payload_drop_once_and_take_moves_payload`, drop counters |
| I-6 | Every cast from `NonNull<()>` to `&T` or `&mut T` is preceded by a `TypeId` comparison inside the same vtable function. | `test_raw_payload_ref_wrong_type_is_none` for every wrapper |
| I-7 | `Error: Send + Sync` holds because the only constructor requires `E: Send + Sync + 'static` and every header field is `Send + Sync`. The `unsafe impl Send` and `unsafe impl Sync` carry that argument as their `SAFETY:` comment. | compile-fail doc tests in the crate root: a `!Send`, `!Sync`, or non-`'static` error does not convert (D-021) |
| I-8 | `&mut Header` is produced only from `&mut RawError`, so it never coexists with a shared reference. | borrow checker; Miri |
| I-9 | Zero-sized payloads still produce a non-zero-sized node (the header), so the allocation is never zero-sized. | `test_raw_zero_sized_payload` |
| I-10 | Over-aligned payloads (align 64 and 4096) are placed correctly; the node's alignment is `max(align_of::<Header>(), align_of::<E>())`. | `test_raw_over_aligned_payload_64`, `_4096` |
| I-11 | A payload whose `Drop` panics: the panic propagates out of `drop(error)` like any value's would, and Miri reports no use-after-free; the node is still freed: Miri reports neither a leak nor a use after free (0.3.0 result, `docs/UNSAFE.md`). | `test_raw_payload_drop_panics` (unwind builds) |
| I-12 | Every frame's link points to the next layer inward, or, with bit 0 set, to its own node; it is derived by raw projection from the pointer that layer's allocation was made with (the node pointer, the extension block's pointer, or the spill vector's buffer pointer), never from a reference. After any move of frames (a spill reallocation, a dropped middle layer), every link from that layer outward is rewritten before `&mut` access ends. A frame never outlives the node that holds it, so `source()` reads through a pointer that is valid for as long as the `&Frame` it is called on, and `Frame` is `Send + Sync` for the same reason the `Error` is (I-7). | `test_raw_frame_source_walks_every_layer` (0 to 20 layers), `test_raw_spill_growth_keeps_links` (100 layers), `test_raw_max_frames_drops_oldest_middle_layer`, `test_raw_remove_inline_layer_pulls_spill_frame_in`, under Miri with both aliasing models |
| I-13 | Code reachable from `deref`, `deref_mut`, and a frame's `source()` never creates a reference covering a whole `Header`, `Node<E>`, or `Ext`; it projects to the field it reads (`addr_of!`) and creates a reference only to that field or to the one frame it returns. So a `&mut` handed out by `DerefMut` (which covers one frame) is never invalidated by a `source()` walk that reads the other frames, under Stacked Borrows or Tree Borrows. | `test_raw_deref_mut_then_source_then_reuse`, `test_deref_mut_then_read_again`, under Miri with both aliasing models |

Lints on the module: `#![deny(unsafe_op_in_unsafe_fn)]`, `clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`, `clippy::ptr_as_ptr`, `clippy::cast_ptr_alignment`, `clippy::borrow_as_ptr`. Every `unsafe fn` has a `# Safety` section; every block has a `// SAFETY:` comment naming the invariant it relies on (I-1 to I-13).

### 7.4 Decision E-2 / RD-3: one word with an audited `unsafe` core

Three representations were compared on 2026-10-22 with a probe program compiled with `rustc 1.95.0 -C opt-level=3 -C codegen-units=1` on Windows 11 (AMD Ryzen 9 9950X3D, 128 GiB). Timings are the best of 7 runs after warm-up; the first, cold round of each run varied by up to 2x and is discarded.

| | **A: one word, audited `unsafe`** (anyhow's technique) | **B: two words, `forbid(unsafe_code)`** (`Box<dyn ErasedNode>`) | **C: one word, safe** (`Box<Header>` holding `Box<dyn Error>`) |
|---|---|---|---|
| `size_of::<Error>()` | 8 | 16 | 8 |
| `size_of::<Result<(), Error>>()` | **8** | 16 | 8 |
| `size_of::<Option<Error>>()` | **8** | 16 | 8 |
| `size_of::<Result<u32, Error>>()` | 16 | 16 | 16 |
| `size_of::<Result<u64, Error>>()` | 16 | 16 (niche in the data pointer) | 16 |
| `size_of::<Result<[u64; 2], Error>>()` | 24 | 24 | 24 |
| `size_of::<Vec<Error>>` element (children, suppressed, `Errors`) | 8 | 16 | 8 |
| allocations to create from a typed error | 1 | 1 | **2** |
| extra cost per creation [measured: one allocation and free on the Windows system allocator, 21-23 ns warm; 46-51 ns for a pair] | 0 | 0 | about 21-23 ns |
| pointer hops to the root error | 1 | 1 | 2 |
| success path, 5-deep `?` chain, `Result<(), _>` [measured] | 3.99 ns | 3.87 ns | as A |
| success path, 5-deep `?` chain, `Result<u64, _>` [measured] | 3.79 / 3.80 ns | 4.14 / 4.09 ns | as A |
| success path, 5-deep `?` chain, `Result<[u64; 2], _>` [measured] | 4.11 / 4.12 ns | 3.92 / 4.02 ns | as A |
| `unsafe` lines | about 250 [est.], one module | 0 | 0 |

What the numbers say:

1. **The success path is not the reason.** On x86_64 the Rust calling convention returns a two-word `Result` in registers, and the measured differences (within 9% in both directions) are noise. the error suite plan section 6 said one word "keeps the success path fast"; the measurement does not support that on this target, and this plan does not repeat the claim. It should be re-measured on aarch64 at 0.6.
2. **The reasons are memory and parity.** `Result<()>` is the most common return type in application code (in kraken's two anyhow files, `compiler/src/main.rs` and `compiler/src/modules/loader.rs`, 63 of the 75 functions returning `Result` return `Result<()>`, counted 2026-10-22), and every one of them is 8 bytes with anyhow. A drop-in replacement that doubles that is a regression users can measure: in structs that keep a result, in `Vec<Result<(), Error>>` from parallel jobs, in channel messages, in `Option<Error>` fields, and in report-err's own children, suppressed, and `Errors` lists, which halve in size under A.
3. **C keeps one word safely but pays a second allocation on every error**, about 21-23 ns [measured], which doubles creation cost and fails NFR-2 and the 5% creation target against anyhow (11.1). C is rejected.
4. **The risk of A is bounded and known**: about 250 lines, the same technique anyhow (1.08 billion downloads on crates.io, 2026-10-22) and eyre have shipped for years, the invariants in 7.3 each tested, Miri in CI under both aliasing models.

**Recommendation: A.** The representation is private, so if the Miri job or the 1.0 audit finds a problem that cannot be fixed, the crate moves to B before 1.0 with no API change (only `size_of` results change, and FR-2 with them). If James prefers `#![forbid(unsafe_code)]` over the 8-byte size, B is the choice and the rest of this plan is unchanged.

### 7.5 Creation paths and allocation counts

Every count below is a contract proven by a counting-allocator test (12.5). Counts for anyhow are from reading its source (`src/error.rs`, 7.3 references) and are re-measured in the parity suite at 0.6.

| Operation | anyhow 1.0.104 | report-err |
|---|---|---|
| `?` on `Err(io::Error)` into `Result<_, Error>` | 1 | 1 |
| `err!("literal")`, `bail!("literal")` (no arguments) | 1 | 1 (`format_args!(...).as_str()` gives a `&'static str`; no `String`) |
| `err!("value {x}")` | 2 (`String` + node) | 2 at 0.3; 1 if RD-14 (one-allocation formatting) is adopted at 0.6 |
| `io_result.context("literal")?` | 1 (one node holds context and error) | 1 (context in the header slot) |
| `?` first, then `.context("literal")` on the `Error` | 2 | 1 |
| second `.context("literal")` | +1 | +1 (the `Ext` block) |
| third to fifth `.context("literal")` | +1 each | 0 (inline frames in `Ext`) |
| `.context(format!(...))` after `Ext` exists | +1 node (+ the caller's `String`) | 0 (+ the caller's `String`, moved in) |
| `.context(MyValue)` (a non-string type) | +1 | +1 (boxed value) |
| `.with_context(...)` on `Ok` | 0 | 0 (closure not called) |
| `?` on `Err(Error)` into `Result<_, Error>` | 0 | 0 |
| `.trace()` | not available | +1 if `Ext` does not exist yet, else 0 |
| backtrace enabled | std's capture allocations at every root | same, at the root only |
| drop of an error with three literal contexts | 4 frees | 2 frees |

### 7.6 Context layers (frames)

Context is stored **flat**: the innermost layer in the header slot `first`, the rest in `Ext.frames`, innermost first. Each frame holds its location and one of three values:

```rust
// Sketch.
pub(crate) struct Frame {
    location: &'static Location<'static>,
    value: FrameValue,
}
pub(crate) enum FrameValue {
    Static(&'static str),               // .context("literal"): no allocation, no copy
    Text(String),                       // .context(format!(...)) or a String: moved in, not copied
    Value(Box<dyn ContextValue>),       // any other C: Display + Send + Sync + 'static
}
```

A context value `C` is moved into the right variant without `unsafe` and without boxing strings: the value is put in an `Option<C>`, viewed as `&mut dyn Any`, and taken out with `downcast_mut::<Option<&'static str>>()` or `downcast_mut::<Option<String>>()`; after monomorphisation this is a constant branch.

Downcasting to a context type goes through `FrameValue` (`Static` answers `&'static str`, `Text` answers `String`, `Value` answers its own type).

#### 7.6.1 Every layer is a link of a real `source()` chain (D-003, D-011)

James's rule: an `Error` must work wherever a `&dyn Error` is expected, with a `source()` chain that walks every context layer and then the root cause, exactly as `anyhow::Error` does, without giving up the flat store (and so the allocation counts of 7.5). anyhow gets its chain from nesting: each context is a node that owns the error below it. Here a frame does not own the layer below it, so each frame keeps a **link** to it:

```rust
// As built in 0.3 (src/raw.rs).
pub(crate) struct Frame {
    location: &'static Location<'static>,
    value: FrameValue,
    link: NonNull<Frame>,      // the next layer inward, or (bit 0 set) the node, whose payload is next
}
impl core::error::Error for Frame {
    fn source(&self) -> Option<&(dyn Error + 'static)> { /* one load and one branch */ }
}
impl Deref for Error {
    type Target = dyn core::error::Error + Send + Sync + 'static;
    fn deref(&self) -> &Self::Target { /* the outermost frame, or the root when there is no context */ }
}
```

- **`Deref` and `AsRef`.** `Error` implements `Deref<Target = dyn Error + Send + Sync + 'static>`, `DerefMut`, `AsRef<dyn Error + Send + Sync + 'static>`, and `AsRef<dyn Error + 'static>`, as anyhow does. The target is the outermost frame, or the root error itself when there is no context, so `&*error`, `error.source()`, and passing `&*error` or `error.as_ref()` where a `&dyn Error` is expected all behave as with anyhow. `Error` still does not implement `core::error::Error` itself (FR-4: it could not, and still convert from every error through `?`).
- **The link.** Each frame stores a pointer to the next layer inward. The innermost layer's link is the node pointer with bit 0 set (headers are aligned to 8, so the bit is free; `wrapping_byte_add` keeps the pointer's provenance); its `source()` clears the bit and returns the root through the vtable. `source()` is one load and one branch.
- **Where links come from.** Every link is derived from the pointer its allocation was made with, by raw projection only: links into the header from the node pointer (I-3), links into the four inline slots from the extension block's own pointer (made with `Box::leak` and kept as a raw pointer, never re-boxed), and links into the spill vector from the vector's buffer pointer (`as_ptr`, which makes no reference to the elements). Code that reads or writes frames projects to the one field or frame it needs (I-13), so a `&mut` from `DerefMut` and any number of shared walks stay sound under Stacked Borrows and Tree Borrows.
- **When frames move, links are rewritten.** A frame moves only when the spill vector reallocates (amortised over its growth) or when the `max_frames` policy drops a middle layer (7.18); after either, the links of every layer from that point outward are rewritten, under `&mut`, before any reader can see them. The 0.3 bug found by Miri during development (the newest layer relinked before it was counted) is covered by `test_raw_spill_growth_keeps_links`.
- **Why not pointer arithmetic from `&self`.** Inside `source(&self)`, `self` covers only the frame's own bytes; stepping from it to a neighbouring slot is undefined behaviour under Stacked Borrows (Miri rejects it; Tree Borrows accepts it, which is why both models run in CI).
- **Thread safety.** The link makes `Frame` neither `Send` nor `Sync` by default; `raw.rs` carries the `unsafe impl`s, with invariant I-12 (7.3) as their argument.

**Three mechanisms were measured** (D-009, D-011). A prototype on 2026-10-23 measured the first two; the crate itself, built with the third, was measured with criterion on 2026-10-29 (rustc 1.99.0, `opt-level = 3`, fat LTO, one codegen unit, Windows 11, AMD Ryzen 9 9950X3D, the Windows system allocator, backtraces off, the machine shared with other builds):

| Hand-written `source()` walk from `&*error` | 1 layer | 3 layers | 5 layers | 9 layers |
|---|---|---|---|---|
| address comparison against the slots (prototype; frame keeps the node pointer, 40 bytes) | 5.2 to 6.4 ns | 11.5 to 23.3 ns | 19.7 to 34.4 ns | 41 to 77 ns |
| per-frame index (prototype; 48-byte frames) | 5.6 to 6.2 ns | 13.1 to 18.0 ns | 17.5 to 22.0 ns | 31.8 to 33.8 ns |
| **direct link (built; 40-byte frames)**, criterion | **2.93 ns** | **5.64 ns** | **8.07 ns** | **13.69 ns** |
| anyhow 1.0.104, criterion | 2.98 ns | 6.35 ns | 9.85 ns | 16.78 ns |

The prototype figures are the range over alternating runs of a plain timing loop; they are slower in absolute terms than criterion's on every row (anyhow too), so the comparison that decides is within each harness: in the prototype both stored-pointer variants were 1 to 30 ns behind anyhow, and the direct link is level with anyhow at one layer and 11% to 18% faster from three layers on. **Decision (D-011): the direct link.** It keeps the 40-byte frame, the one machine word, and every allocation count of 7.5, and makes a hand-written walk as fast as anyhow's.

`Frame` is 40 bytes on 64-bit targets (`String`'s capacity niche keeps `FrameValue` at 24 bytes) [measured]; the crate asserts five words at compile time. The header is 72 bytes at 0.3 (vtable, origin, extension pointer, the layer and value counters, the omitted counter, the first layer); the code, flags, severity, and exit fields of 7.2 join it in 0.4 and 0.5.

With a real chain, `into_std()` boxes the `Error` itself in a private wrapper whose `Error` impl delegates to the deref target: one allocation in all, however many layers there are (the earlier design rebuilt a nested chain, one allocation per layer). `from_boxed` of such a box gives the original `Error` back.

### 7.7 Chain and tree walking (F9, R14)

The cause **chain** of an error, outermost first:

1. the context layers, newest to oldest (`Ext.frames` reversed, then `first`);
2. the root error (the payload);
3. the root error's `source()` chain.

Every link is a real `source()` link (7.6.1), and `chain()` yields exactly the links a hand-written `source()` walk from `&**self` visits, in the same order, as `&(dyn Error + 'static)` like anyhow's `chain()`. It reads the context layers from the flat store by index (outermost first) and then follows the root's own `source()` chain, which is what makes it faster than anyhow's `chain()` (measured in 7.6.1, D-011). Walking stops after `max_chain` links (default 128), which also stops a buggy `source()` that returns itself or loops (10.1 T5). There is no repeat check (D-020): an error and the first field of its struct share an address, and two trait-object pointers to the same object can carry different vtable pointers, so no comparison on stable Rust tells a cycle from a newtype wrapper reliably; the budget alone ends every walk, and `Chain::len()` is exact.

The **tree** adds children (several causes) below the root: `find::<T>()` visits, depth first, the context values, the root error, its sources, then each child's tree, up to `max_depth` (default 16) and `max_chain` per chain. Suppressed errors are not causes and are not searched unless `find_in_suppressed::<T>()` is called. The walk is iterative (an explicit stack bounded by `max_depth`), never recursive, so a deep tree cannot overflow the stack.

The **innermost code** (`code()`): the explicitly set code if any; otherwise L1's `innermost_code` (X-6) walks the root error's `source()` chain and returns the innermost link whose probe carries a code, with its `Identity` (severity, category, kinds, type name). Context layers never carry a code. Children are not consulted (a group has its own code or none).

### 7.8 Formatting rules (R7)

The same error, created by:

```rust
fn read_config() -> Result<String> {
    Ok(std::fs::read_to_string("app.noml").context("reading config")?)    // line 2
}
fn start() -> Result<()> {
    let _text = read_config().context("starting server")?;               // line 5
    Ok(())
}
```

| Format | Output |
|---|---|
| `{}` | `starting server` |
| `{:#}` | `starting server: reading config: No such file or directory (os error 2)` |
| `{:?}` | the report below |
| `{:#?}` | `Error { message: "starting server", code: None, location: src/main.rs:2:61, layers: ["starting server", "reading config"], root: Os { code: 2, kind: NotFound, message: "No such file or directory" }, children: 0, suppressed: 0, attachments: 0 }` |

`{:?}`, which is also what `fn main() -> Result<()>` prints after std's `Error: ` prefix:

```text
starting server

Caused by:
    0: reading config
    1: No such file or directory (os error 2)

Location: src/main.rs:2:61
Trace:
    0: src/main.rs:2:61  (origin)
    1: src/main.rs:2:61  reading config
    2: src/main.rs:5:31  starting server
```

The full order of the `{:?}` report, each block printed only when it has content:

1. the outermost message;
2. `Caused by:` then the rest of the chain (one unnumbered line if there is one cause, numbered `0:`, `1:` if several, as anyhow prints);
3. `Causes (n):` the children, as an indented tree, each child in its `{:#}` form, `max_depth` levels;
4. `Note:`, `Warning:`, `Suggestion:` lines in the order added;
5. `Suppressed:` each suppressed error in its `{:#}` form;
6. `Code: DB-0042 (unavailable, severity 17)` when a code is known;
7. `Location:` the origin;
8. `Trace:` when there is more than the origin;
9. `Attachments:` the attachments added with `attach_display`;
10. `Fields:` the fields added by the creation hook;
11. `Omitted:` the counters of anything dropped at a budget;
12. `Backtrace:` when one was captured (symbols resolved now, not before).

**The de-duplication rule** (the L3 part of R7): in `{:#}` and in the `Caused by:` list, a layer is skipped when the layer before it already ends with `": "` followed by exactly its text, or is exactly its text. This removes the common repetition from typed errors whose `Display` already includes their source (`#[error("io error: {0}")]` with `#[source]`). Only adjacent layers are compared; L4 owns any fuzzier rule.

`{}` and `{:#}` never resolve a backtrace and never allocate; `{:?}` may (backtrace symbols).

### 7.9 Propagation trace (R11)

The trace answers "which way did this error travel", cheaper than a backtrace and present in release builds.

- **Recorded at:** creation (the origin: `From::from` behind `?`, `Error::new`, `err!`, `bail!`, `ensure!`, all `#[track_caller]`), every `.context()` and `.with_context()` (the frame's own location), and every explicit `.trace()` (a trace point).
- **Not recordable:** a `?` that does not change the error type (`Result<_, Error>` into `Result<_, Error>`) calls core's identity `From<T> for T`, which is not `#[track_caller]` and cannot be replaced. This is why `.trace()` exists: `step().trace()?` records that line for one store.
- **Storage:** frame locations live in their frames; trace points live in a fixed array of 15 in `Ext` that keeps the first 4 and the latest 11, with a saturating count of dropped points. The `Trail` iterator merges origin, frames, and trace points in the order they happened (each carries a 16-bit sequence number).
- **Cost:** a location is a `&'static Location` (8 bytes, no allocation); recording is one store plus, the first time, the `Ext` allocation.

### 7.10 Attachments (R12) and sections (R13)

**Attachments** are values of any type `A: Send + Sync + 'static`, stored as `Box<dyn Any + Send + Sync>` with their `TypeId`. `attachment::<A>()` returns the most recently attached `A`; `attachments::<A>()` iterates all of them, newest first. This is error-stack's `downcast_ref`-style retrieval, which works on stable Rust, without the nightly `provide`/`request_ref` API. Attachments are **opaque**: never printed, never sent through `report()` as text, unless attached with `attach_display`, which also stores a formatter function and prints them under `Attachments:`.

**Sections** are text: `note`, `warning`, `suggestion` (and lazy `with_note`, `with_warning`, `with_suggestion`). A `&'static str` is stored as is; anything else is formatted once, when attached, into a bounded `Box<str>` (budget `max_bytes`). Formatting at attach time means the report never calls a section's `Display` later, so a section cannot panic during rendering, and its size is known when it is charged against the budget. L4 renders sections with colour; report-err prints them as plain lines.

### 7.11 Several causes (R14) and collections (F10)

An error can have **children**: the failures it consists of. Three ways to build one:

```rust
// 1. Gather by hand (validation).
let mut errors = Errors::new();
if name.is_empty() { errors.push(err!("name is empty")); }
if age > 150 { errors.push(err!("age {age} is out of range")); }
errors.finish("invalid user record")?;        // Ok(()) if nothing was pushed

// 2. Collect every failure of an iterator, not only the first.
let parsed: Vec<Config> = files.iter().map(load).collect_all()?;

// 3. Wrap existing errors.
let error = Error::group("3 of 10 shards failed", shard_errors);
```

- `Errors` counts every failure pushed, even past the budget (the count is exact; the stored list is bounded by `max_tree`).
- `collect_all()` is an extension method on any iterator of `Result<T, E>` with `E: Into<Error>`; it consumes the whole iterator, returns `Ok(collection)` if every item succeeded, else a group error whose message is `"{failed} of {total} items failed"` and whose children are the failures in order.
- `split(predicate)` divides a group into the children that match and those that do not, each half keeping the group's message, code, and sections (Python `ExceptionGroup.split`).
- `flatten()` returns the leaf errors of the tree in depth-first order (Go `errors.Join` read the other way).
- The tree is bounded in total size (`max_tree`, default 1024 errors including nested children); an error records the size of its own tree, so adding a child charges the child's whole tree.

### 7.12 Suppressed errors (R15)

A suppressed error is a failure that happened while handling another one (closing a file after a write failed). It is shown in the report but is not a cause: `find` and `code()` ignore it, `downcast` ignores it.

```rust
let result = write_all(&mut file, &data)
    .with_cleanup(|| file.sync_all());       // runs the cleanup either way
```

`with_cleanup` returns the main error with the cleanup's error suppressed in it if both fail, the cleanup's error if only the cleanup fails, and the main result otherwise. `unraisable(error)` is for errors with nowhere to go (in `Drop`, in a detached thread): it reports through the L1 facade as an unhandled error (X-8; the record's `handled` field is `false`); when no reporter is installed and `std` is on, it writes one guarded line to standard error so the error is never lost silently (RD-15).

### 7.13 Backtraces (F11)

- Captured with `std::backtrace::Backtrace::capture()` **only when the root is created** (never on context, which keeps context cheap), and only when std's rules enable it: `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE` set and not `0` (`RUST_LIB_BACKTRACE=0` turns error backtraces off while leaving panic backtraces on, the same rules anyhow documents).
- `with_backtrace()` forces a capture for one error regardless of the environment.
- Stored in `Ext`, so an error without a backtrace pays nothing for the field.
- Symbols are resolved only when the backtrace is printed (`{:?}`, `backtrace()` then `Display`).
- Costs [measured, same machine as 7.4]: `Backtrace::capture()` with the variables unset, 1.8 ns (the enabled check is a cached load); `Backtrace::force_capture()` without resolution, about 0.82 µs; capture plus resolution and formatting, about 357 µs. These numbers are why capture is off by default and why `{}` and `{:#}` never resolve.
- `Backtrace::frames()` is nightly-only; report-err only prints backtraces and never inspects frames.

### 7.14 `main` integration (R2)

Two ways, both one line:

```rust
// 1. Plain std Termination: prints "Error: " + {:?}, exits with 1.
fn main() -> report_err::Result<()> {
    run()
}

// 2. ExitReport: same report, exit code chosen from the error.
fn main() -> report_err::ExitReport {
    run().into()
}
```

`ExitReport` implements `std::process::Termination` (stable since Rust 1.61). On `Err` it writes `Error: ` and the guarded, bounded `{:?}` report to standard error, escaping terminal control characters (10.1 T6), ignoring a closed or broken standard error, and returns the exit code chosen in this order:

1. an explicit `with_exit_code(n)` on the error (`n` from 1 to 255; 0 is refused at the call and becomes 1);
2. an error created by a panic (`boundary`, `from_panic`): 101, the code Rust uses for a panicking `main`;
3. the error's `category()` (7.17) through L1's `Category::exit_code()` (X-3: sysexits values, 64 usage, 65 data, 66 no input, 69 unavailable, 70 software, 73 cannot create, 74 I/O, 75 temporary failure, 77 permission, 78 configuration, 130 cancelled), **when the category is known**: from a code, a configured catalog, or an `io::Error` root through `Category::from_io_kind`;
4. otherwise 1 (`EXIT_FAILURE`). L1 maps `Unknown` to 70; report-err deliberately uses 1 for an error it knows nothing about, which is what anyhow users get today (L1 OD-10 leaves this policy to L3).

`report_err::run(app)` combines `boundary` and `ExitReport`: `fn main() -> ExitReport { report_err::run(app) }` turns a panic in `app` into a report and exit code 101 when panics unwind.

### 7.15 Panic boundary (R24)

```rust
pub fn boundary<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + UnwindSafe;
```

- Calls `std::panic::catch_unwind(f)`. `Ok(result)` passes through; a caught panic becomes an `Error` whose root is the panic message (`&'static str` and `String` payloads), or `panicked with a non-string payload`, with the `panicked` flag and `Kinds::PANIC` (L1 bit 10, reserved for L3 and L7) set, category `Internal`, exit code 101, and the origin at the `boundary` call.
- **Under `panic = "abort"` it catches nothing**: the process aborts at the panic, as std documents for `catch_unwind`. The portfolio's release profile uses `panic = "abort"`, so in those release builds `boundary` is a plain call. `report_err::PANICS_CAUGHT` (`cfg!(panic = "unwind")`) says which applies; the README states it beside the example.
- The panic hook still runs first (std calls it before unwinding), so the default hook still prints `thread '...' panicked at ...`. report-err never installs or replaces a hook; L7 owns hooks (R8).
- The bound is `UnwindSafe`, as std's: a closure that captures `&mut` state must be wrapped in `AssertUnwindSafe` by the caller, who then asserts that the state is not observed broken (RD-6).
- Dropping a panic payload can itself panic (std documents this); `from_panic` drops the payload inside a second `catch_unwind`, and if that panics too, the second payload is leaked with `mem::forget` (bounded: one payload per occurrence), never propagated.
- Foreign exceptions caught by `catch_unwind` have unspecified behaviour in std (abort, or an opaque payload); report-err maps an opaque payload to `panicked with a non-string payload` and documents the rest.
- `Error::from_panic(payload)` is available without `boundary` for `JoinHandle::join`: `handle.join().map_err(Error::from_panic)??`.

### 7.16 Creation hook (R25) and reporting

When an `Error` is created (not when context is added), report-err:

1. builds the header and payload (one allocation);
2. under `std`, captures a backtrace if enabled (7.13);
3. loads the creation hook slot (X-9: one acquire load; in L1 if L1 accepts the request, else in report-err). If a hook is installed and this thread is not already inside a hook (a thread-local flag under `std`), it calls `hook.on_create(&info, &mut annotate)`, where `info` carries the origin location, the explicit code if any, and `core::any::type_name` of the root error, and `annotate` lets the hook add fields and kinds (charged against `max_bytes`). Under `std` with unwinding, the call is wrapped in `catch_unwind`; a panicking hook leaves the field `creation_hook = "panicked"` on the error and the error creation continues. Under `no_std` there is no thread-local flag: the hook contract (documented with the slot, X-9) forbids creating an `Error` inside the hook.

`report(&self) -> bool` sets the `reported` flag with one atomic `fetch_or`; if the flag was already set it returns `false`; otherwise, if L1 reports a reporter installed (`is_installed()`, one load), it builds the error's `ErrorRecord` through the guarded renderer and delivers it (`report_record`, X-8; or the fallback in X-8), and returns `true`. Because the record's messages are rendered by report-err's guarded writer, a reporter never calls a user's `Display` unguarded. With no reporter installed the cost is the flag and one load, and nothing is allocated.

`record(&self) -> ErrorRecord` builds L1's owned snapshot (X-7) through the same guarded renderer, within L1's size caps; L7 calls it when a listener needs an owned copy, and relay-io carries records (not errors) across process and machine boundaries.

Errors that carry `Kinds::SECURITY` are reported like every other; report-err has no path that keeps a security error local (James's rule: security objects join the coms bridge).

### 7.17 Codes, severity, category, kinds

| Accessor | Order of lookup |
|---|---|
| `code()` | explicit (`coded`, `bail!(CODE, ...)`, `ensure!(cond, CODE, ...)`) → innermost coded error in the chain (7.7) → `None` |
| `severity()` | explicit `with_severity` → the identity of the coded error found by `code()` (X-6) → for an explicit code, its `CodeInfo` in the configured catalogs (CFG-05, X-5) → 21 (`FATAL`) for a panic → the category's default severity (L1 7.8) |
| `category()` | the coded error's identity → the configured catalogs for an explicit code → `Category::from_io_kind` when the root is a `std::io::Error` (std) → `Internal` for a panic → `Unknown` |
| `kinds()` | `with_kind` bits, OR the coded error's kinds, OR the creation hook's kinds, OR `Kinds::PANIC` for a panic; `Kinds::ERROR` when empty |
| `exit_code()` | 7.14 order |
| `public_message()` | the innermost coded error's L1 public message, else `None`. The `Display` of an error is a private (log) message and must never be sent to a client; L4 uses `public_message()` and the category |

### 7.18 Budgets

Every growable part of an error is bounded. The defaults are safe for servers; `configure(Limits)` changes them once at start (RD-8).

| Budget | Default | Allowed range | At the limit |
|---|---|---|---|
| `max_frames` (context layers per error) | 64 | 4 to 4,096 | keep the innermost 8 and the newest; drop the oldest of the middle; count `omitted.frames` |
| trace points | 15 (fixed) | fixed | keep first 4 and latest 11; count `omitted.trace` |
| `max_attachments` | 32 | 0 to 1,024 | refuse the new one; count |
| `max_sections` | 32 | 0 to 1,024 | refuse the new one; count |
| `max_suppressed` | 16 | 0 to 1,024 | refuse the new one; count |
| `max_tree` (errors in one tree, nested children included) | 1,024 | 1 to 1,048,576 | refuse the child; count (`Errors` keeps the exact failure count) |
| `max_depth` (nesting of children) | 16 | 1 to 64 | a child that would exceed the depth is refused; count |
| `max_bytes` (text and shallow value bytes owned by one error's extension: formatted context, sections, display attachments, hook fields, `size_of` of boxed values) | 64 KiB | 1 KiB to 16 MiB | truncate text at a character boundary and append `...(truncated)`; refuse values; count |
| `max_chain` (links walked in one chain) | 128 | 8 to 4,096 | stop; print `(chain truncated)` |
| `max_render` (bytes written by guarded rendering) | 1 MiB | 4 KiB to 64 MiB | stop; print `(report truncated)` |

Budgets are checked before the allocation or the work. Worst-case memory of one tree at the defaults: 1,024 errors × (80-byte header + payload + about 430-byte `Ext` + 64 KiB) ≈ 66 MiB [est.]; a typical error is 80 bytes plus its payload. `docs/PERFORMANCE.md` publishes the measured numbers.

### 7.19 `no_std` + `alloc`

| Feature | `no_std` + `alloc` | needs `std` |
|---|---|---|
| `Error`, one word, all constructors except `from_panic` payload text extraction (works: `Box<dyn Any + Send>` is in `alloc`) | yes | |
| `?` from any `core::error::Error` (stable in `core` since Rust 1.81, the MSRV) | yes | |
| `context`, `with_context`, `Option` context, `coded` | yes | |
| `err!`, `bail!`, `ensure!`, `Result` alias | yes | |
| downcasting, `chain`, `root_cause`, `find`, codes, severity, category, kinds | yes | |
| `Display`, `Debug` rules (7.8) | yes | |
| location, trace, `Trail` | yes (`core::panic::Location`) | |
| attachments, sections, children, `Errors`, `collect_all`, `split`, `flatten`, suppressed | yes | |
| budgets and `configure` | yes (one atomic pointer; targets need 32-bit atomic compare-and-swap) | |
| `report()`, `record()`, `unraisable()` through the L1 facade | yes | the stderr fallback of `unraisable` |
| creation hook | yes, without the reentrancy guard and without panic isolation | guard and isolation |
| guarded rendering: bounds and `fmt::Error` handling | yes | panic isolation (`catch_unwind`) |
| backtraces | no | yes |
| `boundary`, `run` | no | yes |
| `ExitReport`, `Termination`, exit codes | no | yes |
| `into_std` returning `Box<dyn core::error::Error + Send + Sync>` | yes | |

Targets without atomic compare-and-swap (`thumbv6m`) are not supported; `cfg(target_has_atomic = "ptr")` gates the crate with a clear `compile_error!`.

### 7.20 Thread safety

`Error` is `Send + Sync`. All `&self` methods are read-only except `report()`, which uses an atomic flag, so a `&Error` (for example inside an `Arc`) can be read and reported from several threads; exactly one `report()` call returns `true`. Mutation (`context`, `attach`, `note`, children) takes `self` by value or `&mut self`. Attachments, context values, and children must be `Send + Sync`. The write-once configuration is published with release ordering and read with acquire ordering; a racing `configure` from two threads succeeds once and returns `AlreadyConfigured` on the other (`loom` tested).

### 7.21 Features

| Feature | Default | Adds |
|---|---|---|
| `std` | yes | backtraces, `boundary`, `run`, `ExitReport`, the creation-hook guard and panic isolation, guarded-rendering panic isolation, the `unraisable` fallback |
| `testing` | no | assertions for tests (8.13) |

No other features. The L1 dependency is always on (`default-features = false`, plus L1's `alloc` feature and, with `std`, its `std` feature).

### 7.22 What can never be observed (stated in the docs)

From the error collection plan 10.5, as it applies here: report-err sees an error when an `Error` is created (constructors, the type-changing `?`), when context or `.trace()` is added, when `report()` is called, at `boundary`, and when `main` returns. It never sees an error discarded with `let _ =`, `.ok()`, or `unwrap_or`; an error handled inside third-party code; a same-type `?` (7.9); or a panic under `panic = "abort"`. A creation hook does not run for typed errors that never become an `Error`.

---

## 8. API specification

### 8.1 Conventions

- Every item has an id (`ERR-03`), a **tier** (T1, T2, T3: the library standard 2.10), a **grade** (P0 to P3: 1.1 of the standard), and the version it ships in (section 14).
- Signatures are sketches in final form; generic bounds are exact. "Cost" targets are [est.] until 0.6 measures them.
- `Error` methods that return an `Error` take `self` and are `#[must_use]`; methods on `Result` and `Option` come from the sealed `Context` trait.
- Every method that records a location is `#[track_caller]`.
- report-err has almost no failure modes of its own: operations at a budget drop and count (7.18). The crate's only error type is `ConfigureError` (CFG-04).

### 8.2 Tier 1 at a glance

Tier 1 is one import, one type, three macros, and two methods. Everything here is what anyhow users already write.

```rust
use report_err::prelude::*;          // Error, Result, Context, CollectAll, err!, bail!, ensure!
// or, exactly like anyhow:
use report_err::{bail, ensure, Context, Result};
```

**Return any error with `?`:**

```rust
fn read(path: &str) -> Result<String> {
    let text = std::fs::read_to_string(path)?;      // io::Error becomes a Error
    Ok(text)
}
```

**Say what you were doing:**

```rust
let text = std::fs::read_to_string(path).context("reading the config file")?;
let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
```

**Turn a missing value into an error:**

```rust
let home = std::env::var_os("HOME").context("HOME is not set")?;
let user = users.get(&id).with_context(|| format!("no user {id}"))?;
```

**Fail with a message:**

```rust
bail!("port {port} is already in use");
return Err(err!("unexpected token {token:?}"));
ensure!(n <= MAX, "batch of {n} is larger than {MAX}");
ensure!(!name.is_empty());                 // "condition failed: !name.is_empty()"
```

**Fail with an error code (from L1):**

```rust
bail!(DbError::TIMEOUT, "query timed out after {ms} ms");
ensure!(row.active, UserError::INACTIVE, "user {id} is inactive");
let row = db.get(id).coded(DbError::NOT_FOUND)?;
```

**Look inside an error:**

```rust
if let Some(io) = error.downcast_ref::<std::io::Error>() {
    if io.kind() == std::io::ErrorKind::NotFound { create_default()?; }
}
if error.code() == Some(DbError::TIMEOUT) { retry_later(); }
for cause in error.chain() { eprintln!("  because: {cause}"); }
```

**Add a hint for the person reading the report:**

```rust
let config = load(path).suggestion("run `kraken init` to create one")?;
```

**Return it from `main`:**

```rust
fn main() -> Result<()> { run() }                          // exit code 1 on error
fn main() -> report_err::ExitReport { run().into() }      // exit code from the error's category
```

**Print it:**

```rust
println!("{error}");        // starting server
println!("{error:#}");      // starting server: reading config: No such file or directory (os error 2)
eprintln!("{error:?}");     // the full report (7.8)
```

### 8.3 Root items

| Id | Item | Tier | Grade | Since |
|---|---|---|---|---|
| ROOT-01 | `pub const VERSION: &str` | T3 | P0 | 0.1 |
| ROOT-02 | `pub type Result<T, E = Error> = core::result::Result<T, E>;` | T1 | P0 | 0.3 |
| ROOT-03 | `pub mod prelude` re-exporting `Error`, `Result`, `Context`, `CollectAll`, `err!`, `bail!`, `ensure!` | T1 | P0 | 0.3 |
| ROOT-04 | `pub fn Ok<T>(value: T) -> Result<T>` (anyhow's helper for closures and blocks whose error type cannot be inferred) | T1 | P2 | 0.4 |

ROOT-02 has a default error type, so `Result<T, MyError>` still works where a typed error is wanted (as with anyhow). ROOT-04 is not in the prelude because a glob import would shadow `core::result::Result::Ok` in pattern positions; it is used as `report_err::Ok(value)`.

```rust
let parse = |s: &str| -> report_err::Result<u32> { Ok(s.trim().parse()?) };
let value = (|| { let n: u32 = "7".parse()?; report_err::Ok(n * 2) })()?;
```

### 8.4 `Error`

| Id | Signature | Tier | Grade | Since | Behaviour | Cost target [est.] |
|---|---|---|---|---|---|---|
| ERR-01 | `pub struct Error` (one word; `Send + Sync + Unpin + UnwindSafe + RefUnwindSafe + 'static`; not `Clone`; `#[must_use]`) | T1 | P0 | 0.3 | 7.2 | 8 bytes |
| ERR-02 | `impl<E> From<E> for Error where E: core::error::Error + Send + Sync + 'static` (`#[track_caller]`) | T1 | P0 | 0.3 | what `?` calls; origin = the `?` | one allocation, ≤ anyhow + 5% |
| ERR-03 | `pub fn new<E>(error: E) -> Error where E: core::error::Error + Send + Sync + 'static` | T1 | P0 | 0.3 | as ERR-02, explicit | as ERR-02 |
| ERR-04 | `pub fn msg<M>(message: M) -> Error where M: Display + Debug + Send + Sync + 'static` | T1 | P0 | 0.3 | root is a message; `downcast_ref::<M>()` returns it | one allocation |
| ERR-05 | `pub fn context<C>(self, context: C) -> Error where C: Display + Send + Sync + 'static` | T1 | P0 | 0.3 | adds a layer (7.6) | 0 allocations for the first layer and for layers 3 to 5 of `&'static str`; ≤ 3 ns |
| ERR-06 | `pub fn downcast_ref<T>(&self) -> Option<&T> where T: Display + Debug + Send + Sync + 'static` | T1 | P0 | 0.3 | searches the root and the context values, newest layer first | ≤ 5 ns with 3 layers |
| ERR-07 | `pub fn downcast_mut<T>(&mut self) -> Option<&mut T>` (same bound) | T2 | P1 | 0.3 | as ERR-06, mutable | as ERR-06 |
| ERR-08 | `pub fn downcast<T>(self) -> core::result::Result<T, Error>` (same bound) | T2 | P1 | 0.3 | moves the root or a context value out; on a miss returns the error unchanged | one free on a hit |
| ERR-09 | `pub fn is<T>(&self) -> bool` (same bound) | T1 | P1 | 0.3 | `downcast_ref::<T>().is_some()` | as ERR-06 |
| ERR-10 | `pub fn location(&self) -> &'static Location<'static>` | T1 | P1 | 0.3 | the origin | 1 load |
| ERR-11 | `pub fn code(&self) -> Option<Code>` | T1 | P0 | 0.4 | 7.17 | ≤ 60 ns for a 5-link chain (L1 estimates `identify` at about 10 ns per generated link) |
| ERR-12 | `pub fn coded(self, code: Code) -> Error` | T1 | P0 | 0.4 | sets the explicit code (replaces any earlier explicit code) | 1 store |
| ERR-13 | `pub fn severity(&self) -> Severity`; `pub fn category(&self) -> Category`; `pub fn kinds(&self) -> Kinds` | T2 | P1 | 0.4 | 7.17 | `innermost_code` walk, or a catalog scan for an explicit code |
| ERR-14 | `pub fn with_severity(self, severity: Severity) -> Error`; `pub fn with_kind(self, kinds: Kinds) -> Error` | T2 | P2 | 0.4 | overrides (kinds are added with OR) | 1 store |
| ERR-15 | `pub fn public_message(&self) -> Option<&'static str>` | T2 | P1 | 0.4 | the public message of the code: from its catalog entry (CFG-05), or from the root's `ErrorInfo` when built with `from_info`; when only the identity is known (the probe carries no public text), the category's default public message; `None` without a code. Never any `Display` text (D-015) | chain walk |
| ERR-16 | `pub fn is_panic(&self) -> bool` | T2 | P2 | 0.5 | created by `boundary` or `from_panic` | 1 load |
| ERR-17 | `impl Display`, `impl Debug` | T1 | P0 | 0.3 | 7.8 | `{}` ≤ cost of the outer layer's own `Display` + 2 ns |
| ERR-18 | `pub fn from_info<E>(error: E) -> Error where E: define_err::ErrorInfo + Send + Sync + 'static` | T3 | P2 | 0.4 | as ERR-03, and keeps the root reachable as `&dyn ErrorInfo` (HOOK-05), so per-value metadata the probe cannot carry (`retry_after`, `#[location]` fields) survives; L1 7.11 expects L3 to capture it while the type is known | as ERR-03 |
| ERR-19 | `impl Deref<Target = dyn Error + Send + Sync + 'static>`, `DerefMut`, `AsRef<dyn Error + Send + Sync + 'static>`, `AsRef<dyn Error + 'static>` for `Error` | T1 | P0 | 0.3 | 7.6.1, D-003: the outermost layer, whose `source()` chain walks every layer and the root | no allocation; `source()` per link measured in 7.6.1 |

```rust
// ERR-03, ERR-04
let a = Error::new(std::io::Error::other("disk full"));
let b = Error::msg("no input files");
let c = Error::msg(format!("{count} files failed"));

// ERR-05 on a Error you already hold
let error = error.context("while compacting level 2");

// ERR-06, ERR-08: context values are downcastable too (anyhow behaviour)
#[derive(Debug)] struct ShardId(u32);
impl std::fmt::Display for ShardId { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "shard {}", self.0) } }
let error = load_shard(7).context(ShardId(7)).unwrap_err();
assert_eq!(error.downcast_ref::<ShardId>().map(|s| s.0), Some(7));
let io: std::io::Error = error.downcast().expect("root was an io::Error");

// ERR-11, ERR-12
match load_user(id) {
    Err(f) if f.code() == Some(DbError::TIMEOUT) => retry_later(),
    Err(f) => return Err(f.context("in the account page")),
    Ok(user) => user,
}
```

### 8.5 The `Context` trait (Result and Option)

```rust
pub trait Context<T, E>: private::Sealed {
    fn context<C>(self, context: C) -> Result<T, Error> where C: Display + Send + Sync + 'static;
    fn with_context<C, F>(self, f: F) -> Result<T, Error> where C: Display + Send + Sync + 'static, F: FnOnce() -> C;
    fn coded(self, code: Code) -> Result<T, Error>;
    fn note<S>(self, text: S) -> Result<T, Error> where S: Display + Send + Sync + 'static;
    fn with_note<S, F>(self, f: F) -> Result<T, Error> where S: Display + Send + Sync + 'static, F: FnOnce() -> S;
    fn warning<S>(self, text: S) -> Result<T, Error> where S: Display + Send + Sync + 'static;
    fn with_warning<S, F>(self, f: F) -> Result<T, Error> where S: Display + Send + Sync + 'static, F: FnOnce() -> S;
    fn suggestion<S>(self, text: S) -> Result<T, Error> where S: Display + Send + Sync + 'static;
    fn with_suggestion<S, F>(self, f: F) -> Result<T, Error> where S: Display + Send + Sync + 'static, F: FnOnce() -> S;
    fn attach<A>(self, value: A) -> Result<T, Error> where A: Send + Sync + 'static;
    fn trace(self) -> Result<T, Error>;
    fn with_cleanup<F, R>(self, cleanup: F) -> Result<T, Error> where F: FnOnce() -> core::result::Result<(), R>, R: Into<Error>;
}
```

Implemented for `Result<T, E>` where `E: core::error::Error + Send + Sync + 'static`, for `Result<T, Error>`, and for `Option<T>` (where `None` becomes an error whose root is the context message, as anyhow does). The two `Result` impls do not overlap because `report_err::Error` does not implement `core::error::Error` (it dereferences to one, which coherence does not count); the trait is sealed (as anyhow's is) so that stays true. All methods are `#[track_caller]`. On `Ok` and `Some` every method returns the value untouched and no closure runs.

| Id | Method | Tier | Grade | Since | Notes |
|---|---|---|---|---|---|
| CTX-01 | `context`, `with_context` on `Result` | T1 | P0 | 0.3 | F8 |
| CTX-02 | `context`, `with_context` on `Option` | T1 | P0 | 0.3 | R3 |
| CTX-03 | `coded` on `Result` and `Option` | T1 | P0 | 0.4 | on `Option`, `None` becomes an error with that code whose message is the code's catalog title when a catalog is configured (CFG-05), else the code itself (`DB-0042`) (R3's `ok_or_code`, renamed: RD-7) |
| CTX-04 | `note`, `warning`, `suggestion` and their `with_` forms | T1 | P1 | 0.4 | R13 |
| CTX-05 | `attach` | T2 | P1 | 0.4 | R12 |
| CTX-06 | `trace` | T2 | P1 | 0.4 | R11 |
| CTX-07 | `with_cleanup` | T2 | P2 | 0.5 | R15 |

```rust
// CTX-02: the common anyhow pattern `.ok_or_else(|| anyhow!(...))` becomes one call
let parent = path.parent().context("entry file has no parent directory")?;

// CTX-03 on Option
let user = cache.get(&id).coded(UserError::NOT_FOUND)?;

// CTX-04
let bytes = read_blob(id)
    .note(format!("blob {id} is referenced by {count} pages"))
    .suggestion("run `hive repair --blobs`")?;

// CTX-05
let rows = query(sql).attach(RequestId(req.id)).attach(Sql(sql.to_owned()))?;

// CTX-06: record a same-type `?` that would otherwise leave no trace (7.9)
let plan = optimise(plan).trace()?;

// CTX-07
let written = write_all(&mut file, &data).with_cleanup(|| file.sync_all())?;
```

### 8.6 Macros

| Id | Macro | Tier | Grade | Since |
|---|---|---|---|---|
| MAC-01 | `err!` | T1 | P0 | 0.3 |
| MAC-02 | `bail!` | T1 | P0 | 0.3 |
| MAC-03 | `ensure!` | T1 | P0 | 0.3 |
| MAC-04 | code-first arms of all three (`bail!(CODE, "...")`) | T1 | P0 | 0.4 |
| MAC-05 | `ensure!` operand values for comparisons (`condition failed: a == b (left: 3, right: 4)`) | T1 | P3 | after 1.0 (RD-10) |

Arms, tried in this order (`err!` shown; `bail!(...)` is `return Err(err!(...))`; `ensure!(cond, ...)` is `if !cond { bail!(...) }`):

| Arm | Example | Result |
|---|---|---|
| `err!($msg:literal)` | `err!("no input")` | message root; `&'static str` when the literal has no `{}` arguments (no allocation) |
| `err!($fmt:literal, $($arg:tt)*)` | `err!("bad port {}", p)` | formatted message root |
| `err!($code:expr, $fmt:literal $(, $($arg:tt)*)?)` | `err!(DbError::TIMEOUT, "after {ms} ms")` | as above, with the code set; `$code` must be an L1 `Code` (a type error otherwise, with a `#[diagnostic::on_unimplemented]` note) |
| `err!($err:expr)` | `err!(io_err)`, `err!(message_string)` | an error value becomes the root (`Error::new`); any other `Display + Debug` value becomes a message root (`Error::msg`). Chosen at compile time by autoref dispatch, the stable technique anyhow uses |

`ensure!($cond:expr)` with no message produces `condition failed: ` followed by `stringify!($cond)`. Inline format captures (`"{name}"`) work in every format arm.

```rust
fn check_header(h: &Header) -> Result<()> {
    ensure!(h.magic == MAGIC, "bad magic {:#x}", h.magic);
    ensure!(h.version <= 3, FormatError::UNSUPPORTED_VERSION, "version {} is newer than 3", h.version);
    ensure!(h.len > 0);
    if h.flags & RESERVED != 0 {
        bail!("reserved flags set: {:#b}", h.flags & RESERVED);
    }
    Ok(())
}

// The anyhow pattern `.map_err(|e| anyhow!(e))` for an error type that is not `std::error::Error`:
let sigs = load_signatures().map_err(|e| err!(e)).context("invalid FFI signature table")?;
```

### 8.7 Chain tools (F9)

| Id | Signature | Tier | Grade | Since | Behaviour |
|---|---|---|---|---|---|
| CHN-01 | `pub fn chain(&self) -> Chain<'_>`; `Chain<'a>: Iterator<Item = &'a (dyn Error + 'static)> + ExactSizeIterator + FusedIterator + Clone` (as anyhow's) | T1 | P0 | 0.3 | 7.7, outermost first, bounded |
| CHN-02 | `pub fn root_cause(&self) -> &(dyn Error + 'static)` | T1 | P1 | 0.3 | the last link of the chain |
| CHN-03 | `pub fn find<T: Error + 'static>(&self) -> Option<&T>` | T2 | P1 | 0.5 | tree search, 7.7 |
| CHN-04 | `pub fn find_all<T: Error + 'static>(&self) -> FindAll<'_, T>` | T3 | P2 | 0.5 | every match, depth first |
| CHN-05 | `pub fn find_in_suppressed<T: Error + 'static>(&self) -> Option<&T>` | T3 | P2 | 0.5 | searches suppressed errors only |
| CHN-06 | `Chain::rev(self)` (double-ended through a bounded buffer of `max_chain` links) | T3 | P2 | 0.5 | innermost first |

```rust
// Exactly the loop kraken's test helper uses today (compiler/src/main.rs:1022):
for (index, cause) in error.chain().enumerate() {
    let text = cause.to_string();
    if index > 0 { combined.push_str("\ncaused by: "); }
    combined.push_str(&text);
}

// The innermost io error anywhere in the tree, for an exit code decision
if let Some(io) = error.find::<std::io::Error>() { eprintln!("I/O: {}", io.kind()); }

// Root cause for a one-line log
log::error!("{} (root: {})", error, error.root_cause());
```

### 8.8 Attachments and sections (R12, R13)

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| ATT-01 | `pub fn attach<A: Send + Sync + 'static>(self, value: A) -> Error` | T2 | P1 | 0.4 |
| ATT-02 | `pub fn attachment<A: 'static>(&self) -> Option<&A>` (newest) | T2 | P1 | 0.4 |
| ATT-03 | `pub fn attachments<A: 'static>(&self) -> Attachments<'_, A>` (all, newest first) | T3 | P2 | 0.4 |
| ATT-04 | `pub fn attach_display<A: Display + Send + Sync + 'static>(self, value: A) -> Error` (also printed) | T2 | P2 | 0.4 |
| ATT-05 | `pub fn note`, `warning`, `suggestion` and `with_note`, `with_warning`, `with_suggestion` on `Error` (same bounds as 8.5) | T1 | P1 | 0.4 |
| ATT-06 | `pub fn sections(&self) -> Sections<'_>`; item `Section<'a> { pub kind: SectionKind, pub text: &'a str }` | T3 | P1 | 0.4 |
| ATT-07 | `#[non_exhaustive] pub enum SectionKind { Note, Warning, Suggestion }` | T3 | P1 | 0.4 |

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestId(u64);

fn handle(req: &Request) -> Result<Response> {
    let order = load_order(req.order).attach(RequestId(req.id))?;
    Ok(render(order))
}

// At the request boundary: read the attachment back by type.
if let Err(error) = handle(&req) {
    let id = error.attachment::<RequestId>().copied();
    respond_error(id, error.category());
    error.report();
}

// Sections for people
let error = error
    .note("the cache was rebuilt 3 times in the last minute")
    .warning("data written after 12:04 may be missing")
    .suggestion("check disk health with `smartctl -a /dev/nvme0`");
```

### 8.9 Several causes and collections (R14, F10)

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| GRP-01 | `pub fn group<M, I>(message: M, errors: I) -> Error where M: Display + Debug + Send + Sync + 'static, I: IntoIterator<Item = Error>` | T2 | P1 | 0.5 |
| GRP-02 | `pub fn children(&self) -> &[Error]` | T2 | P1 | 0.5 |
| GRP-03 | `pub fn push_child(&mut self, child: impl Into<Error>) -> bool` (`false` when refused at a budget) | T3 | P2 | 0.5 |
| GRP-04 | `pub fn split<P: FnMut(&Error) -> bool>(self, predicate: P) -> (Option<Error>, Option<Error>)` | T3 | P2 | 0.5 |
| GRP-05 | `pub fn flatten(self) -> Vec<Error>` | T3 | P2 | 0.5 |
| GRP-06 | `pub struct Errors` with `new()`, `push(impl Into<Error>)`, `push_result<T, E: Into<Error>>(Result<T, E>) -> Option<T>`, `len()` (exact count, including refused), `is_empty()`, `finish(message) -> Result<()>`, `finish_with<T>(value: T, message) -> Result<T>`, `into_error(message) -> Option<Error>` | T2 | P1 | 0.5 |
| GRP-07 | `pub trait CollectAll<T, E>: Iterator<Item = Result<T, E>>` with `fn collect_all<B: FromIterator<T>>(self) -> Result<B>` | T2 | P1 | 0.5 |

```rust
// GRP-06: validation that reports every problem at once
fn validate(user: &NewUser) -> Result<()> {
    let mut errors = Errors::new();
    if user.name.trim().is_empty() { errors.push(err!("name is empty")); }
    if !user.email.contains('@') { errors.push(err!("email {:?} has no @", user.email)); }
    let age = errors.push_result(user.age.parse::<u8>().context("age is not a number"));
    if let Some(age) = age { if age < 13 { errors.push(err!("age {age} is under 13")); } }
    errors.finish("invalid sign-up form")
}

// GRP-07: load every file and report all failures, not only the first
let configs: Vec<Config> = paths.iter().map(|p| load(p)).collect_all()?;
// error: "2 of 9 items failed", children: the two load errors with their own context

// GRP-04, GRP-05
let (retryable, permanent) = error.split(|f| f.category().is_transient());
for leaf in permanent.map(Error::flatten).unwrap_or_default() { eprintln!("{leaf:#}"); }
```

### 8.10 Suppressed errors and cleanup (R15)

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| SUP-01 | `pub fn suppress(self, other: impl Into<Error>) -> Error` | T2 | P1 | 0.5 |
| SUP-02 | `pub fn suppressed(&self) -> &[Error]` | T2 | P1 | 0.5 |
| SUP-03 | `pub fn unraisable(error: Error) -> bool` (free function; `true` if a reporter received it) | T2 | P2 | 0.5 |
| SUP-04 | `Context::with_cleanup` (CTX-07) | T2 | P2 | 0.5 |

```rust
// SUP-01: keep the rollback failure beside the original error
if let Err(error) = tx.apply(batch) {
    let error = match tx.rollback() {
        Ok(()) => error,
        Err(rollback) => error.suppress(rollback),
    };
    return Err(error.context("applying batch"));
}

// SUP-03: an error in Drop has nowhere to go
impl Drop for Segment {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            report_err::unraisable(Error::new(e).context("flushing segment on drop"));
        }
    }
}
```

### 8.11 Trace and backtrace (R11, F11)

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| TRC-01 | `pub fn trace(self) -> Error` (and `Context::trace`, CTX-06) | T2 | P1 | 0.4 |
| TRC-02 | `pub fn trail(&self) -> Trail<'_>`; item `Step<'a> { pub location: &'static Location<'static>, pub kind: StepKind, pub context: Option<&'a (dyn Error + 'static)> }`; `Trail::omitted(&self) -> u32` | T2 | P1 | 0.4 |
| TRC-03 | `#[non_exhaustive] pub enum StepKind { Origin, Context, Trace }` | T3 | P1 | 0.4 |
| TRC-04 | `pub fn backtrace(&self) -> Option<&std::backtrace::Backtrace>` (feature `std`) | T2 | P1 | 0.4 |
| TRC-05 | `pub fn with_backtrace(self) -> Error` (feature `std`; forces a capture if none) | T2 | P2 | 0.4 |

```rust
for step in error.trail() {
    println!("{:?} at {}", step.kind, step.location);
}
// Origin at src/store.rs:88:21
// Context at src/store.rs:88:30
// Trace at src/engine.rs:204:41
// Context at src/main.rs:17:22

let error = err!("invariant broken in compaction").with_backtrace();   // always has one
```

### 8.12 Configuration

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| CFG-01 | `#[non_exhaustive] pub struct Limits` with `pub` fields `max_frames`, `max_attachments`, `max_sections`, `max_suppressed`, `max_tree`, `max_depth`, `max_bytes`, `max_chain`, `max_render` (all `u32`); `Limits::DEFAULT`; changed as a copy, like define-err's `RecordLimits` (D-013) | T2 | P1 | 0.5 |
| CFG-02 | `pub fn configure(limits: Limits) -> core::result::Result<(), ConfigureError>` (write once) | T2 | P1 | 0.5 |
| CFG-03 | `pub fn limits() -> &'static Limits` | T3 | P2 | 0.5 |
| CFG-04 | `#[non_exhaustive] pub enum ConfigureError { AlreadyConfigured, OutOfRange { field: &'static str, value: u64, min: u64, max: u64 } }` with `Display`, `Debug`, `Error` (`AlreadyConfigured` from 0.4 for `configure_catalogs`) | T2 | P1 | 0.5 |
| CFG-05 | `pub fn configure_catalogs(catalogs: &'static [&'static [define_err::CodeInfo]]) -> core::result::Result<(), ConfigureError>` (write once; lets an explicit code set with `coded` or a code-first macro find its title, severity, category, and kinds through L1's `catalog::find`) | T2 | P2 | 0.4 |

```rust
// In main, before anything else runs:
let mut limits = Limits::DEFAULT;
limits.max_frames = 32;
limits.max_bytes = 16 * 1024;
report_err::configure(limits)?;
report_err::configure_catalogs(&[DbError::CATALOG, UserError::CATALOG])?;   // optional
```

`ConfigureError` messages say what to do: `AlreadyConfigured` - "limits were already configured; call configure once, at the start of main, before other threads start"; `OutOfRange` - "max_frames = 2 is outside 4..=4096; choose a value in that range". Every variant has a test.

### 8.13 `main`, boundary, reporting, std interop, testing

| Id | Signature | Tier | Grade | Since |
|---|---|---|---|---|
| MAIN-01 | `fn main() -> Result<()>` through std's `Termination` for `Result<(), E: Debug>` (nothing to call) | T1 | P0 | 0.3 |
| MAIN-02 | `pub struct ExitReport`; `impl Termination`; `impl<E: Into<Error>> From<core::result::Result<(), E>> for ExitReport`, `impl From<Error> for ExitReport` (feature `std`) | T1 | P1 | 0.4 |
| MAIN-03 | `pub fn exit_code(&self) -> u8`; `pub fn with_exit_code(self, code: u8) -> Error` | T2 | P2 | 0.4 |
| MAIN-04 | `pub fn run<F: FnOnce() -> Result<()> + UnwindSafe>(app: F) -> ExitReport` (feature `std`) | T1 | P2 | 0.5 |
| BND-01 | `pub fn boundary<T, F: FnOnce() -> Result<T> + UnwindSafe>(f: F) -> Result<T>` (feature `std`) | T1 | P1 | 0.5 |
| BND-02 | `pub const PANICS_CAUGHT: bool` | T2 | P1 | 0.5 |
| BND-03 | `pub fn from_panic(payload: Box<dyn Any + Send + 'static>) -> Error` | T3 | P2 | 0.5 |
| HOOK-01 | creation hook: installed with define-err's `set_creation_hook` (X-9, accepted); report-err calls it once per `Error` created and owns the reentrancy guard and panic isolation | T3 | P1 | 0.5 |
| HOOK-02 | `pub fn report(&self) -> bool` | T2 | P1 | 0.5 |
| HOOK-03 | `pub fn is_reported(&self) -> bool` | T2 | P2 | 0.5 |
| HOOK-04 | `pub fn record(&self) -> define_err::ErrorRecord` | T2 | P1 | 0.5 |
| HOOK-05 | `pub fn info(&self) -> Option<&dyn define_err::ErrorInfo>` (the root's metadata when the error was built with `from_info`, ERR-18) | T3 | P2 | 0.4 |
| STD-01 | `pub fn into_std(self) -> Box<dyn core::error::Error + Send + Sync + 'static>` (nested chain, sources intact) | T3 | P1 | 0.3 |
| STD-02 | `impl From<Error> for Box<dyn Error + Send + Sync + 'static>`, `for Box<dyn Error + Send + 'static>`, and `for Box<dyn Error + 'static>` (as anyhow) | T3 | P1 | 0.3 |
| STD-03 | `pub fn from_boxed(error: Box<dyn Error + Send + Sync + 'static>) -> Error` | T3 | P1 | 0.3 |
| STD-04 | `pub fn render(&self, out: &mut dyn fmt::Write, style: RenderStyle) -> fmt::Result` (guarded, bounded; `RenderStyle::{Message, Chain, Report}` non-exhaustive) | T3 | P2 | 0.5 |
| TST-01 | `testing::assert_chain_contains(&Error, &str)`, `assert_code(&Error, Code)`, `assert_section(&Error, SectionKind, &str)` (feature `testing`; each panics with the full report on failure) | T2 | P2 | 0.5 |
| TST-02 | `testing::CountingHook` (a creation hook that counts calls, for tests of R25 wiring) | T3 | P3 | after 1.0 |

```rust
// MAIN-02 with tokio, as kraken's compiler runs today
#[tokio::main]
async fn main() -> report_err::ExitReport {
    run().await.into()
}

// MAIN-04: also turns a panic into a report when panics unwind
fn main() -> report_err::ExitReport {
    report_err::run(|| {
        let config = load_config("app.noml")?;
        serve(config)
    })
}

// BND-01: isolate a plugin call
let output = report_err::boundary(|| plugin.render(&page))
    .context("plugin `gallery` failed")?;

// BND-03: a thread that panicked
let total = worker.join().map_err(Error::from_panic)??;

// HOOK-02 at a request boundary
if let Err(error) = handle(req) {
    error.report();                         // once; outer layers see is_reported() == true
    return problem_response(&error);        // L4 builds the response from code/category
}

// STD-01: hand the error to a library that wants Box<dyn Error>
let boxed: Box<dyn std::error::Error + Send + Sync> = error.into_std();
```

### 8.14 Migrating from anyhow

#### 8.14.1 The rules (find-and-replace)

| anyhow | report-err | Note |
|---|---|---|
| `anyhow = "1"` in `Cargo.toml` | `report-err = "X.Y"` | |
| `use anyhow::{Context, Result};` | `use report_err::{Context, Result};` | same names |
| `use anyhow::{anyhow, bail, ensure, ...}` | `use report_err::{err, bail, ensure, ...}` | the one renamed item (RD-18) |
| `anyhow::Result<T>` | `report_err::Result<T>` | |
| `anyhow::Error` | `report_err::Error` | same name (D-002) |
| `anyhow::anyhow!(...)` / `anyhow!(...)` | `report_err::err!(...)` / `err!(...)` | same arms |
| `anyhow::format_err!(...)` / `format_err!(...)` | unchanged (`report_err::format_err!`, an alias of `err!`, RD-18) | same arms |
| `anyhow::bail!(...)` | `report_err::bail!(...)` | same arms |
| `anyhow::ensure!(...)` | `report_err::ensure!(...)` | same arms; the bare-condition message text differs (FR-8) |
| `anyhow::Ok(v)` | `report_err::Ok(v)` | |
| `Error::msg`, `Error::new`, `.context`, `.with_context`, `.chain()`, `.root_cause()`, `.downcast*`, `.is`, `.backtrace()` | same names on `Error` | |
| `.ok_or_else(\|\| anyhow!("text"))` | `.context("text")` | optional clean-up; the original also works |

A sed-style pass covers it (run on a branch, review the diff):

```text
s/\banyhow::anyhow!/report_err::err!/g
s/\banyhow::/report_err::/g
s/\buse anyhow::/use report_err::/g
s/\banyhow!(/err!(/g      (only in files that imported the macro by name)
```

#### 8.14.2 What behaves differently (and why)

| Difference | Effect on migrated code | Why |
|---|---|---|
| `ensure!(cond)` message | `condition failed: <cond>` instead of anyhow's wording | report-err's own text; tests that compare it must be updated |
| `{:?}` report | adds `Location:` and, when present, `Trace:`, sections, code, and other blocks after anyhow's `Caused by:` | 7.8; tests that compare the whole `{:?}` output must be updated |
| `downcast_ref::<T>()` on a source deeper than the root | `None`, as anyhow; use `find::<T>()` for a tree search | parity, plus a better tool |
| Exit status from `main` | unchanged (1) unless `ExitReport` is used | opt-in |

#### 8.14.3 kraken, worked through

Usage found on 2026-10-22 in the kraken repository:

| Location | Usage | Count | Migration |
|---|---|---|---|
| `kraken/Cargo.toml` (workspace) | `anyhow = "1.0"` | 1 | replace with `report-err` |
| `kraken/compiler/Cargo.toml` | `anyhow = { workspace = true }` | 1 | rename the workspace dependency |
| `kraken-lsp/Cargo.toml` | `anyhow = "1.0"` | 1 | **no source file in `kraken-lsp` uses anyhow**; remove the dependency rather than migrate it |
| `compiler/src/main.rs`, `compiler/src/modules/loader.rs` | `use anyhow::{Context, Result};` | 2 | `use report_err::{Context, Result};` |
| same two files | `anyhow::bail!(...)` | 26 | `report_err::bail!(...)` |
| same two files | `anyhow::anyhow!(...)` | 8 | `report_err::err!(...)`; the 4 inside `.ok_or_else(\|\| ...)` can become `.context(...)` |
| `compiler/src/main.rs:403`, `:578` (2 of the 8 above) | `.map_err(\|e\| anyhow::anyhow!(e))` | 2 | `.map_err(\|e\| err!(e))` (works for any `Display + Debug` value, 8.6) |
| same two files | `.context(...)` | 22 | unchanged |
| same two files | `.with_context(...)` | 12 | unchanged |
| `compiler/src/main.rs:1022` | `err.chain().enumerate()` with `cause.to_string()` | 1 | unchanged |
| `compiler/src/main.rs:118` | `#[tokio::main] async fn main() -> Result<()>` | 1 | unchanged, or `-> report_err::ExitReport` with `run().await.into()` for category exit codes |
| anywhere | `anyhow::Error` by name, `ensure!`, `downcast` | 0 | nothing to do |

All 71 uses (2 imports, 26 `bail!`, 8 `anyhow!`, 22 `context`, 12 `with_context`, 1 `chain`) are expected to compile after the find-and-replace pass with no hand edits, because none of them touches a difference listed in 8.14.2. That is a design claim until the 0.8 alpha (section 14) proves it by migrating kraken on a branch and running its test suite, which includes the chain-inspecting test at `main.rs:1022`.

#### 8.14.4 From eyre and color-eyre

`eyre::Report` maps to `Error`, `eyre::Result` to `Result`, `eyre!` to `err!`, `WrapErr::wrap_err` to `context`, `wrap_err_with` to `with_context`, and color-eyre's `Section` methods (`note`, `warning`, `suggestion`, and their `with_` forms) to the same names on `Context` and `Error`. eyre's `set_hook`/`EyreHandler` has no equivalent: formatting is fixed (7.8) and colour is L4's job; scope data at creation goes through L1's creation hook (7.16). color-eyre's `section` (custom headed sections) and `error` sections are covered by `note` and `attach_display`.

### 8.15 API count by grade

| Grade | Items |
|---|---|
| P0 | ROOT-01, ROOT-02, ROOT-03, ERR-01 to ERR-06, ERR-11, ERR-12, ERR-17, ERR-19, CTX-01 to CTX-03, MAC-01 to MAC-04, CHN-01, MAIN-01 (22) |
| P1 | ERR-07 to ERR-10, ERR-13, ERR-15, CTX-04 to CTX-06, CHN-02, CHN-03, ATT-01, ATT-02, ATT-05 to ATT-07, GRP-01, GRP-02, GRP-06, GRP-07, SUP-01, SUP-02, TRC-01 to TRC-04, CFG-01, CFG-02, CFG-04, MAIN-02, BND-01, BND-02, HOOK-01, HOOK-02, HOOK-04, STD-01 to STD-03 (38) |
| P2 | ROOT-04, ERR-14, ERR-16, ERR-18, CTX-07, CHN-04 to CHN-06, ATT-03, ATT-04, GRP-03 to GRP-05, SUP-03, SUP-04, TRC-05, CFG-03, CFG-05, MAIN-03, MAIN-04, BND-03, HOOK-03, HOOK-05, STD-04, TST-01 (25) |
| P3 | MAC-05, TST-02 (2) |

Every P0, P1, and P2 item ships by 1.0 (section 14). The two P3 items are after 1.0 and the design leaves room for both (the `ensure!` macro's arm order and the hook bridge).

---

## 9. Parity matrix

Versions checked on crates.io on 2026-10-22: anyhow 1.0.104 (rust-version 1.68), eyre 0.6.14 (1.65.0), color-eyre 0.6.5 (1.65.0), error-stack 0.8.0 (1.83.0), snafu 0.9.2 (1.65). "yes" and "no" below come from each crate's documentation or source as read on that date (section 18); "not checked" means the documentation read did not settle it, and the 0.6 parity suite settles it.

| Capability | anyhow | eyre + color-eyre | error-stack | snafu (`Whatever`, `Report`) | report-err |
|---|---|---|---|---|---|
| One type for any error, `?` from any std error | yes | yes | typed context per report (`Report<C>`), `change_context` to move between types | `Whatever` with `whatever_context`; typed errors otherwise | yes (ERR-02) |
| One machine word | yes (documented) | yes (`ErrorHeader` + payload behind one pointer, from source) | not checked | not checked | yes (FR-2, 7.4) |
| Context on `Result`, eager and lazy | yes | yes (`wrap_err`, `wrap_err_with`) | yes (`attach`, `change_context`) | yes | yes |
| Context on `Option` | yes | not checked | no (`ResultExt` and `FutureExt` only) | yes (`OptionExt`) | yes (CTX-02) |
| `bail!`, `ensure!`, ad-hoc macro | yes | yes | `bail!`, `ensure!` | `whatever!`, `ensure_whatever!` | yes (MAC-01 to MAC-03) |
| Downcast to root or context type | yes | yes | `downcast_ref` (stable) | typed errors are matched directly | yes (ERR-06 to ERR-09) |
| Cause chain iterator, root cause | yes | yes | `frames()` | `ChainCompat` iterator | yes (CHN-01, CHN-02) |
| Tree search across several causes | no | no | `contains`, `frames` over `Report<[C]>` | no | yes (CHN-03, CHN-04) |
| Error codes, severity, category | no | no | no (attachments can carry them) | no | yes, from L1 (ERR-11 to ERR-15) |
| Creation location | no | handler `track_caller` (stored by the handler) | locations appear in its output (capture rule not checked) | `Location` type in typed errors | yes, always (ERR-10) |
| Propagation trace | no | no | each `attach`/`change_context` is a frame with its location | no | yes (TRC-01, TRC-02) |
| Lazy backtraces | yes | yes | yes | yes (with features) | yes, root only (TRC-04) |
| Typed attachments read back by type, stable Rust | no | no | yes (`downcast_ref`; `request_ref` is nightly) | no | yes (ATT-01 to ATT-03) |
| Notes, warnings, suggestions | no | yes (`Section`) | printable attachments | no | yes (ATT-05) |
| Several causes | no | no | yes (`Report<[C]>`, `expand`, `push`, `append`) | no | yes (GRP-01 to GRP-07) |
| Collect every failure of an iterator | no | no | not checked | no | yes (GRP-07) |
| Suppressed errors | no | no | no | no | yes (SUP-01 to SUP-04) |
| `main` integration through `Termination` | yes (through `Result`) | yes (through `Result`) | yes (through `Result`) | `Report` implements `Termination`; `#[snafu::report]` | yes (MAIN-01, MAIN-02) |
| Exit code chosen from the error | no | no | no | only with the nightly provider API | yes (MAIN-02, MAIN-03) |
| Panic boundary | no | no (color-eyre installs a panic hook instead) | no | no | yes (BND-01 to BND-03) |
| Hook at creation | no | `set_hook` builds a handler for every report | `install_debug_hook` (formatting only) | no | through L1 (HOOK-01) |
| Handler or hook changes formatting | no | yes | yes | no | no: fixed rules (7.8); L4 renders |
| Report to a central pipeline, once | no | no | no | no | yes (HOOK-02 to HOOK-05) |
| Budgets on growth | no | no | no | no | yes (7.18) |
| Guarded rendering (panicking `Display`) | no | no | no | no | yes (FR-26) |
| `no_std` + `alloc` | yes (needs a global allocator) | not checked | yes (without `std` feature) | yes | yes (7.19) |
| Third-party runtime dependencies | 0 | eyre: not checked for 0.6.14; color-eyre: several | none required (anyhow, eyre, futures-core, pin-project-lite, serde_core, spin, tracing, tracing-error all optional) | `snafu-derive` (proc macro) | 0 (L1 is first-party) |

Ideas taken from outside Rust (the error collection plan 10): Zig error return traces (R11), Python `add_note` and `ExceptionGroup.split` (R13, R14), Go `errors.Join` (R14), .NET `AggregateException` (R14) and first-chance notifications (R25), Java `Throwable.addSuppressed` (R15), Go `recover` (R24).

---

## 10. Security

### 10.1 Threats, defences, tests, costs

| Id | Threat | Defence | Attack test | Cost |
|---|---|---|---|---|
| T1 | **Unbounded context growth**: a retry loop or a recursive walk adds a context layer on every pass until memory runs out | `max_frames` (default 64): keep the innermost 8 and the newest, drop the oldest of the middle, count them; `max_bytes` caps text | add 1,000,000 layers in a loop; memory stays under the documented bound, 64 layers kept, `omitted.frames == 999,936`, the report says so | one compare per context [est. under 1 ns] |
| T2 | **Attachment and tree memory**: large or many attachments, or `collect_all` over an endless stream of failures | `max_attachments`, `max_bytes` (shallow size of each boxed value), `max_tree`, `max_depth`; `Errors` keeps an exact count but a bounded list | `collect_all` over 10,000,000 failing items: 1,024 children stored, count exact, memory flat; 10,000 attachments of 1 KiB values: 32 kept | one compare per push |
| T3 | **Secrets in context**: a password or token formatted into a context string or attached, then printed in a log or a report | report-err cannot read intent, so: L1 `Secret<T>` prints `[redacted]` in `Display` and `Debug` (formatting it into a context is safe); attachments are opaque unless `attach_display` is chosen; context values are printed only through `Display`, never `Debug`; `Error`'s `Display` is documented as private (logs only) and `public_message()` is the only client-facing text | canary test: attach a `Secret("canary-7f3a")` and a struct holding the same text, add the secret to a context through `format!`; render `{}`, `{:#}`, `{:?}`, `{:#?}`, `ExitReport` output, `record()`, and what a test reporter receives; the canary text never appears | none |
| T4 | **Panic or failure in `Display` while reporting**: a user `Display` panics, returns `fmt::Error`, or never stops writing, inside `main`'s report, a `Drop`, or a panic hook (a second panic aborts the process and the report is lost) | guarded rendering (FR-26) for `{:?}`, `ExitReport`, `report()`, `record()`, `render()`, `unraisable()`: each layer is written through a byte-counting adapter (stops at `max_render`), wrapped in `catch_unwind` under `std` with unwinding, and a failure prints `<display panicked>` or `<display failed>` and continues. `{}` and `{:#}` are not guarded (cheap paths, like every other `Display`); documented | layers whose `Display` panics, returns `Err` without an underlying error, writes 100 MiB, or writes forever; each format and each guarded path; no panic escapes the guarded paths and output is bounded | per layer one `catch_unwind` frame (no cost unless a panic happens) and one counter add per write |
| T5 | **Cyclic or endless `source()` chains** from a buggy error type (returns itself, or generates a new link forever) | `max_chain` (default 128); no repeat check (D-020) | an error whose `source()` returns `self`; a generator of 200 distinct links; both stop at the budget, `{:#}` ends with `(chain truncated)` and `{:?}` prints it on its own line (`test_chain_self_source_stops_at_the_budget`, `test_chain_longer_than_budget_is_bounded`, `test_attack_cyclic_and_endless_chains_stop`) | one counter compare per link |
| T6 | **Terminal and log injection** through attacker-controlled text inside an error (a file name containing escape sequences, carriage returns, or a fake `Error:` line) | `ExitReport` and the `unraisable` fallback escape C0 controls except newline and tab, DEL, and C1 controls (U+0080 to U+009F) as `\u{..}` text, and indent continuation lines so a message cannot start a line that looks like a new report block; `Display` and `Debug` stay raw for programs, and L4 escapes for logs | messages containing `ESC ] 0 ; ... BEL`, `ESC [ 2 J`, `\r`, `\u{9b}`, and `"\nError: fake"`; stderr output contains none of them raw | one scan per byte written to stderr (cold path) |
| T7 | **Error floods**: an attacker triggers millions of errors per second to exhaust memory or CPU in error handling | each error is bounded (T1, T2); creation is one allocation with no lock; `report()` never blocks (one atomic flag and the L1 facade); flood control is L7's (storm control) | 64 threads each creating, enriching, reporting, and dropping 1,000,000 errors with and without a reporter installed; no growth, throughput recorded in `docs/PERFORMANCE.md` | none beyond the operations themselves |
| T8 | **Unsoundness in the `unsafe` core**: type confusion in a downcast, double free, use after free, misaligned access, aliasing violations | invariants I-1 to I-13 (7.3), each with a test; Miri under Stacked Borrows and Tree Borrows with strict provenance; AddressSanitizer job; operation-sequence fuzzing (12.6) | every row of 7.3's table; the fuzz target under ASan; Miri on the whole test suite that touches `raw` | none at run time |
| T9 | **Allocation failure while creating an error** (the system is already out of memory) | allocation failure aborts the process through `alloc::alloc::handle_alloc_error`, as everywhere in `alloc`; report-err keeps no emergency pool (it would be global state). Documented in `docs/PLATFORM-NOTES.md`; L7's crash path owns out-of-memory reporting | a test with a failing global allocator in a subprocess confirms the abort (no undefined behaviour, no hang) | none |
| T10 | **Panic in `Drop`** of a root error, attachment, or panic payload | a panicking `Drop` of user values propagates like any value's (Miri shows no undefined behaviour); panic payloads are dropped inside a second `catch_unwind` and leaked if that panics too (7.15) | payload whose `Drop` panics through `boundary`; root error whose `Drop` panics under Miri | none unless it happens |
| T11 | **Stack exhaustion** from deep trees, long chains, or recursive drop | every walk is iterative with an explicit stack bounded by `max_depth`; drop recursion depth is bounded by `max_depth` (16 by default) | build, render, search, and drop a full tree at `max_depth` on a thread with a 64 KiB stack | none |
| T12 | **Information disclosure** through reports sent to clients: build-machine file paths in locations, backtraces, private messages | `Display` and `Debug` are documented as private; `public_message()` and the category are the client-facing data (L4 problem details uses only those); `docs/SECURITY.md` shows `--remap-path-prefix` for paths in release builds | test that `public_message()` never contains the private message or a location; documentation check | none |
| T13 | **Creation hook misuse**: a hook that panics, recurses by creating errors, or is slow | thread-local reentrancy guard (`std`); `catch_unwind` around the hook (`std` + unwinding) with a `creation_hook = "panicked"` field; the hook contract (L1 docs) says microseconds; under `no_std` the contract forbids creating errors inside the hook | a hook that creates an error (no recursion, one call), a hook that panics (error creation succeeds, field present), a hook that sleeps (documented effect measured) | one acquire load when no hook is installed [est. under 1 ns] |
| T14 | **A security failure kept local**: a security-relevant error handled where it happens and never reported, so the rest of the system never learns of the attack | report-err has no "local only" mode; `report()` and `unraisable()` always go through the L1 facade, which L7 connects to dispatch-io and relay-io; errors with `Kinds::SECURITY` are reported like every other; the docs tell request and task boundaries to call `report()` | a test reporter receives a `SECURITY` error reported from a nested call | none |
| T15 | **Budget misconfiguration**: limits set so high that they no longer bound memory, or set to zero by mistake | `configure` validates every field against its range and refuses the whole `Limits` value (fail closed: defaults stay) with `OutOfRange` saying which field and the allowed range | each field at min-1, min, max, max+1 | at start only |
| T16 | **Unbounded report output**: an error with huge sections, many children, and long chains printed by `ExitReport` fills a log disk or a terminal | `max_render` (default 1 MiB) stops guarded rendering with `(report truncated)` | an error built at every budget's maximum; `ExitReport` output ≤ `max_render` + the truncation line | one counter add per write |

### 10.2 What report-err cannot defend against

- Secrets the caller formats into context text with plain `Display` (only `Secret<T>` and opaque attachments help); the documentation says so in its first security paragraph.
- A `Display` that panics under `panic = "abort"`: no guard can run; the process aborts.
- Memory owned by an attached value beyond its shallow size (an attached `Vec<u8>` of 1 GiB counts as 24 bytes); the caller decides what to attach. A 0.6 idea (10.3) narrows this for `String` and `Vec<u8>`.
- Errors that never become an `Error` (7.22).

### 10.3 Defence ideas per version

Recorded as hypothesis, method, and expected cost, as 6.4 requires; each version adds at least one to `dev/PLAN.md`.

| Version | Hypothesis | Method | Expected cost [est.] |
|---|---|---|---|
| 0.3 | escaping terminal controls in `ExitReport` stops report spoofing in terminals and CI logs | escape table (T6), golden tests | one scan of stderr output |
| 0.4 | opaque-by-default attachments stop most accidental leaks of attached data | `attach` never prints; `attach_display` is explicit | none |
| 0.5 | a reentrancy guard plus panic isolation keeps a broken creation hook from turning every error into a crash | thread-local flag, `catch_unwind` | one thread-local read per creation with a hook installed |
| 0.6 | counting the heap length of `String`, `Vec<u8>`, and `Box<str>` attachments makes `max_bytes` meaningful for the common large cases | `Any` checks at attach time | about 2 ns per attach |
| 0.7 | rendering under a 64 KiB thread stack finds any recursion left in formatting or drop | a test thread with a small stack in CI | none at run time |
| 0.8 | a canary scan over every rendering of every test error catches new leak paths | the T3 canary applied to the whole test suite through a test-only reporter | test time only |
| 0.9 | fuzzing the operation sequences under ASan finds memory errors Miri's interpreter is too slow to reach | the 12.6 targets built with `-Zsanitizer=address` | CI time only |

---

## 11. Performance

### 11.1 Targets against competitors

Measured with criterion at 0.6 against anyhow 1.0.104, eyre 0.6.14 (with its default handler), error-stack 0.8.0, and `Box<dyn Error + Send + Sync>` as the floor, on Linux and Windows, x86_64 and aarch64. Targets are [est.] until measured.

| Id | Benchmark | anyhow work | report-err work | Target |
|---|---|---|---|---|
| B1 | `size_of::<Result<(), Error>>()` | 8 bytes | 8 bytes | equal (exact) |
| B2 | success path through a 5-deep `?` chain | identical layout | identical layout | within 5% (measured equal in 7.4's probe) |
| B3 | create from `io::Error` through `?` (backtraces off) | 1 allocation, backtrace check | 1 allocation, backtrace check, location store | within 5% (allocation-bound) |
| B4 | `err!("literal")` | 1 allocation | 1 allocation | within 5% (allocation-bound) |
| B5 | `err!("value {x}")` | 2 allocations | 2 allocations; 1 with RD-14 | within 5%; at least 20% faster if RD-14 is adopted |
| B6 | `io_result.context("literal")?` | 1 allocation | 1 allocation | within 5% (allocation-bound) |
| B7 | `?` then `.context("literal")` on the error | 2 allocations | 1 allocation | at least 20% faster |
| B8 | create plus 3 literal contexts | 3 or 4 allocations | 2 allocations | at least 20% faster |
| B9 | drop of B8's error | 3 or 4 frees | 2 frees | at least 20% faster |
| B10 | `downcast_ref` to the root after 3 contexts | vtable walk through 3 nodes | one vtable call | at least 20% faster |
| B11 | `{}` of B8's error into `io::sink()` | outer node `Display` | outer frame `Display` | within 5% |
| B12 | `{:#}` of B8's error | walk 3 nodes and the root | walk 3 frames and the root | at least 20% faster [hypothesis: fewer pointer hops; may be within 5% if formatting dominates] |
| B13 | `{:?}` of B8's error (no backtrace) | as anyhow | more blocks (location, trace) | within 20% slower allowed and recorded: report-err prints more |
| B14 | create with backtraces enabled | capture at creation | capture at creation | within 5% (capture-bound, about 0.8 µs measured) |
| B15 | creation hook cost when none is installed | n/a | one acquire load | under 1 ns |
| B16 | `collect_all` over 10,000 items with 1% failures | n/a (`collect` stops at the first) | one pass | report alongside `Iterator::collect::<Result<Vec<_>, _>>()` |
| B17 | `boundary` with no panic | n/a | `catch_unwind` call | under 2 ns over a plain call |
| B18 | creation under contention: 64 threads creating and dropping | allocator-bound | allocator-bound | within 5%; recorded with the system allocator and with mimalloc (dev-dependency) |

Where a target says "within 5%" the standard's 20% bar cannot be met by design, because both libraries do exactly one allocation and the allocator dominates; that is recorded here as the reason the standard asks for. eyre does at least one more allocation per creation for its handler box (its `ErrorHeader` holds `Option<Box<dyn EyreHandler>>`, from source), so report-err is expected to be at least 20% faster than eyre on B3 to B8 [hypothesis, measured at 0.6].

### 11.2 Footprint

| Measure | Target [est.] |
|---|---|
| bytes on the stack per `Error` | 8 (FR-2) |
| bytes on the heap for a plain error | 80-byte header + payload (72 before D-003) |
| bytes on the heap for an error with 2 to 5 literal contexts | header + payload + `Ext` (about 400) |
| compile time added (clean, release, Tier 1 only) | under 1.5 s on James's desktop |
| binary size added (release, `strip = "symbols"`, Tier 1 only) | under 60 KiB |
| binary size added per distinct error type converted with `?` | one vtable (6 function pointers) and its functions [est. under 1 KiB] |

### 11.3 Method

- criterion benchmarks in `benches/` (one file per group: `create.rs`, `context.rs`, `downcast.rs`, `format.rs`, `collect.rs`, `contention.rs`), competitors as dev-dependencies, the same operations written idiomatically in each library.
- Every benchmark that claims an allocation count has a twin counting-allocator test, so a benchmark win cannot come from a missed allocation.
- Runs on Windows 11 (James's desktop), WSL 2 Ubuntu on the same machine, a Linux server, and an Apple silicon Mac; `docs/PERFORMANCE.md` records CPU, OS, allocator, rustc version, and the criterion baseline for each.
- The success-path claim (B2) is also checked by reading the generated assembly of a 5-deep chain in both libraries (`cargo asm` or `--emit asm`), recorded in `docs/PERFORMANCE.md`.
- A regression over 5% against the stored baseline blocks the change (6.2).

### 11.4 Measured so far

**0.3.0, the crate against anyhow 1.0.104** (criterion 0.8, 2026-10-29; rustc 1.99.0, `opt-level = 3`, fat LTO, one codegen unit; Windows 11 Pro, AMD Ryzen 9 9950X3D, 128 GiB, Windows system allocator; backtraces off; the machine was shared with other builds; point estimates; `benches/benches/core.rs`):

| Id | Benchmark | report-err | anyhow | Result |
|---|---|---|---|---|
| B2 | success path, 5-deep `?` chain | 4.96 ns | 4.85 ns | equal (noise) |
| B3 | create from `io::Error` through `?` | 22.1 ns | 26.8 ns | 17% faster |
| B4 | `err!("literal")` | 22.7 ns | 26.6 ns | 15% faster |
| B5 | `err!("value {x}")` | 55.5 ns | 61.0 ns | 9% faster |
| B6 | `io_result.context("literal")?` | 26.8 ns | 29.8 ns | 10% faster |
| B7 | `?` then `.context("literal")` | 26.7 ns | 52.3 ns | 1.96 times faster |
| B8 | create plus 3 / 5 / 9 literal contexts | 65.6 / 72.2 / 125.5 ns | 110.5 / 154.2 / 409.1 ns | 1.7 / 2.1 / 3.3 times faster |
| B9 | drop of an error with 3 contexts | 26.3 ns | 51.6 ns | 1.96 times faster |
| B10 | `downcast_ref` to the root after 3 contexts | 1.01 ns | 2.47 ns | 2.4 times faster |
| B11 | `{}` of B8's error | 7.52 ns | 8.32 ns | 10% faster |
| B12 | `{:#}` of B8's error | 54.7 ns | 27.3 ns | **2.0 times slower**: the de-duplication rule formats the root once into a buffer to compare it with the layer before it; anyhow does no de-duplication. Recorded as an exception (D-022) and revisited at 0.6 |
| B13 | `{:?}` of B8's error | 106.8 ns | 92.4 ns | 16% slower, within the 20% allowed: report-err also prints `Location:` and runs the de-duplication pass |
| - | hand-written `source()` walk, 1 / 3 / 5 / 9 layers | 2.93 / 5.64 / 8.07 / 13.69 ns | 2.98 / 6.35 / 9.85 / 16.78 ns | level, then 11% to 18% faster |
| - | `chain().count()`, 1 / 3 / 5 / 9 layers | about 3.4 / 6.6 / 8.5 / 9.3 ns | about 3.5 / 7.6 / 10.9 / 18.4 ns | equal to 2 times faster |

B3 to B6 are each one allocation in both libraries; report-err's lead there comes from anyhow's backtrace-status check on every creation (report-err checks only when backtraces arrive, in 0.4). The 0.6 milestone re-measures all of them on Linux and aarch64 against eyre and error-stack as well (11.1).

**Before any code (the design measurements of 7.4 and 7.6.1):**

Measured on 2026-10-22 while writing this plan, with a stand-alone probe (no crates), `rustc 1.95.0 -C opt-level=3 -C codegen-units=1`, Windows 11 Pro, AMD Ryzen 9 9950X3D, 128 GiB, Windows system allocator. Best of 7 runs after warm-up.

| Measurement | Result |
|---|---|
| `size_of`: thin pointer / `Box<dyn Error + Send + Sync>` | 8 / 16 |
| `size_of::<Result<(), _>>()`: thin / fat | 8 / 16 |
| `size_of::<Option<_>>()`: thin / fat | 8 / 16 |
| `size_of::<Result<u32, _>>()`, `Result<u64, _>`, `Result<[u64; 2], _>`, `Result<String, _>`: thin / fat | 16/16, 16/16, 24/24, 24/24 |
| `size_of::<Option<std::backtrace::Backtrace>>()` | 48 |
| 7.2 header with the exact field types (before the node pointer of D-003) | 72 |
| 5-deep `?` chain, success path: unit / u32 / u64 / [u64; 2], thin vs fat | 3.99 vs 3.87; 4.32 vs 4.16; 3.79 vs 4.14; 4.11 vs 3.92 ns (noise level) |
| one allocation and free of a small boxed error | 21.2 ns |
| two allocations and frees (`Box<Box<dyn Error>>`, or `String` + box) | 46.5 to 51.2 ns |
| `Backtrace::capture()` with the variables unset | 1.8 ns |
| `Backtrace::force_capture()` (unresolved) | 818 ns |
| `Backtrace::force_capture()` plus formatting (resolution) | 357 µs |

---

## 12. Testing plan

### 12.1 Unit tests

At the bottom of every file, named `test_<subject>_<condition>_<expected>`, covering every function and every branch. `raw.rs` tests cover each invariant of 7.3 and run under Miri in CI.

### 12.2 Integration tests (`tests/`)

| File | Covers |
|---|---|
| `tests/tier1.rs` | every example in 8.2, compiled and run |
| `tests/context.rs` | 8.5 on `Result<T, E>`, `Result<T, Error>`, `Option<T>` |
| `tests/macros.rs` | every arm of 8.6, including inline captures and code-first arms |
| `tests/chain.rs` | 7.7, 8.7 |
| `tests/format.rs` | 7.8 golden files for `{}`, `{:#}`, `{:?}`, `{:#?}` |
| `tests/codes.rs` | 7.17 with a test catalog of L1 codes |
| `tests/attachments.rs`, `tests/sections.rs` | 8.8 |
| `tests/groups.rs` | 8.9 |
| `tests/suppressed.rs` | 8.10 |
| `tests/trace.rs` | 7.9, 8.11 |
| `tests/backtrace.rs` | 7.13 (environment cases run as subprocesses) |
| `tests/limits.rs` | every budget of 7.18 at n-1, n, n+1 |
| `tests/exit.rs` | `ExitReport` exit codes and output, through `examples/exit_*.rs` run as subprocesses |
| `tests/boundary.rs` | 7.15 |
| `tests/hook.rs` | 7.16 with a test creation hook and a test reporter installed through L1 |
| `tests/std_interop.rs` | `into_std`, `from_boxed`, round trips |
| `tests/no_std.rs` | the 7.19 `no_std` column, run with `--no-default-features` |
| `tests/parity_anyhow.rs` | 12.7 |
| `tests/attacks.rs` | T1 to T16 |
| `tests/ui/*.rs` | trybuild compile-fail cases (12.11) |

### 12.3 Property tests (proptest)

- **Never panics**: any sequence of up to 200 operations (create, context, with_context, coded, attach, note, trace, group, push_child, suppress, split, flatten, downcast, render in every format, report, record, drop) on errors with arbitrary text never panics outside caller code.
- **Never exceeds a budget**: after any sequence, every counted size is within its budget and every omitted counter equals what was dropped.
- **Formatting**: `{}` equals the newest layer's text (or the root's when there is none); `{:#}` equals the de-duplicated join of `chain()`; `{:?}` begins with `{}`.
- **Round trip**: `from_boxed(error.into_std())` prints the same `{:#}` as `error`.
- **Downcast**: after any sequence, `downcast_ref::<Root>()` succeeds exactly when the root type was `Root`.
- **`collect_all`**: equals `collect::<Result<Vec<_>, _>>()` when there are no failures; otherwise the child count equals `min(failures, budget)` and the message's counts are exact.
- **`split`**: the children of the two halves are a partition of the original children, in order.
- **Trace**: the trail is in creation order and holds `min(points, 15)` trace points plus every frame.

### 12.4 Edge cases

**Construction**

- E-1 zero-sized root error.
- E-2 root error aligned to 64 bytes. E-3 aligned to 4096 bytes.
- E-4 root error of 1 MiB (large inline payload).
- E-5 root error with a counting `Drop`: dropped exactly once on drop, never on `downcast` by value.
- E-6 root error whose `Drop` panics (unwind builds; Miri).
- E-7 root error whose `Display` is the empty string.
- E-8 root error whose `Display` panics (guarded and unguarded paths).
- E-9 root error whose `Display` returns `fmt::Error`.
- E-10 root error whose `Display` writes 100 MiB.
- E-11 messages containing NUL, ESC, CR, a 1 MiB single line, and every Unicode plane.
- E-12 `err!("{{}}")` (escaped braces, no arguments) prints `{}`.
- E-13 `err!(io_error)`: the root is downcastable as `io::Error` (error arm chosen, not message arm).
- E-14 `err!(a_string)`: a message root, downcastable as `String`.
- E-15 `from_boxed` of a boxed concrete error: downcast to the concrete type works.
- E-16 `from_boxed(error.into_std())` does not double-wrap (the error is recovered).
- E-17 creation on 64 threads at once with a creation hook installed.
- E-18 a `!Send` error, a `!Sync` error, and a non-`'static` error do not compile (trybuild).
- E-19 `?` on `Result<_, Box<dyn Error + Send + Sync>>` does not compile (as anyhow; the error message points to `from_boxed`, through `#[diagnostic::on_unimplemented]`).

**Context**

- E-20 `with_context` on `Ok` and `Some`: the closure is never called (counter).
- E-21 `context` on `None`: the root is the context message.
- E-22 empty context text.
- E-23 1 layer (header slot), 2 layers (`Ext` allocated), 5 layers (inline frames spill to the `Vec`).
- E-24 `max_frames - 1`, `max_frames`, `max_frames + 1` layers.
- E-25 1,000,000 layers (T1).
- E-26 a `String` context larger than the remaining `max_bytes`, cut at a multi-byte character boundary.
- E-27 context values of type `&'static str`, `String`, and a custom struct: each stored in the right variant (allocation counts) and each downcastable.
- E-28 downcast to a context type after more layers are added on top.
- E-29 a `with_context` closure that panics: no leak (Miri), the panic propagates.
- E-30 a context value whose `Display` panics, in `{:?}` (guarded) and `{}` (propagates, documented).

**Macros**

- E-31 `bail!("literal")` with no arguments: one allocation.
- E-32 inline captures `{name}`, positional, named, and width arguments.
- E-33 `ensure!(cond)` where the condition contains string literals with braces: the text is printed literally, never treated as a format string.
- E-34 `ensure!` with a multi-line condition: whitespace normalised by `stringify!`, documented.
- E-35 code-first arm with an expression that is not a `Code`: a compile error with the `on_unimplemented` note (trybuild).
- E-36 `bail!` inside a closure with no annotated return type infers `Result<_, Error>`.
- E-37 macros used from a `no_std` crate.

**Downcast and chain**

- E-38 `downcast_ref` to a wrong type: `None`.
- E-39 `downcast` by value: hit frees exactly once; miss returns the same error (same address).
- E-40 `downcast_mut` on the root: the change shows in `Display`.
- E-41 `downcast_ref::<&str>()` after `err!("literal")`; `downcast_ref::<String>()` after `err!("{x}")`.
- E-42 `is::<C>()` for a context type.
- E-43 `chain()` with no layers: root, then its sources.
- E-44 `chain()` with a `source()` that returns itself: stops (T5).
- E-45 a chain longer than `max_chain`.
- E-46 `root_cause()` with no sources is the root.
- E-47 `find` across children at depth 16 (found) and 17 (refused at insertion).
- E-48 `find` does not search suppressed errors; `find_in_suppressed` does.
- E-49 `chain().rev()` equals the forward chain reversed, also at `max_chain`.
- E-49a `(&*error).source()` walked by hand gives the same links, in the same order, as `chain()`: with 0, 1, 2, 5 layers, at `max_frames`, after a spill from the inline slots to the `Vec`, and after the `max_frames` policy dropped a middle layer (D-003).
- E-49b `&*error` and `error.as_ref()` passed to a function taking `&(dyn std::error::Error + 'static)` and to one taking `&(dyn Error + Send + Sync + 'static)`; `Box<dyn Error + Send + Sync>` from `into_std()` walked the same way.
- E-49c `error.source()` called through auto-deref on an `Error` with no context returns the root's own source.
- E-49d `Display` of `&*error` equals `{}` of the error; `Debug` of `&*error` names the layer without printing the report.

**Codes**

- E-50 no code anywhere: `None`, severity 17, category `Unknown`.
- E-51 `coded` twice: the latest wins.
- E-52 a coded error deep in the root's sources is found.
- E-53 two coded errors in the chain: the innermost wins.
- E-54 an explicit code beats the chain.
- E-55 L1's `identify` answers `None` for every link (no generated errors in the chain): no panic, `None`.
- E-56 `Option::coded` on `None`: the message is the catalog title; with no catalog configured, or a code missing from it, the message is the code itself.

**Formatting**

- E-57 `{}` with no layers prints the root.
- E-58 de-duplication on an exact `": "` suffix; E-59 no de-duplication on a prefix match; E-60 de-duplication of identical adjacent layers.
- E-61 multi-line messages are indented under their block in `{:?}`.
- E-62 `{:?}` with every block present (golden file); E-63 `{:#?}` (golden file).
- E-64 width, fill, and precision flags (`{:>40}`) are ignored except `#`, as documented.
- E-65 formatting the same error from 8 threads while another thread calls `report()`.

**Trace and backtrace**

- E-66 0, 4, 5, 15, 16, and 1,000 trace points.
- E-67 the omitted counter saturates at `u32::MAX` (forced in a unit test).
- E-68 the trail merges contexts and trace points in creation order.
- E-69 variables unset: no backtrace. E-70 `RUST_BACKTRACE=1` with `RUST_LIB_BACKTRACE=0`: no error backtrace (subprocess).
- E-71 `with_backtrace` forces one; calling it twice keeps the first.
- E-72 a platform where capture is unsupported: `None`, `{:?}` has no backtrace block.
- E-73 context never captures a backtrace (allocation count with backtraces enabled).

**Attachments and sections**

- E-74 the same type attached twice: `attachment` gives the newest, `attachments` gives both.
- E-75 a zero-sized attachment.
- E-76 `max_attachments + 1`: refused and counted.
- E-77 an opaque attachment never appears in any rendering (canary).
- E-78 `attach_display` appears under `Attachments:`.
- E-79 section text of 0 bytes, exactly the remaining `max_bytes`, and one byte more (cut at a character boundary).
- E-80 `with_note` on `Ok`: the closure is never called.

**Several causes**

- E-81 `group` with no children; `Errors::finish` with nothing pushed is `Ok(())`.
- E-82 a group with one child.
- E-83 `collect_all` over an empty iterator: `Ok` of an empty collection.
- E-84 `collect_all` over 10,000,000 failing items (T2).
- E-85 `collect_all` over an iterator that panics halfway: the panic propagates, nothing leaks.
- E-86 `split` with a predicate that is always true, and always false.
- E-87 `flatten` of a tree at `max_depth`.
- E-88 adding a child whose own tree would exceed the remaining `max_tree`: refused as a whole and counted.
- E-89 an error cannot become its own child (ownership forbids it; a compile-fail case documents it).

**Suppressed and cleanup**

- E-90 `with_cleanup`: both succeed; main fails; cleanup fails; both fail.
- E-91 a cleanup closure that panics: the panic propagates and the main error is dropped normally (Miri).
- E-92 `unraisable` with a reporter, without one, and with standard error closed.

**`main`, boundary, hook, report, configuration**

- E-93 `ExitReport` exit codes: `Ok` 0; plain error 1; coded category per L1's table; panic 101; `with_exit_code(0)` becomes 1; `with_exit_code(255)` is 255.
- E-94 `ExitReport` with standard error closed and with a broken pipe: no panic, the exit code is still right.
- E-95 `boundary`: no panic; `&'static str` payload; `String` payload; custom payload; payload whose `Drop` panics; nested boundaries; a `boundary` call inside a `Drop` during unwinding.
- E-96 under `panic = "abort"`: the subprocess aborts with the platform's abort status and `PANICS_CAUGHT` is `false` (12.8).
- E-97 `report()` from 64 threads on the same error (in an `Arc`): exactly one `true`.
- E-98 `report()` with no reporter installed: `true` once, nothing printed.
- E-99 creation hook that creates an error; that panics; that is slow; installed while other threads are creating errors.
- E-100 `configure` twice; each field at min-1, min, max, max+1; two threads racing (`loom`).
- E-101 `record()` with every field at its budget: within L1's caps.
- E-102 `into_std()` then walking `source()` gives the same text as `chain()`.
- E-103 an `Error` in an `Arc` read from 16 threads at once.
- E-104 dropping a full tree at `max_depth` on a thread with a 64 KiB stack.
- E-105 `run(app)` where `app` returns `Ok`, `Err`, and panics.
- E-106 `Error::from_panic` on a `JoinHandle::join` error from a real panicking thread.
- E-107 every public function called on an error created by every constructor (a matrix test).

### 12.5 Counting-allocator tests

One test per row of 7.5, asserting the exact number of allocations and frees, plus: `with_context` on `Ok` allocates nothing; `?` of an existing `Error` allocates nothing; `{}` and `{:#}` allocate nothing; `report()` with no reporter allocates nothing; `downcast_ref` and `chain()` allocate nothing; `configure` allocates once.

### 12.6 Fuzz targets (cargo-fuzz)

| Target | Input drives |
|---|---|
| `ops` | a sequence of operations from 12.3 with arbitrary text, sizes, and budgets; asserts the property-test invariants after every step |
| `display_adversary` | root errors, context values, and sections whose `Display` panics, errors, writes huge output, or writes control characters at fuzzer-chosen points; renders through every guarded path |
| `limits` | arbitrary `Limits` values through `configure` (in a fresh process per input via the fuzzer's fork mode) and arbitrary operation sequences under them |

Each target runs 24 hours before 0.7 with no open finding; the corpus lives outside the repository. `ops` also runs under ASan (10.3, 0.9).

### 12.7 anyhow parity suite

`tests/parity_anyhow.rs` writes the same 60 small programs twice, once against anyhow (dev-dependency) and once against report-err, and asserts equal results for: `{}` and `{:#}` text, `chain()` texts, `root_cause()` text, `downcast_ref` and `is` outcomes for root and context types, and `Option` context. Differences listed in 8.14.2 are asserted as differences, so a change in either library shows up.

### 12.8 Subprocess tests

- `examples/exit_*.rs` binaries cover each exit-code row of E-93; `tests/exit.rs` runs them and checks status and stderr.
- `tests/abort/` is a separate small binary crate built with `panic = "abort"` (Cargo ignores `panic` for the test profile, so the abort case needs its own binary); `tests/boundary.rs` builds and runs it and checks the abort status and `PANICS_CAUGHT == false`.
- Backtrace environment cases (E-69, E-70) run the test binary itself as a child with the variables set.

### 12.9 Miri, loom, sanitizers

- Miri: `cargo +nightly miri test` on `raw` unit tests and every integration test that does not spawn processes, with `-Zmiri-strict-provenance`, once with default Stacked Borrows and once with `-Zmiri-tree-borrows`.
- loom: the `reported` flag (E-97) and `configure` (E-100).
- AddressSanitizer: the full test suite and the `ops` fuzz target on Linux nightly.

### 12.10 Sustained load and soak

8 hours, 64 threads, a mix of every operation, with and without a creation hook and a reporter installed; resident memory sampled every minute must be flat after warm-up (no growth beyond 1%); the run is repeated on Windows and Linux before 0.7 exits.

### 12.11 Compile-fail tests (trybuild)

`!Send` and `!Sync` errors (E-18); `Box<dyn Error>` through `?` (E-19); a non-`Code` first argument to the code arms (E-35); implementing `Context` outside the crate (sealed); an error as its own child (E-89).

### 12.12 Requirement-to-test map

Kept in `docs/STABILITY-1.0.md` from 0.9: every FR and NFR id with the names of the tests that prove it. The definition of done (section 16) requires every row to be filled.

---

## 13. Documentation deliverables

| Document | Content | First version |
|---|---|---|
| `README.md` | from the README template: header image, badges (MSRV included), pitch ("dynamic errors for applications: the anyhow you know, plus codes, traces, attachments, and exit codes"), features, install, quick start with Tier 1 only (8.2), a migration teaser (the 8.14.1 table), the `panic = "abort"` note beside `boundary`, status, documentation links, license, footer | 0.1, rewritten at 0.3 and 0.5 |
| `docs/API.md` | metrics-lib format: nested table of contents, every public item with signature, description, parameters, errors, cost, and several examples per use case; the version each item arrives in | 0.2 (design lock), updated every version |
| `docs/MIGRATING.md` | 8.14 in full: rules, the sed pass, differences, the kraken worked example, eyre and color-eyre, snafu's `Whatever` | 0.3, kraken result added at 0.8 |
| `docs/UNSAFE.md` | the `raw` module: layout, vtable, invariants I-1 to I-13 with the test for each, Miri commands, what changed in each version | 0.3 |
| `docs/ARCHITECTURE.md` | 7.1 to 7.7 and 7.16 with diagrams: layout, flat frames, the extension block, walks, hooks | 0.5 |
| `docs/PERFORMANCE.md` | 11.1 to 11.4 measured: numbers, hardware, method, allocators, assembly notes, footprint | 0.6 |
| `docs/SECURITY.md` | section 10: each threat, its defence, its test, its cost; 10.2 limits; the secret-handling guidance with `Secret<T>`; `--remap-path-prefix` | 0.7 |
| `docs/PLATFORM-NOTES.md` | backtrace support per platform, `panic = "abort"` behaviour, out-of-memory behaviour, `no_std` targets and the atomic requirement | 0.5 |
| `docs/STABILITY-1.0.md` | the 1.0 audit, the requirement-to-test map (12.12), the 1.x promises: API frozen, `{}` and `{:#}` text stable, `{:?}` layout changes only in minor releases with a note, MSRV policy | 0.9 drafted, 1.0 final |
| `CHANGELOG.md` | Keep a Changelog 1.1.0, updated in the same change as the code | every version |
| `docs/release/vX.Y.Z.md` | release notes in past tense from the release-notes template | every version |
| rustdoc | every public item with a runnable example; crate docs open with Tier 1 (8.2) and the formatting rules (7.8); `doc(cfg)` badges on `std`-only items | every version |
| `examples/` | `quick_start.rs`, `context.rs`, `codes.rs`, `exit_report.rs` and the `exit_*.rs` set (12.8), `attachments.rs`, `validation.rs` (`Errors`, `collect_all`), `cleanup.rs` (`with_cleanup`, `suppress`), `boundary.rs`, `migrate_from_anyhow.rs` | grows per version |

The writing rules of 6.12 apply to every file: plain and specific, no em dash, no emoji, none of the four banned marketing words, no `.dev/` reference.

---

## 14. Version roadmap

Every version ends with the release routine (section 15). The anti-deferral rule applies: an item moves to a later version only with a decision-log entry giving the reason. P0 items ship in the version listed and never move.

### 0.1.0 - Scaffold

- Repository `jamesgober/report-err` created and cloned to `error-collection/report-err`, portfolio layout (6.13), `REPS.md` byte-identical.
- `Cargo.toml`: edition 2021, MSRV 1.81 (RD-9), features `std` (default) and `testing` declared, no dependencies yet.
- `src/lib.rs`: crate docs, lint attributes (6.9, 6.13: `no_std`, `deny(unsafe_code)`, `deny(missing_docs)`), `VERSION` (ROOT-01), one test.
- `dev/PLAN.md` (this plan, with James's two decisions applied: the type is `Error`, D-002, and every layer is a real `source()` link, D-003; open decisions in section 17, decisions in section 20), `dev/DIRECTIVES.md`, `dev/ROADMAP.md`.
- CI: Linux, macOS, Windows × stable and 1.81, x86_64 and aarch64; security job; `no_std` job; Miri and loom jobs present (running the one test).

**Exit:** CI green on every job; the crate published to claim the name (RD-1 decided: D-001).

### 0.2.0 - Design lock

- `docs/API.md` for every item in section 8, with examples that compile as doc tests against the 0.3 code as it lands (the published 0.2 crate exposes only what is implemented; `docs/API.md` marks the version of each item).
- `docs/UNSAFE.md` first version: layout and invariants I-1 to I-13.
- The L1 interface X-1 to X-12 (3.4) agreed with L1's plan and recorded in both plans; L1 at its own 0.2 design lock.
- Every open decision in section 17 answered by James or accepted as recommended, recorded in the decision log. RD-3 (one word) must be settled here, because it decides whether `raw.rs` exists.
- The simplicity review of 6.10: every Tier 1 use in 8.2 is one or two lines.
- The chain view of 7.6.1 (D-003) prototyped with the real `raw` layout, Miri-clean under both aliasing models with strict provenance, and its cost measured (frame and header sizes, `source()` per layer, creation and context with and without the node pointer, allocation counts unchanged); the numbers replace the [est.] marks in 7.2, 7.6.1, and 11.2. If one-word size or an allocation count of 7.5 would have to change, James gets the numbers and the options before the lock.

**Exit:** James approves `docs/API.md`; the L1 interface is locked.

### 0.3.0 - Core: anyhow parity

API: ROOT-02, ROOT-03 · ERR-01 to ERR-10, ERR-17 · CTX-01, CTX-02 · MAC-01 to MAC-03 · CHN-01, CHN-02 · MAIN-01 · STD-01 to STD-03.

- `src/raw.rs` with every invariant test; Miri clean under both aliasing models; ASan job active.
- Flat frames with the header slot (7.2, 7.6); formatting rules (7.8) for the blocks that exist at 0.3 (message, causes, location); de-duplication rule.
- Counting-allocator tests for every 7.5 row that exists at 0.3; the anyhow parity suite (12.7) for the 0.3 surface; the `no_std` build and tests.
- `ExitReport`'s terminal escaping is not in 0.3 (no `ExitReport` yet); `{:?}` through std's `Termination` prints raw text, as anyhow does.
- No L1 dependency yet: 0.3 has no codes, so it does not wait for L1's publication.

**Exit:** a kraken branch migrated by the 8.14.1 pass compiles and passes its tests (trial; the merge happens at 0.8); edge cases E-1 to E-49 and E-57 to E-65 that apply to the 0.3 surface pass.

### 0.4.0 - Extended: codes, sections, attachments, trace, exit codes

API: ROOT-04 · ERR-11 to ERR-15, ERR-18 · CTX-03 to CTX-06 · MAC-04 · ATT-01 to ATT-07 · TRC-01 to TRC-05 · CFG-05 · MAIN-02, MAIN-03 · HOOK-05.

- L1 dependency added by published version (L1 at 0.3 or later, interface locked at its 0.2).
- Code lookup through `identify` and `innermost_code` (7.7, 7.17), catalogs (CFG-05), `from_info` (ERR-18, HOOK-05); `ExitReport` with exit-code selection and terminal escaping (7.14, T6); backtraces (7.13); trace and trail (7.9); attachments and sections (7.10); the remaining `{:?}` blocks.
- Edge cases E-50 to E-56, E-66 to E-80, E-93, E-94.

**Exit:** a CLI (cli-forge or hive-exec, James's choice) builds against 0.4 with `fn main() -> ExitReport` and category exit codes.

### 0.5.0 - Feature complete

API: ERR-16 · CTX-07 · CHN-03 to CHN-06 · GRP-01 to GRP-07 · SUP-01 to SUP-04 · CFG-01 to CFG-04 · MAIN-04 · BND-01 to BND-03 · HOOK-01 to HOOK-04 · STD-04 · TST-01.

- Several causes and collections (7.11), suppressed errors (7.12), budgets and `configure` (7.18), guarded rendering complete (FR-26), panic boundary (7.15) and the abort subprocess test, creation hook and reporting through L1 (7.16).
- Every P0, P1, and P2 item implemented; edge cases E-81 to E-107.
- `docs/ARCHITECTURE.md`, `docs/PLATFORM-NOTES.md`.

**Exit:** every P0 to P2 item in section 8 implemented and tested on Linux, macOS, and Windows; L7's prototype can install a creation hook and receive reports.

### 0.6.0 - Optimisation

- Benchmarks B1 to B18 against anyhow, eyre, error-stack, and `Box<dyn Error>`, on Windows, Linux, and aarch64 (Apple silicon).
- RD-14 decided with numbers (one-allocation formatted messages, which would extend the `unsafe` core); the E-2 success-path finding re-measured on aarch64 and recorded.
- 10.3's 0.6 idea (heap length of string and byte attachments).
- `docs/PERFORMANCE.md` published; criterion baselines stored; the 5% regression gate active.

**Exit:** every 11.1 target measured; each met or its reason recorded and reviewed.

### 0.7.0 - Hardening

- Fuzzing: 24 hours per target (12.6) with no finding open.
- Attack tests T1 to T16 complete; `docs/SECURITY.md`.
- Soak: 8 hours on Windows and Linux (12.10), flat memory.
- Miri on every eligible test; ASan clean; the 64 KiB stack test (E-104).

**Exit:** no open finding; every threat has a passing attack test.

### 0.8.0 - Alpha

- kraken migrated for real: the 8.14.3 pass merged on a branch James reviews, kraken's full test suite green, `kraken-lsp`'s unused anyhow dependency removed.
- One more application migrated (a Hive DB tool or WASP boot code), using codes, `ExitReport`, and `report()`.
- API review against real use; changes made now, not after 1.0.

**Exit:** both migrations green; every surprise fixed or recorded in `docs/MIGRATING.md`.

### 0.9.0 - Beta and release candidates

- API frozen; `0.9.x` only fixes.
- Documentation complete; `docs/STABILITY-1.0.md` drafted with the requirement-to-test map (12.12).
- Release candidates `1.0.0-rc.N` as needed; ASan fuzzing (10.3).

**Exit:** two weeks without an API change; every document checked against the build.

### 1.0.0 - Stable

- Full audit (API, docs, tests, benchmarks, security, the `unsafe` core line by line) recorded in `docs/STABILITY-1.0.md`.
- API frozen until 2.0; MSRV raise is a major change.

**Exit:** the audit passes; the definition of done (section 16) holds.

### After 1.0 (P3, design room already left)

| Version | Items |
|---|---|
| 1.1 | MAC-05 (`ensure!` operand values), TST-02 (`CountingHook`) |
| later | a `provide` bridge behind a non-default feature once `Error::provide` is stable in Rust (codes and attachments offered to generic consumers) |

---

## 15. Release routine

Copied from the library standard section 3. James's instruction (2026-10-21): after each version is complete, push to GitHub, tag, release, and once CI passes, `cargo doc` and `cargo publish`. In order:

1. **Finish the version**: every item of the version's roadmap entry done; version bumped in every place it appears (`Cargo.toml`, README install snippet, `docs/API.md`, rustdoc examples, `clippy.toml` MSRV if changed); `CHANGELOG.md` rolled (`[Unreleased]` becomes `[X.Y.Z] - YYYY-MM-DD`, compare links updated); `docs/release/vX.Y.Z.md` written; README and `docs/API.md` checked against the build.
2. **Local gate** (6.13) passes on Windows and WSL, stable and MSRV.
3. **Commit and push** to GitHub (the first scaffold to `main`; later versions on a `feature/` branch with a pull request, squash-merged after CI passes).
4. **Wait for CI to pass** on every platform (`gh run list`, `gh run view --log-failed`). If it fails, fix it in place; never cut a follow-up patch release for a red version.
5. **Tag** `vX.Y.Z` and push the tag.
6. **Release**: `gh release create vX.Y.Z --title "vX.Y.Z - <Title>" --notes-file docs/release/vX.Y.Z.md`, marked pre-release for 0.x.
7. **Docs**: `cargo doc --no-deps` with default and with all features under `RUSTDOCFLAGS=-D warnings`: no warnings.
8. **Publish**: `cargo publish --dry-run`, then `cargo publish`.
9. **Verify**: the version is on crates.io and the docs.rs build succeeded.

1.0.0 only after a full audit (API, docs, tests, benchmarks, security) recorded in `docs/STABILITY-1.0.md`.

report-err specifics: the local gate also runs Miri on the `raw` tests whenever `src/raw.rs` changed in the version; `cargo doc` is also run with `--no-default-features` (the `no_std` surface), because a doc link to a `std`-only item breaks only in that mode; the L1 version required is checked to be published before step 3 (no path dependency, 6.8); `[package.metadata.docs.rs] all-features = true` with `doc(cfg)` badges.

---

## 16. Definition of done

report-err 1.0 is done when every line below is true and recorded in `docs/STABILITY-1.0.md`:

1. Every P0, P1, and P2 item in section 8 is implemented, documented with a runnable example, and tested on Linux, macOS, and Windows, stable and the MSRV (1.81, RD-9).
2. Every FR and NFR in section 5 has at least one named test that passes (12.12).
3. Every edge case E-1 to E-107 has a test that passes.
4. Every threat T1 to T16 has an attack test that passes; `docs/SECURITY.md` states each defence's cost.
5. Every fuzz target has at least 24 hours of fuzzing and no open finding.
6. Counting-allocator tests prove every allocation count in 7.5 and every "no allocation" claim in 12.5; the soak shows no growth over 8 hours.
7. Benchmarks published in `docs/PERFORMANCE.md`; each 11.1 target met, or its reviewed reason recorded.
8. Zero third-party runtime dependencies (`cargo tree`); `cargo deny check` and `cargo audit` clean.
9. `unsafe` only in `src/raw.rs`, every block with a `SAFETY:` comment, every invariant I-1 to I-13 tested, Miri clean under Stacked Borrows and Tree Borrows with strict provenance, ASan clean; or, if James chose RD-3 option B, `#![forbid(unsafe_code)]` at the crate root.
10. `size_of::<Result<(), Error>>() == size_of::<usize>()` asserted at compile time (option A).
11. The `no_std` + `alloc` build passes on `thumbv7em-none-eabihf` and the 7.19 `no_std` column passes its tests.
12. kraken migrated and green (0.8); the anyhow parity suite green.
13. CI green on Linux, macOS, Windows (x86_64 and aarch64), stable and the MSRV; Miri, loom, `no_std`, abort-subprocess, and security jobs green.
14. README, `docs/API.md`, and every document in section 13 complete and matching the build; no banned words, no U+2014, no emoji, no `.dev/` reference in public files.
15. The simplicity review (6.10) passed at the 1.0 audit: every Tier 1 use is one or two lines a newcomer can read.
16. The release routine (section 15) completed for 1.0.0, docs.rs build green.

---

## 17. Open decisions

Each decision has a recommendation. The recommendation stands unless James overrides it; every outcome goes into the decision log (section 20). RD-1, RD-2, RD-3, RD-4, and RD-18 were decided by James; RD-17 was answered by define-err; every other recommendation was accepted at the `0.2.0` design lock (D-018).

| Id | Decision | Options | Recommendation and reason |
|---|---|---|---|
| RD-1 | **Crate name** | James names every crate (error collection plan EC-1) | **Decided by James (D-001): `report-err`.** Checked free on crates.io and GitHub on 2026-10-22 before the repository was created. |
| RD-2 | **Name of the error type** | `Fault`; `Error` (anyhow's name); `Report` (eyre, error-stack) | **Decided by James (D-002): `Error`.** The original recommendation was `Fault` (no confusion with the `std::error::Error` trait, coexistence with anyhow without aliasing); James chose `Error` so that migrating from anyhow is a change of crate name. The coexistence case is served by a rename on import (`use report_err::Error as Report;`), which anyhow users already know. |
| RD-3 | **Representation (ERROR-SUITE E-2)** | A: one word, audited `unsafe` core; B: two words, `forbid(unsafe_code)`; C: one word, safe, two allocations | **Decided by James (D-016): A**, as recommended in 7.4: the one-word layout, with every `unsafe` line in `src/raw.rs`. B remains the fallback before 1.0 if the Miri job or the 1.0 audit finds a problem that cannot be fixed. |
| RD-4 | **Flat context storage vs nested nodes** | flat (header slot + extension block); nested like anyhow (each context a node owning the inner error) | **Flat, with a real `source()` chain (D-003).** Flat storage keeps fewer allocations for 2 or more contexts (7.5), simple budget counting, and a cheaper drop. The original design gave up `Deref<Target = dyn Error>` for it; James required the chain without the trade-off, and 7.6.1 provides it: each frame carries its node pointer and finds the layer below it by address. The cost (8 bytes per frame, no allocation) is measured at the 0.2 design lock. |
| RD-5 | **How a code is read from a `&dyn Error` on stable Rust (X-6)** | (a) L1's identity probe through the deprecated `Error::description()`; (b) a registry of downcast probes; (c) explicit codes only; (d) nightly `provide` | **Settled by L1 (OD-5: the probe).** report-err uses `identify` and `innermost_code` and also ships (c) (`coded`, code-first macros) and `from_info` for per-value metadata the probe cannot carry. If L1 ever removes the probe, `code()` finds only explicit codes and the docs say so. |
| RD-6 | **`boundary` closure bound** | `UnwindSafe` (std's choice); take any `FnOnce` and wrap it in `AssertUnwindSafe` internally | **`UnwindSafe`.** The caller who wraps a closure in `AssertUnwindSafe` makes the claim knowingly; hiding it would let broken state be observed silently. The README example shows the wrapper. |
| RD-7 | **R3's `.ok_or_code(Code)` name** | `ok_or_code` on `Option` only; `coded` on both `Result` and `Option` | **`coded` on both.** One verb with one meaning ("this result, coded as X") is simpler to learn than two (6.10). |
| RD-8 | **Configuring budgets** | write-once global `configure(Limits)`; per-error limits only; compile-time constants only | **Write-once global plus the defaults**, the `set_logger` pattern already used by L1's facade: one acquire load to read, no mutation after start, `AlreadyConfigured` on a second call. Per-error limits would make every builder call heavier; constants would break 2.16 ("every limit configurable"). |
| RD-9 | **MSRV and edition** | 1.81 with edition 2021 (L1's choice, L1 OD-2); 1.85 with edition 2024 (portfolio default) | **Follow L1: 1.81 / 2021.** L1 caps every error crate's MSRV; kraken, the first migration, is at 1.80 and would move one minor version instead of five. Nothing in report-err needs more than 1.81: `core::error::Error` 1.81, `offset_of!` 1.77, `#[diagnostic::on_unimplemented]` 1.78, `Backtrace` 1.65, `Termination` 1.61, `cfg(panic)` 1.60. If James sets L1 to 1.85, report-err follows (edition 2024). The MSRV clippy gate applies: run `cargo +1.81 clippy` locally, not only `build`. |
| RD-10 | **`ensure!` operand values** (`left: 3, right: 4`) | at 1.0; after 1.0 | **After 1.0 (P3).** It needs a token-tree parser in `macro_rules!` that adds compile time to every use; the plain condition text covers the need. The bare-condition text format is chosen so it can be extended without breaking. |
| RD-11 | **Visibility and collection** | public in `error-collection`; private | **Public, `error-collection`.** It replaces anyhow in public portfolio crates' binaries and tests and holds nothing proprietary. |
| RD-12 | **`{:?}` layout** | anyhow's layout plus new blocks after it; a new layout | **anyhow's layout first** (`Caused by:` with anyhow's numbering), new blocks after, so reports look familiar and anyhow-era log parsers keep working on the first lines. |
| RD-13 | **`Ok` helper (ROOT-04)** | export as anyhow does; omit | **Export, not in the prelude** (a glob import would shadow `Ok`). It exists only to ease migration of closures that use `anyhow::Ok`. |
| RD-14 | **One-allocation formatted messages** (`err!("x {y}")`) | keep two allocations (`String` + node); format into the node with a two-pass write (measure length, then allocate a node with a trailing string), which extends the `unsafe` core with a dynamically sized node | **Decide at 0.6 with numbers.** Expected gain is one allocation (about 21-23 ns measured) on a common path; expected cost is formatting twice and about 80 more lines of `unsafe`. Adopt only if B5 improves by at least 20% and Miri stays clean. |
| RD-15 | **`unraisable` with no reporter installed** | drop silently; write one guarded line to standard error | **Write one line** (`std` only), as Python does for unraisable exceptions: an error in `Drop` must not vanish. Silent under `no_std`. |
| RD-16 | **Creation hook isolation** | `catch_unwind` around the hook (std + unwind); let a hook panic propagate | **Isolate.** A broken observer must not turn every error creation into a panic; the error records `creation_hook = "panicked"` so the bug is visible. |
| RD-17 | **Where the creation hook slot (R25) lives** | in L1 beside the reporter slot (request X-9); in report-err | **Answered by L1: in define-err** (its draft 0.2 API accepts X-9, D-012): one place to install observers, and L7 needs no dependency on report-err. Generated typed errors never call the slot; only report-err does. The hook is off unless installed and costs one load. |
| RD-18 | **Name of the ad-hoc error macro** (anyhow's `anyhow!`, the original design's `fault!`) | `err!`; `error!`; `format_err!` only; `anyhow!` | **Decided by James (D-017): `err!`, with `format_err!` as an alias**, as recommended: `error!` collides with `log` and `tracing`; `format_err!` keeps code that already uses anyhow's and eyre's alias migrating by crate name alone. |

---

## 18. References

URLs were checked on 2026-10-22 unless marked (*), which marks a reference cited for orientation and not opened while writing.

### 18.1 Rust language and standard library

- `std::process::Termination` (stable 1.61.0; `Result<T, E>` implementor): https://doc.rust-lang.org/std/process/trait.Termination.html
- `std::process::ExitCode` (*): https://doc.rust-lang.org/std/process/struct.ExitCode.html
- `std::backtrace::Backtrace` (capture rules, `force_capture`, `frames` nightly-only): https://doc.rust-lang.org/std/backtrace/struct.Backtrace.html
- `std::panic::catch_unwind` (panic = abort, foreign exceptions, hook order, payload drop may panic): https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
- `#[track_caller]`: https://doc.rust-lang.org/std/attribute.track_caller.html
- Reference, the `track_caller` attribute (*): https://doc.rust-lang.org/reference/attributes/codegen.html#the-track_caller-attribute
- `core::error::Error` (stable in `core` since 1.81) (*): https://doc.rust-lang.org/core/error/trait.Error.html
- Miri (*): https://github.com/rust-lang/miri
- loom (*): https://github.com/tokio-rs/loom

### 18.2 Competitors and prior art (Rust)

- anyhow 1.0.104, crate docs: https://docs.rs/anyhow/latest/anyhow/
- anyhow `Error` (one word, formatting representations, backtrace variables): https://docs.rs/anyhow/latest/anyhow/struct.Error.html
- anyhow `Context` (sealed; implemented for `Result` and `Option`; downcast to `C` or `E`): https://docs.rs/anyhow/latest/anyhow/trait.Context.html
- anyhow `ensure!`: https://docs.rs/anyhow/latest/anyhow/macro.ensure.html
- anyhow source, `ErrorImpl`, `ErrorVTable`, `ContextError`, `construct`: https://github.com/dtolnay/anyhow/blob/master/src/error.rs
- eyre 0.6.14 `EyreHandler`: https://docs.rs/eyre/latest/eyre/trait.EyreHandler.html
- eyre `set_hook` (handler built whenever a `Report` is constructed): https://docs.rs/eyre/latest/eyre/fn.set_hook.html
- eyre source, `ErrorHeader` with `handler: Option<Box<dyn EyreHandler>>`: https://github.com/eyre-rs/eyre/blob/master/eyre/src/error.rs
- color-eyre 0.6.5 `Section` (note, warning, suggestion, section, error, `suppress_backtrace`): https://docs.rs/color-eyre/latest/color_eyre/section/trait.Section.html
- error-stack 0.8.0 `Report` (`attach`, `attach_opaque`, `change_context`, `downcast_ref` on stable, `request_ref` nightly, `Report<[C]>` with `expand`, `push`, `append`, `install_debug_hook`): https://docs.rs/error-stack/latest/error_stack/struct.Report.html
- error-stack crate docs (`bail!`, `ensure!`, `no_std`, optional dependencies): https://docs.rs/error-stack/latest/error_stack/
- snafu 0.9.2 `Report` (`Termination`, `#[snafu::report]`, `capture`, `from_error`): https://docs.rs/snafu/latest/snafu/struct.Report.html
- snafu crate docs (`Whatever`, `whatever!`, `ensure_whatever!`, `OptionExt`, `ChainCompat`, `Location`): https://docs.rs/snafu/latest/snafu/
- crates.io API, versions and `rust-version` of the five competitors: https://crates.io/api/v1/crates/anyhow (and `/eyre`, `/color-eyre`, `/error-stack`, `/snafu`)
- Locations of `?` through a `#[track_caller]` `From` impl: https://www.svix.com/blog/try-operator-error-locations-rust-tests/ ; https://users.rust-lang.org/t/getting-line-numbers-with-as-i-would-with-unwrap/47002 ; https://docs.rs/traced_result

### 18.3 Prior art outside Rust (*)

- Zig error return traces: https://ziglang.org/documentation/master/#Error-Return-Traces
- Python `BaseException.add_note` and `ExceptionGroup`: https://docs.python.org/3/library/exceptions.html
- Python unraisable exceptions (`sys.unraisablehook`): https://docs.python.org/3/library/sys.html#sys.unraisablehook
- Java `Throwable.addSuppressed`: https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/Throwable.html
- Go `errors.Join`: https://pkg.go.dev/errors#Join
- .NET `AggregateException`: https://learn.microsoft.com/en-us/dotnet/api/system.aggregateexception
- sysexits (exit code values): https://man.freebsd.org/cgi/man.cgi?query=sysexits

### 18.4 Related plans and reference code

- The portfolio's library standard (the standard this plan follows) and `REPS.md`.
- The error collection plan (the split into L1 to L7, research additions R1 to R26, what can never be observed, the coms bridge and kind masks) and the earlier error suite plan (decision E-2 on the representation).
- define-err's plan ([`jamesgober/define-err`](https://github.com/jamesgober/define-err), L1; the interface of 3.4 is requested from it).
- kraken (the first migration target: `kraken/compiler/src/main.rs`, `kraken/compiler/src/modules/loader.rs`, `kraken/Cargo.toml`, `kraken/compiler/Cargo.toml`, `kraken-lsp/Cargo.toml`).
- Probe programs used for 7.4 and 11.4 (not part of any crate; reproducible from the descriptions in 7.4 and 11.4).

---

## 19. Ideas

Defence ideas are recorded as hypothesis, method, and expected cost (6.4); section 10.3 holds the ones planned per version, and this section the ones added as the work goes.

- **Defence idea (0.1, from D-003):** hypothesis: the chain view (7.6.1) is the one place where a frame reads memory it does not own; if a bug ever left a frame whose address is not one of its node's frame slots (a frame copied out of its node, or a slot table out of step with the frames), `source()` would read the wrong layer or worse. Method: `source()` returns a layer only after it has found its own address among the node's slots; a frame that cannot find itself returns `None` (the chain ends early, fail closed) instead of assuming a position, and a debug assertion makes the case loud in tests and under Miri. Expected cost: none beyond the comparisons `source()` makes anyway [est.]. Test: `test_raw_frame_not_in_its_node_has_no_source`, built through the `raw` test helpers.
- **Defence idea (0.2, from D-015):** hypothesis: when a coded error's identity says it has its own public message but no catalog is configured to supply the text, the easy fallback (the error's `Display`) would put a private message, possibly with a secret, in a client response. Method: `public_message()` falls back only to the category's default public text, which is fixed and generic, never to any `Display`; the T3 canary test is extended with a coded error whose `Display` holds the canary, no catalog configured, and every path that produces client-facing text (`public_message`, the record's `public` field). Expected cost: none at run time [est.]. Test: `test_public_message_without_catalog_is_category_default`.
- **Defence idea (0.3):** hypothesis: a `Display` that returns different text on each call (a counter, a clock, an attacker-controlled cell) can make the two passes of `{:?}` disagree, so the de-duplication decision is made on one text and another is printed, which could hide a layer from a log reader. Method: the guarded renderer of 0.5 writes each link from the text it captured for the decision when that text fit the buffer, so each link is formatted once; a test with a `Display` that alternates between two texts checks that what is printed is what was compared. Expected cost: none beyond the buffer the decision already uses [est.].
- **Idea (0.1, revised by D-011):** `chain()` and a hand-written `source()` walk must visit the same links, so any `&dyn Error` consumer (L4 render, a logger, a test helper) sees exactly what `chain()` sees; the anyhow parity suite (12.7) compares hand-written `source()` walks as well as `chain()`, and E-49a checks the two against each other after every kind of layer movement.

---

## 20. Decision log

| Id | Date | Decision | Reason |
|---|---|---|---|
| D-001 | 2026-10-22 | Name `report-err` (import path `report_err`), public repository `jamesgober/report-err`, in `error-collection` (RD-1, RD-11) | James's choice; the name was free on crates.io and GitHub on the day the repository was created |
| D-002 | 2026-10-22 | The main type is `Error`, not `Fault` (reverses RD-2's recommendation). The API ids `FLT-01` to `FLT-18` become `ERR-01` to `ERR-18`; the failure collection `Faults` becomes `Errors`; the private `RawFault` becomes `RawError` | James does not like `Fault` and wants migrating from anyhow to be a change of crate name: `anyhow::Error`, `anyhow::Result`, `Context`, `bail!`, and `ensure!` keep their names. The coexistence case RD-2 worried about is served by `use report_err::Error as Report;` |
| D-003 | 2026-10-22 | An `Error` dereferences (`Deref`, `DerefMut`, `AsRef<dyn Error + Send + Sync>`, `AsRef<dyn Error>`) to a `dyn Error` whose `source()` walks every context layer, then the root error, then the root's own sources (reverses RD-4's trade-off). The store stays flat; each frame carries a copy of its node pointer and finds the layer below it by address comparison (7.6.1, invariant I-12). The cost is measured at the `0.2.0` design lock; if one-word size or an allocation count of 7.5 would have to change, James gets the numbers and the options first | James's requirement: an `Error` must work wherever a `&dyn Error` is expected, as anyhow's does, with no trade-off. The stored node pointer (instead of pointer arithmetic from `&self`) keeps the design sound under Stacked Borrows and Tree Borrows with strict provenance; address comparison needs no provenance; no frame points at another, so frames can move inside `Ext` |
| D-004 | 2026-10-22 | Consequences of D-003 for the L1 interface (3.4): X-7 is reduced to an optional request (`ErrorRecord::from_error(&**error)` already sees every layer); X-8's main path is `report_with(&**error, severity, kinds)` with no allocation, and `report_record` remains a request only so reporters can see sections and the trail. `into_std()` becomes one allocation in all instead of one per layer | the real chain makes the temporary nested chain of the original fallbacks unnecessary |
| D-005 | 2026-10-22 | MSRV 1.81, edition 2021 (RD-9 as recommended) | define-err's MSRV (its OD-2): `core::error::Error` in `core` is stable from 1.81 and the `no_std` build needs it; kraken (1.80) moves one minor version. If define-err moves to 1.85, report-err follows with edition 2024 |
| D-006 | 2026-10-22 | CI from `0.1.0`: the test matrix on Linux, macOS, and Windows, each on x86_64 and aarch64, on stable and 1.81 (fmt, clippy and tests in three feature modes, examples, docs in three feature modes); `no_std` build for `thumbv7em-none-eabihf` on stable and 1.81; Miri with strict provenance under Stacked Borrows and Tree Borrows; loom (`--cfg loom`); package; `cargo audit` and `cargo deny` | 6.13; the jobs exist before the code they guard, so the first `unsafe` line in 0.3 lands under Miri. `cargo doc` also runs with `--no-default-features` because a doc link to a `std`-only item breaks only there (15) |
| D-007 | 2026-10-22 | The crate root refuses targets without pointer-sized atomic compare-and-swap with a `compile_error!` naming the reason, from `0.1.0` | 7.19: the write-once configuration and the `reported` flag need it; a clear message at build time beats a missing-symbol error later |
| D-008 | 2026-10-22 | The macro is written `err!` throughout this plan, pending RD-18 at the design lock | `fault!` no longer matches the type (D-002); `error!` collides with `log` and `tracing` |
| D-009 | 2026-10-23 | The chain view of D-003 was prototyped on the planned layout and measured (7.6.1): one machine word and every allocation count of 7.5 unchanged; 8 bytes more per frame; creation and context 1 to 14 ns slower than without the node pointer and still 1.5 to 2 times faster than anyhow; Miri-clean under both aliasing models with strict provenance. No escalation to James is needed | James asked for the numbers and for options only if the chain could not be had without giving up one-word size or the allocation counts; it can |
| D-010 | 2026-10-23 | `Error` also implements `DerefMut` (as anyhow does). Code reachable from `deref`, `deref_mut`, and `source()` only projects to the fields it reads (invariant I-13) | parity; the prototype showed that a reference to the whole header or extension block, taken while walking `source()`, would invalidate a live `&mut` from `DerefMut` under Stacked Borrows; field projections keep both sound, checked under Miri |
| D-011 | 2026-10-23, decided 2026-10-29 | `chain()` reads the context layers from the flat store by index and then follows the root's own `source()` chain; it yields exactly the links a hand-written `source()` walk visits. The per-link mechanism of `source()` was chosen by benchmark among address comparison, a per-frame index, and a direct link: **the direct link** (each frame points to the next layer inward; the innermost one to its node, tagged), measured with criterion level with anyhow at one layer and 11% to 18% faster from three layers on (7.6.1) | the target was a hand-written walk within 5% of anyhow per link; only the direct link met it, and it keeps the 40-byte frame |
| D-012 | 2026-10-23 | define-err's draft 0.2 API accepts all four requests (X-1 `NonZeroU64` niche, X-7 `ErrorRecord::new` and `push_chain`, X-8 `report_record`, X-9 the creation-hook slot). report-err therefore hosts no hook slot of its own (RD-17 answered); `report()` delivers the error's record through `report_record` so reporters see sections and the trail; `record()` starts from `ErrorRecord::from_error(&**self)` and overwrites the resolved code, severity, category, kinds, and public text. Final when define-err's 0.2 API is approved | the requests were made in 3.4; one place for observers (define-err) is simpler for L7 and for users |
| D-013 | 2026-10-23 | `Limits` has `pub` `u32` fields and is changed as a copy (`let mut limits = Limits::DEFAULT; limits.max_frames = 32;`), like define-err's `RecordLimits`; `configure` validates every field | one way to set a field instead of a getter and a setter per field; the same shape as the sibling crate's limits; `#[non_exhaustive]` still lets fields be added |
| D-014 | 2026-10-23 | anyhow-parity additions to the API: ERR-19 (`Deref`, `DerefMut`, `AsRef` to `dyn Error`, P0, 0.3); `Chain` is also `ExactSizeIterator` and `Clone`; `From<Error> for Box<dyn Error + Send + 'static>`; `From<Error> for ExitReport` | anyhow provides each; a migration that is a change of crate name (D-002) needs all of them |
| D-015 | 2026-10-23 | `public_message()` (ERR-15): the code's catalog entry, else the root's `ErrorInfo` when built with `from_info`, else, when only the identity is known, the category's default public text; never any `Display` text | define-err's identity probe carries a flag for "has its own public message" but not the text; falling back to a generic text keeps private messages out of client responses (section 19, 0.2 defence idea) |
| D-016 | 2026-10-29 | RD-3 (James): the one-word layout, with `unsafe` confined to `src/raw.rs` | James: "keep the one-word layout with unsafe in one module"; the reasons are in 7.4 |
| D-017 | 2026-10-29 | RD-18 (James): the ad-hoc macro is `err!`, with `format_err!` as an alias | James: "use err! (format_err! alias)"; D-008 is now final |
| D-018 | 2026-10-29 | Every open recommendation not decided by James is accepted as written: RD-5 (the identity probe for codes), RD-6 (`UnwindSafe` bound on `boundary`), RD-7 (`coded` on `Result` and `Option`), RD-8 (write-once `configure`), RD-9 (MSRV 1.81, already D-005), RD-10 (`ensure!` operand values after 1.0), RD-11 (public, `error-collection`), RD-12 (anyhow's `{:?}` layout first), RD-13 (`Ok` exported, not in the prelude), RD-14 (one-allocation formatting decided at 0.6 with numbers), RD-15 (`unraisable` writes one line without a reporter), RD-16 (hook isolation) | the design lock requires every open decision answered or accepted (14, 0.2.0); none of these crossed a rule or needed James's judgement |
| D-019 | 2026-10-29 | define-err is a dependency from `0.2.0`, by published version (`0.2`, `default-features = false`, `alloc` on; report-err's `std` feature turns on define-err's `std`). Its types are first used in `0.4.0` | the design lock is written against define-err's locked `0.2` API; depending on that version now pins the interface the API names, and CI then checks every feature combination with it from the first release |
| D-020 | 2026-10-29 | The chain walk has no repeat check; the `max_chain` budget alone bounds it, and `Chain::len()` is exact (amends 7.7 and T5) | Miri showed that comparing trait-object pointers gives different answers on two walks of the same chain (vtable pointers are not unique), which broke `ExactSizeIterator`; comparing addresses alone stops at the first newtype wrapper whose `source()` is its own field. No check on stable Rust is both reliable and deterministic |
| D-021 | 2026-10-29 | Compile-fail cases (E-18, E-19, the sealed `Context`, no `Clone`) are `compile_fail` doc tests in the crate root, not trybuild | the same check with no dependency and no compiler-version-specific output files to keep in step with stable |
| D-022 | 2026-10-29 | B12 (`{:#}`) is recorded as a measured exception: 2.0 times slower than anyhow at 0.3, because the de-duplication rule needs the root's text before printing it. Strings the error holds are compared in place; only links that must be formatted use a 256-byte buffer, made on first need. The candidates for 0.6, measured there: an uninitialised buffer in `raw` (no zeroing) and a streaming comparison for links shorter than the layer before them | the standard allows an exception with its reason; the alternative that avoids the buffer formats each such link twice, which is slower for operating-system errors, whose `Display` asks the OS for the text |
| D-023 | 2026-10-29 | Benchmarks are a separate, unpublished package in `benches/` (criterion 0.8, anyhow 1.0.104 pinned), built in CI on stable only; the crate sets `autobenches = false` | criterion's MSRV (1.86) is above report-err's (1.81), and the MSRV jobs build every target of the crate |
| D-024 | 2026-10-29 | Miri runs the unit and integration tests (`--lib --tests`), not the doc tests; the allocation-count test binary is skipped under Miri, and the one test that reads a file is ignored there | the doc tests read files, which Miri isolates, and contain no `unsafe` code; a global allocator that forwards to `System` makes Miri interpret the platform allocator, outside this crate |
| D-025 | 2026-10-29 | `downcast_ref`, `downcast_mut`, and `downcast` skip the context layers when the target type is neither `&'static str` nor `String` and no layer holds another value type; the header keeps a 16-bit count of such layers beside a 16-bit layer count | B10 went from 12% slower than anyhow to 2.4 times faster; the counters fit the bytes the 32-bit layer count used (`max_frames` is at most 4,096) |
| D-026 | 2026-10-29 | A boxed root (`from_boxed`, `err!(boxed)`) answers `downcast_ref::<Box<dyn Error + Send + Sync>>()`; its concrete error is the root link of `chain()` and of the `source()` walk, so `root_cause().downcast_ref::<T>()` and (in 0.5) `find::<T>()` reach it | `downcast_ref::<T>` has no `T: Error` bound (anyhow's signature), so it cannot ask a `dyn Error` for `T`; the chain can |

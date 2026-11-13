<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>report-err</b><br>
    <sub><sup>MIGRATING</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./API.md" title="API reference"><b>API</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>MIGRATING</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="../dev/PLAN.md" title="Plan"><b>PLAN</b></a>
    </sup>
</div>
<br>

Moving from anyhow, eyre, or snafu's `Whatever` to report-err. For anyhow it is a change of crate name: every name anyhow users write keeps its name and its meaning, except the `anyhow!` macro, which is `err!` here.

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## Table of contents

- [From anyhow](#from-anyhow)
  - [The rules](#the-rules)
  - [A find-and-replace pass](#a-find-and-replace-pass)
  - [What behaves differently](#what-behaves-differently)
  - [A worked migration: a compiler](#a-worked-migration-a-compiler)
- [From eyre and color-eyre](#from-eyre-and-color-eyre)
- [From snafu's `Whatever`](#from-snafus-whatever)
- [From report-err 0.7 to 0.8](#from-report-err-07-to-08)
- [From report-err 0.8 to 0.9](#from-report-err-08-to-09)

## From anyhow

### The rules

| anyhow | report-err | Note |
|---|---|---|
| `anyhow = "1"` in `Cargo.toml` | `report-err = "0.4"` | |
| `use anyhow::{Context, Result};` | `use report_err::{Context, Result};` | same names |
| `use anyhow::{anyhow, bail, ensure};` | `use report_err::{err, bail, ensure};` | `anyhow!` is `err!` |
| `anyhow::Result<T>`, `anyhow::Error` | `report_err::Result<T>`, `report_err::Error` | same names |
| `anyhow!(...)`, `anyhow::anyhow!(...)` | `err!(...)`, `report_err::err!(...)` | same arms |
| `format_err!(...)` | `report_err::format_err!(...)` | unchanged |
| `bail!(...)`, `ensure!(cond, ...)` | the same | same arms |
| `Error::new`, `Error::msg`, `.context`, `.with_context`, `.chain()`, `.root_cause()`, `.downcast`, `.downcast_ref`, `.downcast_mut`, `.is` | the same, on `report_err::Error` | |
| `&*error`, `error.source()`, `error.as_ref()` as `&dyn Error` | the same: every context layer is a link of the `source()` chain | |
| `Box<dyn Error + Send + Sync>` from `error.into()` | the same | |
| `.ok_or_else(\|\| anyhow!("text"))` | `.context("text")` | optional; the original also works |
| `anyhow::Ok(v)` | `report_err::Ok(v)` | from `0.4` |
| `.backtrace()` | the same | from `0.4` |

### A find-and-replace pass

Run on a branch and review the diff:

```text
s/\banyhow::anyhow!/report_err::err!/g
s/\banyhow::/report_err::/g
s/\buse anyhow::/use report_err::/g
s/\banyhow!(/err!(/g      (only in files that imported the macro by name)
```

and in `Cargo.toml`, `anyhow = "..."` becomes `report-err = "0.9"`.

### What behaves differently

| Difference | Effect on migrated code | Why |
|---|---|---|
| `ensure!(cond)` with no message | the error says `condition failed: <the condition's source text>` | report-err's own wording; a test that compares the text needs updating |
| `{:?}` | adds `Location:` (and, from `0.4`, the trace, sections, and code) after anyhow's `Caused by:` block, which stays the same | a test that compares the whole `{:?}` output needs updating; anyhow-era log parsers keep working on the first lines |
| `{:#}` and `Caused by:` | a layer is skipped when the layer before it already ends with `": "` followed by exactly its text, or is exactly its text | errors whose `Display` already includes their source are not printed twice |
| `downcast_ref::<T>()` on an error deeper than the root | `None`, as with anyhow | use `root_cause()` or, from `0.5`, `find::<T>()` |
| a boxed root (`from_boxed`) | answers `downcast_ref::<Box<dyn Error + Send + Sync>>()`; its concrete error is the root link of `chain()` | `root_cause().downcast_ref::<T>()` reaches it |
| exit status from `main` | unchanged (1) unless `ExitReport` is used (`0.4`) | opt-in |
| MSRV | 1.81 | `core::error::Error` |

### A worked migration: a compiler

A compiler and language server was the first migration, tried on 2026-10-29 against report-err `0.3.0` on a copy of its workspace:

| Where | Usage | Count | After the pass |
|---|---|---|---|
| workspace `Cargo.toml` | `anyhow = "1.0"` | 1 | `report-err` |
| `compiler/Cargo.toml` | `anyhow = { workspace = true }` | 1 | `report-err = { workspace = true }` |
| `compiler/src/main.rs`, `compiler/src/modules/loader.rs` | `use anyhow::{Context, Result};` | 2 | `use report_err::{Context, Result};` |
| the same two files | `anyhow::bail!(...)` | 26 | `report_err::bail!(...)` |
| the same two files | `anyhow::anyhow!(...)`, inside `.map_err` and `.ok_or_else` | 8 | `report_err::err!(...)` |
| the same two files | `.context(...)`, `.with_context(...)` | 34 | unchanged |
| `compiler/src/main.rs` | a test helper that walks `err.chain()` and calls `to_string()` | 1 | unchanged |
| `compiler/src/main.rs` | `#[tokio::main] async fn main() -> Result<()>` | 1 | unchanged |

Result: the pass above changed 2 manifest lines and 36 source lines; no line needed a hand edit. The compiler built, and its test suite gave the same result as with anyhow: 1,974 tests passed under both, and the same 3 tests failed under both (`async_basic_compile_and_run`, `async_await_compile_and_run`, `dyn_dispatch_compile_and_run`, which link a native program and failed with "Linking failed" on the machine used, with anyhow as well). The merge into the compiler itself follows with report-err's alpha.

## From eyre and color-eyre

`eyre::Report` maps to `Error`, `eyre::Result` to `Result`, `eyre!` to `err!`, `WrapErr::wrap_err` to `context`, `wrap_err_with` to `with_context`. color-eyre's `Section` methods (`note`, `warning`, `suggestion`) have the same names on `Context` and `Error`, and their `with_` forms on `Context`. eyre's `set_hook`/`EyreHandler` has no equivalent: formatting is fixed, colour belongs to the render library, and scope data at creation goes through define-err's creation hook (`0.5`).

## From snafu's `Whatever`

`Whatever` maps to `Error`, `whatever!` to `err!` (or `bail!` where it returns), `ensure_whatever!` to `ensure!`, and `whatever_context` to `context`. snafu's typed errors move to define-err, not to report-err: report-err is for the application layer, where errors are carried and reported rather than matched on.

## From report-err 0.7 to 0.8

0.8 removed the methods that only repeated another way of doing the same thing, gave two items plainer names, and moved to define-err 0.9. Each change is a compile error at the old name, never a change of behaviour behind the same name. Run on a branch and review the diff:

```text
s/\breport_err::unraisable\b/report_err::report_unhandled/g
s/\bunraisable(/report_unhandled(/g
s/\.with_kind(/.with_kinds(/g
```

| 0.7 | 0.8 | Note |
|---|---|---|
| `unraisable(error)` | `report_unhandled(error)` | Without a reporter, the line on standard error now starts `Error (unhandled): ` instead of `Error (unraisable): `. |
| `error.with_kind(kinds)` | `error.with_kinds(kinds)` | It always took a `Kinds` set; the name now says so. |
| `error.with_note(\|\| text)` on an `Error` | `error.note(text)` | The error already exists, so the closure ran at once. `with_note`, `with_warning`, and `with_suggestion` stay on `Context` (a `Result` or `Option`), where the closure runs only on failure. |
| `error.with_warning(\|\| text)`, `error.with_suggestion(\|\| text)` on an `Error` | `error.warning(text)`, `error.suggestion(text)` | The same. |
| `error.into_std()` | `error.into()`, or `Box::<dyn Error + Send + Sync>::from(error)` | `From` gives the same box, for `Box<dyn Error + Send + Sync>`, `Box<dyn Error + Send>`, and `Box<dyn Error>`; `Error::from_boxed` still gives the original `Error` back. |
| `errors.finish_with(value, message)` | `errors.finish(message)?;` then `Ok(value)` | |
| `error.find_in_suppressed::<T>()` | `error.suppressed().iter().find_map(\|e\| e.find::<T>())` | The old method also searched the suppressed errors of suppressed errors. |
| `define-err = "0.5"` beside report-err | `define-err = "0.9"` | report-err re-exports `Code`, `Severity`, `Category`, and `Kinds`; with two define-err versions in one build they are different types. Nothing else changed between define-err 0.5 and 0.9 for code that uses report-err. |

## From report-err 0.8 to 0.9

0.9 moves to define-err 0.10 and changes no name. In `Cargo.toml`, `report-err = "0.8"` becomes `"0.9"`, and a `define-err` dependency beside it becomes `"0.10"`: define-err 0.10 refuses a second copy of itself in one build, so the two must move together.

| What | 0.8 | 0.9 |
|---|---|---|
| `define-err` beside report-err | `"0.9"` | `"0.10"`; see define-err's own migration note for its changes |
| `error.report()` with no reporter installed | `true`, and the error was marked reported | `false`, and nothing is marked, so the error can be reported once a reporter is installed |
| `configure_catalogs` with two entries that share a code | accepted; the first entry answered | `Err(ConfigureError::DuplicateCode { code, first, second })`, and nothing is set |
| A foreign error whose `description()` begins with define-err's probe (std string errors on Rust 1.81 to 1.89) | could pass for a defined error | is not read as one (define-err 0.10) |

A `match` on `ConfigureError` already has a wildcard arm, since the enum is `#[non_exhaustive]`.

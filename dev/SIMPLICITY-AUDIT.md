# Simplicity audit of the 0.7 API

James, 2026-11-11: "You're free to simplify the API ... we're not 1.0.0 yet." This audit checks every public item of report-err 0.7.0 against four questions, records what was found, and says what 0.8.0 does about it. The changes are in `CHANGELOG.md` under 0.8.0 and `docs/MIGRATING.md` ("From 0.7 to 0.8"); the decisions are D-067 to D-069 in `dev/PLAN.md`.

The four questions:

1. **One line for the common case.** Can a newcomer return an error, add context, and end `main` with a report in one obvious line each, without a builder, a configuration object, or a type parameter they must name?
2. **Plain names.** Can a newcomer read every name without the docs?
3. **One way to do each thing.** Does any item only repeat another?
4. **The reference.** Does `docs/API.md` cover every public item, task by task, with examples that are compiled and run?

## 1. One line for the common case: passes

| Task | The line |
|---|---|
| Return an error | `bail!("port {port} is in use")` |
| Check a condition | `ensure!(!name.is_empty(), "name is empty")` |
| Add context | `std::fs::read(path).context("reading the config")?` |
| Add context built only on failure | `.with_context(\|\| format!("reading {path}"))?` |
| Turn `None` into an error | `map.get(key).context("no such key")?` |
| Give an error a code | `.coded(DB_DOWN)?` or `bail!(DB_DOWN, "pool empty")` |
| Tell the reader what to do | `.suggestion("run tool init")?` |
| End `main` with a report | `fn main() -> Result<()>` |
| End `main` with the error's exit code, panics included | `fn main() -> ExitReport { run(app) }` |
| Collect every failure of a loop | `.collect_all()?` |

Every one is a single call on a value the caller already has. Nothing in Tier 1 needs a builder or a named type parameter. No change.

## 2. Plain names: two findings

| Item | Finding | 0.8.0 |
|---|---|---|
| `unraisable(error)` | Python's term for an exception raised where nothing can catch it. A reader who does not know Python cannot guess that it reports an error that has nowhere to go. | Renamed **`report_unhandled`**: it pairs with `Error::report`, and the record it sends is marked unhandled. |
| `Error::with_kind(kinds)` | Takes a `Kinds` set and adds every kind in it, and the getter is `kinds()`; the singular name suggests one kind replaces another. | Renamed **`with_kinds`**. |

Kept, with the reason:

- `coded`: a plain word that reads as English at the call (`.coded(DB_DOWN)?`), used in every rehearsal. A `with_code` would also clash with `Context`'s rule that `with_` takes a closure.
- `trace` and `trail`: `.trace()` adds a step to `trail()`; each says what it does, and the docs show them together.
- `PANICS_CAUGHT`, `RenderStyle`, `SectionKind`, `StepKind`, `Limits`, `ConfigureError`, `CollectAll`, `FindAll`: plain words.

## 3. One way to do each thing: four findings

| Item | Repeats | 0.8.0 |
|---|---|---|
| `Error::with_note`, `Error::with_warning`, `Error::with_suggestion` | `note(f())`, `warning(f())`, `suggestion(f())`. The error already exists, so the closure always runs at once; the lazy form saves nothing. | **Removed.** The lazy forms stay on `Context`, where the closure runs only on failure, as `with_context` does. |
| `Error::into_std` | `From<Error> for Box<dyn Error + Send + Sync>`, which `?` and `.into()` already use. | **Removed.** `From` already exists for `Box<dyn Error + Send>` and `Box<dyn Error>` too, as in anyhow, so `.into()` works for every box. |
| `Errors::finish_with(value, message)` | `errors.finish(message)?; Ok(value)`. | **Removed.** |
| `Error::find_in_suppressed` | `error.suppressed().iter().find_map(\|e\| e.find::<T>())`. It also searched the suppressed errors of suppressed errors, which no use has needed. | **Removed.** |

Kept, with the reason: each pair below is the anyhow surface, and moving from anyhow stays a change of crate name (D-002):

- `err!` and `format_err!` (James, D-017).
- `Error::new`, `Error::msg`, and `From<E>`.
- `is` and `downcast_ref`.
- `root_cause` and `chain().last()`.
- `report_err::Ok` and `Ok`.

Kept because they look alike but do different things:

- `downcast_ref` and `find`: the first looks at the context values and the root, as anyhow does; the second walks the root's sources and every child too.
- `Error::group`, `Errors`, `collect_all`, and `push_child`: a group from a list, from a loop, from an iterator, and one more child for an error that already exists.
- `Errors::finish` and `Errors::into_error`: a `Result` for `?`, or an `Option` for code that builds something else from the group.
- `fn main() -> Result<()>`, `ExitReport`, and `run`: exit code 1 with the report, the error's own exit code, and the same with panics caught.
- `attach` and `attach_display`: a value the report never prints, and one it does.

## 4. The reference: rewritten

`docs/API.md` was the 0.2 design-lock document: one section per item, with signatures, ids, grades, and the version each item arrived in. It was not a tour by task, its examples were not compiled, and it described items as they were planned, not as they are. 0.8.0 replaces it with a task-by-task reference in the format the other libraries in this collection use: a nested table of contents, every public item with what it does, its parameters, and what it returns or fails with, and examples for each use. `lib.rs` includes the file under `cfg(doctest)`, so every example in it is compiled and run by `cargo test`. The item ids stay in `dev/PLAN.md` section 8.

## Shared with define-err

report-err moves from define-err 0.5 to 0.9 in 0.8.0 (D-069). An application that uses both crates (the first migration's pull request is on define-err 0.9) would otherwise build two copies of `Code`, `Severity`, `Category`, and `Kinds` that do not convert into each other. define-err 0.6 to 0.9.3 changed nothing report-err calls.

These define-err items are part of report-err's own public API, so a rename in define-err is a rename in report-err in the same minor:

- Re-exported at the crate root: `Code`, `Severity`, `Category`, `Kinds`.
- In signatures: `ErrorInfo` (`from_info`, `info`), `CodeInfo` (`configure_catalogs`), `ErrorRecord` (`record`).
- In examples and the code-first macro arms: `code!`, `define_errors!`, the `Severity`, `Kinds`, and `Category` constants.
- Inside report-err only: `identify`, `innermost_code`, `Identity`, `is_installed`, `report_record`, the `ErrorRecord` builders, `TrailEntry`, `TrailStep`, `Category::from_io_kind`, `exit_code`, `default_severity`, `default_public_message`, and the creation hook (`CreationHook`, `CreationInfo`, `Annotate`, `creation_hook`).

## Who a break touches

Nobody outside this repository yet: crates.io lists no dependents of report-err, and no crate on the development machine depends on it. The two migration rehearsals use none of the items 0.8.0 removes or renames. The fuzz targets, the soak, the benchmarks, and the tests in this repository are updated in the same change.

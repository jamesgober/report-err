<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>report-err</b><br>
    <sub><sup>ARCHITECTURE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./API.md" title="API reference"><b>API</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./ARCHITECTURE.md" title="Architecture"><b>ARCHITECTURE</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./UNSAFE.md" title="The unsafe core"><b>UNSAFE</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="../dev/PLAN.md" title="Plan"><b>PLAN</b></a>
    </sup>
</div>
<br>

How report-err is put together: what each module owns, the paths an error takes from creation to report, and the rules that keep it bounded. The design and its reasons are in [`../dev/PLAN.md`](../dev/PLAN.md) section 7; the `unsafe` core is audited in [`UNSAFE.md`](./UNSAFE.md).

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## One error, one word

`Error` is a single pointer to one heap node: an 88-byte header (on 64-bit targets) followed by the root error itself. The header holds the vtable for the root's type, the origin location, the code and the severity and exit overrides, two atomic flags (made from a panic, reported), the innermost context layer, and two pointers, each to a block made the first time it is needed. The extension block holds context layers two and up (184 bytes, then a vector from the sixth layer). The extras hold everything else an error can gather: attachments, sections, trace points, the backtrace, children, suppressed errors, the creation hook's fields, and the counters of what a budget dropped. A plain error is one allocation; an error with one context layer is still one; with three, two. The two blocks are apart because context layers are the common case and the extras are 472 bytes an error with only layers would otherwise allocate and free.

## Modules

| Module | Owns |
|---|---|
| `raw` | the node, the vtable, the flat store of context layers and their `source()` links; the only module with `unsafe` code |
| `error` | `Error`, the constructors, `context`, downcasting, `chain`, `root_cause` |
| `frame` | one context layer's value: a literal, a `String`, or a boxed `Display` |
| `ext` | the extras (`Extras`), safe data: attachments, sections, trace points, children, suppressed errors, fields, counters |
| `chain` | the cause chain, outermost first, bounded by `max_chain` |
| `group` | several causes, suppressed errors, `split`, `flatten`, `find`, `Errors`, `CollectAll` |
| `codes` | code, severity, category, kinds, public message, exit code, looked up from define-err |
| `format` | `Display` and `Debug`, the report, the guarded renderer (`render`) |
| `hook` | the creation hook, `report`, `record`, `unraisable` |
| `panic` | `boundary`, `run`, `from_panic` |
| `exit` | `ExitReport` (std) |
| `limits` | the budgets, `configure`, `configure_catalogs` |
| `context` | the `Context` trait on `Result` and `Option` |
| `trace` | the trail and backtraces |
| `testing` | assertions for tests (feature `testing`) |

## The life of an error

1. **Created** (`Error::new`, `?`, `err!`, `bail!`, `ensure!`, `Error::msg`, `group`, `from_panic`, ...). The node is allocated with the root inside it and the caller's location as the origin. Then, in `created()`: a backtrace if std's rules turn them on, and the creation hook if one is installed (once, guarded against recursion on the same thread, and against a panic with `std`).
2. **Travels.** Each `.context(..)` adds a layer; each `.trace()` records a point; `.note(..)`, `.attach(..)`, `.coded(..)`, `push_child`, `suppress` add the rest. None of these runs the hook or captures a backtrace.
3. **Read.** `chain()`, `find`, `code()`, `severity()`, `category()`, `kinds()`, `public_message()`, `exit_code()` read what is there; none of them allocates.
4. **Reported.** `report()` sets the reported flag once and, when a reporter is installed, builds the error's `ErrorRecord` and hands it to define-err's facade. `ExitReport` prints the report to standard error and exits with the error's code. `unraisable` reports an error with nowhere to go, as unhandled.
5. **Dropped.** The node and its blocks are freed; children and suppressed errors are dropped with them. The tree's depth is bounded, so the drop's recursion is bounded too.

## Bounds

Every part that can grow has a budget (`Limits`, set once with `configure`): context layers, attachments, sections, suppressed errors, the errors in a tree and its depth, the bytes of text and values, the links walked in one chain, and the bytes one guarded rendering writes. A budget is checked before the work or the allocation it guards; what it drops is counted and printed under `Omitted:`. Suppressed errors count toward the tree and its depth like children, so no error can nest without bound in either direction.

## Guarded output

Every path that prints an error for someone else (`ExitReport`, `report`, `record`, `render`, `unraisable`) writes each `Display` through a guard: a `Display` that panics (with `std` and unwinding) prints `<display panicked>`, one that fails prints `<display failed>`, and the rest of the output goes on. Output stops at `max_render` bytes. The terminal paths (`ExitReport`, `unraisable`) also escape control characters and bidirectional controls, and indent a message's continuation lines, so no message can move the cursor or fake a block of the report. `{}`, `{:#}`, and `{:?}` are not guarded: they behave like any other `Display`.

## Process-wide state

Three write-once slots, each set at most once, read with one acquire load, never locked: the budgets (`configure`), the catalogs (`configure_catalogs`), and, in define-err, the reporter and the creation hook. A second write is refused and the first value stays.

## Dependencies

define-err, by published version, for codes, severity, categories, kinds, the identity probe, the catalog, the reporting facade, the creation hook, and `ErrorRecord`. Nothing else at run time, and no async runtime.

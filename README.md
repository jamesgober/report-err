<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>report-err</b>
    <br>
    <sub><sup>DYNAMIC ERRORS FOR APPLICATIONS</sup></sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/report-err"><img alt="Crates.io" src="https://img.shields.io/crates/v/report-err"></a>
    <a href="https://crates.io/crates/report-err"><img alt="Downloads" src="https://img.shields.io/crates/d/report-err?color=%230099ff"></a>
    <a href="https://docs.rs/report-err"><img alt="docs.rs" src="https://img.shields.io/docsrs/report-err"></a>
    <a href="https://github.com/jamesgober/report-err/actions"><img alt="CI" src="https://github.com/jamesgober/report-err/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.81%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>report-err</strong> gives application code one error type that holds any error, gathers context as it travels up the stack, and prints a clear report when it reaches <code>main</code>. It is the <code>anyhow</code> you know: <code>Error</code>, <code>Result</code>, <code>Context</code>, <code>bail!</code>, <code>ensure!</code>, <code>?</code>, a <code>source()</code> chain through every context layer, all with the same names, so moving from anyhow is a change of crate name.
    </p>
    <p>
        On top of that, an error can carry an error code with its severity, category, and exit code; a propagation trace of every place it passed through; typed attachments read back by type; notes, warnings, and suggestions for the person reading the report; several causes; and suppressed cleanup failures. A panic boundary turns a caught panic into an error, and <code>main</code> can exit with a code chosen from the error. All of it costs nothing until used: an error that only carries its cause and location is one allocation and one machine word, the same as anyhow. Its one dependency is <a href="https://crates.io/crates/define-err">define-err</a>, the portfolio's library for error codes, severity, and categories; it is <code>no_std</code> with <code>alloc</code>.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.81+</strong> (Rust 2021 edition). <code>no_std</code> + <code>alloc</code>; <code>std</code> on by default.
    </p>
    <blockquote>
        <strong>Status: <code>0.7.0</code>, feature complete, measured, and hardened.</strong> The anyhow surface; error codes, severity, categories, and kinds from define-err; notes, attachments, a propagation trail, and backtraces; <code>ExitReport</code>; several causes and <code>collect_all()</code>; suppressed errors and cleanup; budgets on everything that grows; the panic boundary; the creation hook; and reporting through define-err's facade. <code>0.6</code> measured it against anyhow, eyre, error-stack, and <code>Box&lt;dyn Error&gt;</code> on three platforms (<a href="./docs/PERFORMANCE.md"><code>docs/PERFORMANCE.md</code></a>); <code>0.7</code> hardened it with attack tests for every threat, a day of fuzzing per target, and 8-hour soaks (<a href="./docs/SECURITY.md"><code>docs/SECURITY.md</code></a>); next comes the alpha, with kraken migrated (<code>0.8</code>). The whole API is in <a href="./docs/API.md"><code>docs/API.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

## Install

```toml
[dependencies]
report-err = "0.7.0"
```

## Quick start

```rust
use report_err::{bail, ensure, Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {path}"))?;   // io::Error becomes an Error
    ensure!(!text.is_empty(), "config file {path} is empty");
    let config = parse(&text).context("parsing config")?;
    if config.port == 0 {
        bail!("port 0 is not allowed");
    }
    Ok(config)
}

fn main() -> Result<()> {
    let config = load_config("app.conf")?;
    run(config)
}
```

When `main` returns the error, the process prints:

```text
Error: reading config file app.conf

Caused by:
    No such file or directory (os error 2)

Location: src/main.rs:5:10
```

| Format | Prints |
|---|---|
| `{error}` | the outermost message: `reading config file app.conf` |
| `{error:#}` | the chain on one line: `reading config file app.conf: No such file or directory (os error 2)` |
| `{error:?}` | the report above |

Looking inside:

```rust
if let Some(io) = error.downcast_ref::<std::io::Error>() {
    eprintln!("I/O error of kind {:?}", io.kind());
}
for cause in error.chain() {
    eprintln!("  because: {cause}");
}
let as_std: &(dyn std::error::Error + 'static) = &*error;   // every layer is in its source() chain
```

## Codes, notes, and exit codes

Each of these costs nothing until it is used. Codes come from [define-err](https://crates.io/crates/define-err); an error with a code found anywhere in its chain has it without any extra call.

```rust
use report_err::{bail, Context, ExitReport, Result};

fn load_user(id: u64) -> Result<User> {
    let row = db.get(id).coded(DbError::NOT_FOUND)?;                // on Option or Result
    if !row.active {
        bail!(UserError::INACTIVE, "user {id} is inactive");
    }
    let user = parse(row)
        .suggestion("run the migration tool on this table")?;       // printed in the report
    Ok(user)
}

fn main() -> ExitReport {
    run().into()                                                    // exits with the category's code
}
```

```text
Error: user 7 is inactive

Code: USER-0003 (failed-precondition, severity 17)
Location: src/users.rs:6:9
```

| Ask | Answer |
|---|---|
| `error.code()` | the code set by hand, else the innermost code in the chain |
| `error.severity()`, `error.category()`, `error.kinds()` | from the code, a catalog, or an `io::Error` root |
| `error.public_message()` | text a client may see; never taken from any `Display` |
| `error.exit_code()` | `with_exit_code`, else the category's sysexits code, else 1 |
| `error.attachment::<T>()` | a typed value added with `.attach(value)` |
| `error.trail()` | where the error was made, each `.trace()`, and each context, in order |

`ExitReport` escapes terminal controls and Unicode bidirectional controls in every message, so a message cannot move the cursor or fake a block of the report, and it cuts the report at 1 MiB.

## Moving from anyhow

| anyhow | report-err |
|---|---|
| `anyhow = "1"` | `report-err = "0.7.0"` |
| `use anyhow::{Context, Result};` | `use report_err::{Context, Result};` |
| `anyhow::Error`, `bail!`, `ensure!`, `format_err!`, `.context`, `.chain()`, `.downcast_ref` | same names |
| `anyhow!(...)` | `err!(...)` |

The rules, a find-and-replace pass, the few differences, and a worked migration of a compiler (36 lines changed, no hand edits, the same test results) are in [`docs/MIGRATING.md`](./docs/MIGRATING.md).

## How it compares

Measured at `0.3.0` with criterion against anyhow 1.0.104 on the same machine (Windows 11, AMD Ryzen 9 9950X3D, system allocator; details and every figure in [`dev/PLAN.md`](./dev/PLAN.md) section 11.4):

| | report-err | anyhow |
|---|---|---|
| `size_of::<Result<(), Error>>()` | 8 bytes | 8 bytes |
| `?` then `.context("...")` | 26.7 ns, 1 allocation | 52.3 ns, 2 allocations |
| create plus 9 contexts | 125.5 ns, 3 allocations | 409.1 ns, 10 allocations |
| drop of an error with 3 contexts | 26.3 ns | 51.6 ns |
| `downcast_ref` to the root after 3 contexts | 1.0 ns | 2.5 ns |
| walk `source()` through 9 layers | 13.7 ns | 16.8 ns |
| `{:#}` of an error with 3 contexts | 54.7 ns | 27.3 ns |

`{:#}` is slower because report-err skips a layer whose text the layer before it already printed, which needs that text before printing; the 0.6 optimisation milestone revisits it.

A `0.4.0` run, on a machine too busy to give absolute figures, kept the leads on context, deep errors, and `downcast_ref`. Creating a plain error is now level with anyhow rather than ahead, because report-err makes the same backtrace check; `{:?}` is 27% slower because the report prints a `Trace:` block anyhow has no equivalent for. The drop comparison was too noisy to read. The 0.6 milestone measures all of it again on a quiet machine.

## Several causes, cleanup, and panics

```rust
use report_err::{boundary, CollectAll, Context, Errors, Result};

let configs: Vec<Config> = paths.iter().map(|p| load(p)).collect_all()?;
// on failure: "2 of 9 items failed", with both load errors as children

let written = write_all(&mut file, &data).with_cleanup(|| file.sync_all())?;
// both failed: the write error, with the sync error suppressed in it

let page = boundary(|| plugin.render(&page)).context("plugin `gallery` failed")?;
// a panic in the plugin becomes an error with exit code 101
```

`boundary` catches a panic only when panics unwind. Under `panic = "abort"` (the usual release profile in this portfolio) the process aborts at the panic, as `std::panic::catch_unwind` documents; `report_err::PANICS_CAUGHT` says which applies.

## What is coming

| What | Version |
|---|---|
| Attack tests, fuzzing, soak runs, `SECURITY.md` | `0.7.0` |

## Documentation

- [`docs/API.md`](./docs/API.md): the public API, with costs and examples.
- [`docs/MIGRATING.md`](./docs/MIGRATING.md): from anyhow, eyre, and snafu's `Whatever`.
- [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md): the modules, the life of an error, and its bounds.
- [`docs/PLATFORM-NOTES.md`](./docs/PLATFORM-NOTES.md): `no_std`, `panic = "abort"`, backtraces, and targets.
- [`docs/UNSAFE.md`](./docs/UNSAFE.md): the one-word representation, its `unsafe` core, and how Miri checks it.
- [`docs/PERFORMANCE.md`](./docs/PERFORMANCE.md): measured against anyhow, eyre, error-stack, and `Box<dyn Error>` on three platforms.
- [`docs/SECURITY.md`](./docs/SECURITY.md): the threats, their defences and tests, the budgets, and how to report a vulnerability.
- [`dev/PLAN.md`](./dev/PLAN.md): the design, the threats and their tests, performance, and the decision log.

<hr>
<br>

## Contributing

See <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for engineering standards and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

<br>

<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b>: <a href="./LICENSE-APACHE">LICENSE-APACHE</a></li>
        <li><b>MIT License</b>: <a href="./LICENSE-MIT">LICENSE-MIT</a></li>
    </ul>
    <p>at your option.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>James Gober.</strong></sup>
</div>

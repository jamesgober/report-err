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
        <strong>Status: <code>0.3.0</code>, the anyhow surface.</strong> <code>Error</code>, <code>Result</code>, <code>Context</code>, <code>err!</code>, <code>bail!</code>, <code>ensure!</code>, downcasting, the cause chain, and the formatting rules are in. Error codes, attachments, sections, the propagation trace, and exit codes arrive in <code>0.4.0</code>; several causes, suppressed errors, budgets, and the panic boundary in <code>0.5.0</code>. The whole API, with the version each item arrives in, is in <a href="./docs/API.md"><code>docs/API.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

## Install

```toml
[dependencies]
report-err = "0.3.0"
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

## Moving from anyhow

| anyhow | report-err |
|---|---|
| `anyhow = "1"` | `report-err = "0.3.0"` |
| `use anyhow::{Context, Result};` | `use report_err::{Context, Result};` |
| `anyhow::Error`, `bail!`, `ensure!`, `format_err!`, `.context`, `.chain()`, `.downcast_ref` | same names |
| `anyhow!(...)` | `err!(...)` |

The rules, a find-and-replace pass, the few differences, and a worked migration of a compiler (36 lines changed, no hand edits, the same test results) are in [`docs/MIGRATING.md`](./docs/MIGRATING.md).

## How it compares

Measured with criterion against anyhow 1.0.104 on the same machine (Windows 11, AMD Ryzen 9 9950X3D, system allocator; details and every figure in [`dev/PLAN.md`](./dev/PLAN.md) section 11.4):

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

## What is coming

| What | Version |
|---|---|
| Error codes, severity, and category through define-err; `ExitReport` with exit codes from the category | `0.4.0` |
| Propagation trace, lazy backtraces, typed attachments, notes, warnings, suggestions | `0.4.0` |
| Several causes, `collect_all()`, `split`, `flatten`, suppressed errors, `with_cleanup` | `0.5.0` |
| Panic boundary, creation hook, reporting through define-err's facade, budgets on every growable part | `0.5.0` |

`boundary` will catch a panic only when panics unwind. Under `panic = "abort"` (the usual release profile in this portfolio) the process aborts at the panic, as `std::panic::catch_unwind` documents; `report_err::PANICS_CAUGHT` will say which applies.

## Documentation

- [`docs/API.md`](./docs/API.md): the public API, with costs and examples.
- [`docs/MIGRATING.md`](./docs/MIGRATING.md): from anyhow, eyre, and snafu's `Whatever`.
- [`docs/UNSAFE.md`](./docs/UNSAFE.md): the one-word representation, its `unsafe` core, and how Miri checks it.
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

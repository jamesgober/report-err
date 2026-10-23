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
        On top of that, an error can carry an error code with its severity, category, and exit code; a propagation trace of every place it passed through; typed attachments read back by type; notes, warnings, and suggestions for the person reading the report; several causes; and suppressed cleanup failures. A panic boundary turns a caught panic into an error, and <code>main</code> can exit with a code chosen from the error. All of it costs nothing until used: an error that only carries its cause and location is one allocation and one machine word, the same as anyhow. No dependencies outside the portfolio, and <code>no_std</code> with <code>alloc</code>.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.81+</strong> (Rust 2021 edition). <code>no_std</code> + <code>alloc</code>; <code>std</code> on by default.
    </p>
    <blockquote>
        <strong>Status: scaffold (<code>0.1.0</code>).</strong> No functional code yet; this release claims the name and sets up the repository, the plan, and CI. The design is in <a href="./dev/PLAN.md"><code>dev/PLAN.md</code></a> and the schedule in <a href="./dev/ROADMAP.md"><code>dev/ROADMAP.md</code></a>. The public API is locked in <code>docs/API.md</code> at <code>0.2.0</code>; the anyhow surface ships in <code>0.3.0</code>.
    </blockquote>
</div>

<hr>
<br>

## Scope

The shape of the Tier 1 API, arriving in `0.3.0`:

```rust
use report_err::{bail, Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {path}"))?;
    if text.is_empty() {
        bail!("config file {path} is empty");
    }
    parse(&text).context("parsing config")
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

| What | From | Version |
|---|---|---|
| `Error`, `Result`, `Context` on `Result` and `Option`, `err!`, `bail!`, `ensure!`, downcasting, `chain()`, `Deref` to `dyn Error` with every layer in `source()` | the anyhow API | `0.3.0` |
| Error codes, severity, category (through define-err), `ExitReport` with exit codes from the category | new | `0.4.0` |
| Propagation trace, lazy backtraces, typed attachments, notes, warnings, suggestions | new | `0.4.0` |
| Several causes, `collect_all()`, `split`, `flatten`, suppressed errors, `with_cleanup` | new | `0.5.0` |
| Panic boundary, creation hook, reporting through define-err's facade, budgets on every growable part | new | `0.5.0` |

`boundary` catches a panic only when panics unwind. Under `panic = "abort"` (the usual release profile in this portfolio) the process aborts at the panic, as `std::panic::catch_unwind` documents; `report_err::PANICS_CAUGHT` says which applies.

Moving from anyhow, once `0.3.0` is out:

| anyhow | report-err |
|---|---|
| `anyhow = "1"` | `report-err = "0.3"` |
| `use anyhow::{Context, Result};` | `use report_err::{Context, Result};` |
| `anyhow::Error`, `bail!`, `ensure!`, `format_err!` | same names |
| `anyhow!(...)` | `err!(...)` |

See [`dev/PLAN.md`](./dev/PLAN.md) for the full design, the formatting rules, the threats and their tests, what is deliberately left to other crates, and the decision log.

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

<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>report-err</b><br>
    <sub><sup>API REFERENCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>API</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="./UNSAFE.md" title="The unsafe core"><b>UNSAFE</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="../dev/PLAN.md" title="Plan"><b>PLAN</b></a>
    </sup>
</div>
<br>

The complete public API of report-err 1.0, written at the `0.2.0` design lock. Every item carries the version it arrives in (**Since**) and its item id from [`../dev/PLAN.md`](../dev/PLAN.md) section 8; the published crate exposes only what is implemented, so an item marked `0.4` does not exist in `0.3`. Signatures here are the contract: a change after `0.2.0` is recorded in the decision log of the plan. Types from define-err (`Code`, `Severity`, `Category`, `Kinds`, `ErrorInfo`, `CodeInfo`, `ErrorRecord`) are the ones its own `docs/API.md` locks.

Costs are estimates (marked [est.]) until they are measured; numbers marked [measured] come from the 0.3.0 benchmarks against anyhow 1.0.104 (criterion; PLAN 11.4) or from the counting-allocator tests (`tests/alloc.rs`). The full comparison with eyre and error-stack, on Linux and aarch64 too, is in `0.6.0`.

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## Table of contents

- [Setup](#setup)
  - [Features](#features)
  - [`no_std`](#no_std)
  - [The prelude](#the-prelude)
- [Conventions](#conventions)
  - [Tiers](#tiers)
  - [Names you already know](#names-you-already-know)
  - [No panics, and the one error this crate returns](#no-panics-and-the-one-error-this-crate-returns)
  - [Threads](#threads)
  - [What can never be observed](#what-can-never-be-observed)
- [Tier 1 on one screen](#tier-1-on-one-screen)
- [`Error`](#error)
  - [Creating an `Error`](#creating-an-error)
  - [`context`](#context)
  - [`Deref` to `dyn Error`: the `source()` chain](#deref-to-dyn-error-the-source-chain)
  - [Downcasting](#downcasting)
  - [Location](#location)
  - [Codes, severity, category, kinds](#codes-severity-category-kinds)
  - [Formatting](#formatting)
- [`Result` and `Ok`](#result-and-ok)
- [`Context` on `Result` and `Option`](#context-on-result-and-option)
- [Macros: `err!`, `bail!`, `ensure!`](#macros-err-bail-ensure)
- [Chain tools](#chain-tools)
  - [`chain` and `Chain`](#chain-and-chain)
  - [`root_cause`](#root_cause)
  - [`find`, `find_all`, `find_in_suppressed`](#find-find_all-find_in_suppressed)
- [Attachments and sections](#attachments-and-sections)
  - [`attach`, `attachment`, `attachments`, `attach_display`](#attach-attachment-attachments-attach_display)
  - [`note`, `warning`, `suggestion`](#note-warning-suggestion)
  - [`sections`, `Section`, `SectionKind`](#sections-section-sectionkind)
- [Several causes](#several-causes)
  - [`group`, `children`, `push_child`](#group-children-push_child)
  - [`split` and `flatten`](#split-and-flatten)
  - [`Errors`](#errors)
  - [`CollectAll`](#collectall)
- [Suppressed errors and cleanup](#suppressed-errors-and-cleanup)
- [Trace and backtrace](#trace-and-backtrace)
- [Configuration](#configuration)
  - [`Limits`](#limits)
  - [`configure` and `limits`](#configure-and-limits)
  - [`configure_catalogs`](#configure_catalogs)
  - [`ConfigureError`](#configureerror)
- [`main`](#main)
  - [`fn main() -> Result<()>`](#fn-main---result)
  - [`ExitReport`](#exitreport)
  - [`exit_code` and `with_exit_code`](#exit_code-and-with_exit_code)
  - [`run`](#run)
- [Panic boundary](#panic-boundary)
- [Reporting](#reporting)
  - [`report` and `is_reported`](#report-and-is_reported)
  - [`record`](#record)
  - [`info`](#info)
  - [The creation hook](#the-creation-hook)
- [Standard-library interop](#standard-library-interop)
- [`render` and `RenderStyle`](#render-and-renderstyle)
- [`testing`](#testing)
- [Crate root](#crate-root)
- [Item index](#item-index)

## Setup

```toml
[dependencies]
report-err = "0.3"
```

### Features

| Feature | Default | Brings |
|---|---|---|
| `std` | yes | backtraces, `ExitReport` and exit codes, `run`, `boundary`, panic isolation in guarded rendering and around the creation hook, the standard-error fallback of `unraisable` |
| `testing` | no | assertions for tests ([`testing`](#testing)) |

### `no_std`

The crate is `#![no_std]` and needs `alloc`: an `Error` lives on the heap. `default-features = false` keeps everything except the items marked **std** below. CI builds `thumbv7em-none-eabihf`. Targets without pointer-sized atomic compare-and-swap (`thumbv6m-none-eabi`) are refused at build time with a message.

```toml
[dependencies]
report-err = { version = "0.3", default-features = false }
```

MSRV is 1.81, the first release with `core::error::Error`.

### The prelude

```rust
use report_err::prelude::*;
```

brings `Error`, `Result`, `Context`, `CollectAll`, `err!`, `bail!`, and `ensure!`. Since `0.3` (ROOT-03; `CollectAll` from `0.5`). The anyhow-shaped import works as well:

```rust
use report_err::{bail, ensure, err, Context, Result};
```

## Conventions

### Tiers

| Tier | What | Examples |
|---|---|---|
| 1 | one import, one type, three macros, two methods | `Result`, `?`, `.context`, `.with_context`, `err!`, `bail!`, `ensure!`, `{}`/`{:#}`/`{:?}`, `fn main() -> Result<()>`, `ExitReport` |
| 2 | the next 15% of uses | codes and categories, attachments, sections, `chain`, `find`, groups, `Errors`, `collect_all`, suppressed errors, `trace`, `configure`, `report`, `record`, `boundary` |
| 3 | the full surface for other libraries | `find_all`, `push_child`, `split`, `flatten`, `render`, `from_info`, `info`, `from_panic`, `limits` |

### Names you already know

Every name anyhow users write is the same here, with the same meaning: `Error`, `Result`, `Context` (`context`, `with_context`), `bail!`, `ensure!`, `format_err!`, `Ok`, `Error::new`, `Error::msg`, `chain`, `root_cause`, `downcast`, `downcast_ref`, `downcast_mut`, `is`, `backtrace`, and an `Error` that dereferences to `dyn std::error::Error` with every context layer in its `source()` chain. anyhow's `anyhow!` is `err!` here. The differences that remain are listed in [`../dev/PLAN.md`](../dev/PLAN.md) 8.14.2.

### No panics, and the one error this crate returns

No function in this crate panics because of its own state or input. A panic can come only from code the caller supplies (a `Display`, a `Drop`, a closure) and, outside the guarded paths ([Formatting](#formatting)), it propagates as it would anywhere else. When a budget is reached ([`Limits`](#limits)), data is dropped and counted, never refused with an error and never printed unbounded. The crate's own error type is [`ConfigureError`](#configureerror). Allocation failure aborts the process, as everywhere in `alloc`.

### Threads

`Error` is `Send + Sync + Unpin + UnwindSafe + RefUnwindSafe + 'static`. Every `&self` method only reads, except `report`, which sets an atomic flag, so a `&Error` (for example in an `Arc`) can be read and reported from several threads; exactly one `report` returns `true`. Every method that changes an error takes `self` or `&mut self`. Context values, attachments, and children must be `Send + Sync`.

### What can never be observed

Errors are values in Rust; there is no throw event. report-err sees an error when an `Error` is created (constructors and the type-changing `?`), when context or a trace point is added, when `report` is called, at a `boundary`, and when `main` returns. It never sees an error discarded with `let _ =`, `.ok()`, or `unwrap_or`; an error handled inside third-party code; a `?` that does not change the error type ([`trace`](#trace-and-backtrace) records that line instead); or a panic under `panic = "abort"`.

## Tier 1 on one screen

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
    let config = load_config("app.noml")?;
    run(config)
}
```

When `main` returns the error, the process prints the report and exits with code 1:

```text
Error: reading config file app.noml

Caused by:
    No such file or directory (os error 2)

Location: src/main.rs:4:10
```

With codes from define-err and `ExitReport`, the exit code comes from the error ([`main`](#main)):

```rust
fn main() -> report_err::ExitReport {
    run().into()
}
```

## `Error`

```rust
pub struct Error { /* one pointer */ }
```

One type for any error. Since `0.3` (ERR-01).

| Property | Value |
|---|---|
| Size | one machine word: `size_of::<Error>()`, `size_of::<Result<(), Error>>()`, and `size_of::<Option<Error>>()` all equal `size_of::<usize>()` (asserted at compile time) |
| Heap | one allocation holding a 72-byte header (on 64-bit targets in 0.3; it grows when codes and flags join it in 0.4 and 0.5) and the root error inline; a second block only when a second context layer, an attachment, a section, a child, or a trace point is added |
| Traits | `Debug`, `Display`, `Deref<Target = dyn Error + Send + Sync + 'static>`, `DerefMut`, `AsRef<dyn Error + Send + Sync + 'static>`, `AsRef<dyn Error + 'static>`, `From<E>` for every `E: core::error::Error + Send + Sync + 'static`, `Send`, `Sync`, `Unpin`, `UnwindSafe`, `RefUnwindSafe` |
| Not | `Clone` (an error is owned by one call path); `core::error::Error` itself (it could not and still convert from every error through `?`: [`Deref`](#deref-to-dyn-error-the-source-chain) gives the `&dyn Error`) |
| `#[must_use]` | `Error` and every method that returns an `Error` or a `Result` |

### Creating an `Error`

```rust
impl<E> From<E> for Error where E: core::error::Error + Send + Sync + 'static;   // #[track_caller]

impl Error {
    #[track_caller]
    pub fn new<E>(error: E) -> Error where E: core::error::Error + Send + Sync + 'static;
    #[track_caller]
    pub fn msg<M>(message: M) -> Error where M: Display + Debug + Send + Sync + 'static;
}
```

| Id | Item | Since | Behaviour | Cost |
|---|---|---|---|---|
| ERR-02 | `From<E>` | 0.3 | what `?` calls; the root is `error`; the location is the `?` | one allocation [est. within 5% of anyhow] |
| ERR-03 | `Error::new` | 0.3 | as `From`, written out | one allocation |
| ERR-04 | `Error::msg` | 0.3 | the root is a message; `downcast_ref::<M>()` returns it | one allocation; a `&'static str` is stored, never copied |

Other constructors: [`err!`](#macros-err-bail-ensure), [`group`](#group-children-push_child), [`from_boxed`](#standard-library-interop), [`from_info`](#info), [`from_panic`](#panic-boundary).

```rust
use report_err::Error;

let a = Error::new(std::io::Error::other("disk full"));
let b = Error::msg("no input files");
let c = Error::msg(format!("{count} files failed"));

fn read(path: &str) -> report_err::Result<String> {
    Ok(std::fs::read_to_string(path)?)           // From<io::Error>
}
```

`?` on a `Result<_, Box<dyn Error + Send + Sync>>` does not compile, as with anyhow (a boxed error is not itself an `Error` type); the compiler's note points to [`from_boxed`](#standard-library-interop).

### `context`

```rust
impl Error {
    #[track_caller]
    pub fn context<C>(self, context: C) -> Error where C: Display + Send + Sync + 'static;
}
```

Adds a layer: `context` becomes the outermost message, and the error that was outermost becomes its source. Since `0.3` (ERR-05).

- A `&'static str` is stored as is and a `String` is moved in; neither is copied. Any other `C` is boxed and stays downcastable as `C`.
- The first layer lives in the error's own allocation and costs nothing; the second allocates the extension block once; layers three to five cost nothing more; later layers grow a vector [measured: one allocation in all for 1 layer, two for 3 or 5 layers, three for 9; anyhow: 2, 4, 6, 10]. Create plus 3 layers took 65.6 ns, anyhow 110.5 ns; plus 9 layers 125.5 ns, anyhow 409.1 ns [measured].
- The number of layers is bounded by [`Limits::max_frames`](#limits) (64): past it, the oldest middle layers are dropped and counted, never the innermost or the newest.

```rust
let error = error.context("while compacting level 2");
```

On a `Result` or an `Option`, the same method comes from the [`Context`](#context-on-result-and-option) trait.

### `Deref` to `dyn Error`: the `source()` chain

```rust
impl Deref for Error { type Target = dyn core::error::Error + Send + Sync + 'static; }
impl DerefMut for Error;
impl AsRef<dyn core::error::Error + Send + Sync + 'static> for Error;
impl AsRef<dyn core::error::Error + 'static> for Error;
```

An `Error` works wherever a `&dyn Error` is expected, as anyhow's does. Since `0.3` (ERR-19, PLAN D-003).

- The target is the outermost layer: the newest context, or the root error itself when there is no context.
- Its `source()` is the next layer inward, and so on through every context layer to the root error, then the root error's own `source()` chain. Walking `source()` by hand visits exactly the links [`chain`](#chain-and-chain) visits, in the same order.
- No allocation. A hand-written `source()` walk costs one load and one branch per link: 2.93 / 5.64 / 8.07 / 13.69 ns for 1 / 3 / 5 / 9 layers, anyhow 2.98 / 6.35 / 9.85 / 16.78 ns [measured]. `chain()` walks the error's own store directly and is as fast or faster (9 layers: 9.3 ns, anyhow 18.4 ns) [measured].

```rust
fn log_cause(error: &(dyn std::error::Error + 'static)) {
    let mut cause = error.source();
    while let Some(c) = cause {
        eprintln!("  because: {c}");
        cause = c.source();
    }
}

let error = load().context("starting server").unwrap_err();
log_cause(&*error);                                   // every layer, then the io::Error
log_cause(error.as_ref());                            // the same
let source = error.source();                          // auto-deref: the layer below "starting server"
```

`(&*error).downcast_ref::<T>()` asks the outermost layer only, as with anyhow; use [`Error::downcast_ref`](#downcasting), which searches every layer and the root.

### Downcasting

```rust
impl Error {
    pub fn downcast_ref<T>(&self) -> Option<&T> where T: Display + Debug + Send + Sync + 'static;
    pub fn downcast_mut<T>(&mut self) -> Option<&mut T> where T: Display + Debug + Send + Sync + 'static;
    pub fn downcast<T>(self) -> core::result::Result<T, Error> where T: Display + Debug + Send + Sync + 'static;
    pub fn is<T>(&self) -> bool where T: Display + Debug + Send + Sync + 'static;
}
```

| Id | Method | Tier | Since |
|---|---|---|---|
| ERR-06 | `downcast_ref` | 1 | 0.3 |
| ERR-07 | `downcast_mut` | 2 | 0.3 |
| ERR-08 | `downcast` | 2 | 0.3 |
| ERR-09 | `is` | 1 | 0.3 |

Each searches the context values, newest first, then the root error, and finds a value whose type is exactly `T`, as anyhow does. A message root (`Error::msg`, `err!`) answers to its message type: `&'static str` for a literal, `String` for a formatted message. A string context layer answers to `&'static str` or `String`. Errors deeper in the root's `source()` chain are not searched: use [`find`](#find-find_all-find_in_suppressed).

`downcast` moves the value out and frees the rest; on a miss it returns the error unchanged (same allocation). Cost: 1.01 ns to the root after three string layers, anyhow 2.47 ns [measured]; the layers are skipped when no layer could hold a `T`. No allocation.

```rust
if let Some(io) = error.downcast_ref::<std::io::Error>() {
    if io.kind() == std::io::ErrorKind::NotFound {
        create_default()?;
    }
}

#[derive(Debug)]
struct ShardId(u32);
impl std::fmt::Display for ShardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "shard {}", self.0) }
}
let error = load_shard(7).context(ShardId(7)).unwrap_err();
assert_eq!(error.downcast_ref::<ShardId>().map(|s| s.0), Some(7));
let io: std::io::Error = error.downcast().expect("the root was an io::Error");
```

### Location

```rust
impl Error {
    pub fn location(&self) -> &'static core::panic::Location<'static>;
}
```

Where the error was created: the `?`, `Error::new`, `err!`, `bail!`, or `ensure!` that made it (`#[track_caller]`, no allocation). Since `0.3` (ERR-10). The way it travelled is the [trail](#trace-and-backtrace).

```rust
eprintln!("created at {}", error.location());   // src/store.rs:88:21
```

### Codes, severity, category, kinds

```rust
use define_err::{Code, Severity, Category, Kinds};

impl Error {
    pub fn code(&self) -> Option<Code>;
    pub fn coded(self, code: Code) -> Error;
    pub fn severity(&self) -> Severity;
    pub fn category(&self) -> Category;
    pub fn kinds(&self) -> Kinds;
    pub fn with_severity(self, severity: Severity) -> Error;
    pub fn with_kind(self, kinds: Kinds) -> Error;
    pub fn public_message(&self) -> Option<&'static str>;
    pub fn is_panic(&self) -> bool;
}
```

| Id | Method | Tier | Since | Answer |
|---|---|---|---|---|
| ERR-11 | `code` | 1 | 0.4 | the explicit code (`coded`, a code-first macro); else the innermost coded error in the chain, read through define-err's `innermost_code`; else `None` |
| ERR-12 | `coded` | 1 | 0.4 | sets the explicit code, replacing an earlier one |
| ERR-13 | `severity` | 2 | 0.4 | `with_severity`; else the coded error's identity; else, for an explicit code, its catalog entry ([`configure_catalogs`](#configure_catalogs)); else `FATAL` (21) for a panic; else the category's default severity |
| ERR-13 | `category` | 2 | 0.4 | the coded error's identity; else the catalog entry of an explicit code; else `Category::from_io_kind` when the root is a `std::io::Error` (**std**); else `Internal` for a panic; else `Unknown` |
| ERR-13 | `kinds` | 2 | 0.4 | `with_kind` bits, OR the coded error's kinds, OR the creation hook's kinds, OR `Kinds::PANIC` for a panic; `Kinds::ERROR` when empty |
| ERR-14 | `with_severity`, `with_kind` | 2 | 0.4 | overrides; kinds are added with OR |
| ERR-15 | `public_message` | 2 | 0.4 | the public message of the code: from the catalog entry, or from the root's `ErrorInfo` when the error was built with `from_info`; when only the identity is known, the category's default public message; `None` when there is no code |
| ERR-16 | `is_panic` | 2 | 0.5 | created by `boundary` or `from_panic` |

Context layers never carry a code; children are not consulted (a group has its own code or none). `Display` of an error is a private message for logs; `public_message` and the category are the only parts meant for a client.

Cost [est.]: `code` under 60 ns for a five-link chain (define-err's probe, about 10 ns per generated link); `coded`, `with_severity`, `with_kind` one store.

```rust
use report_err::{bail, Context, Result};

fn load_user(id: u64) -> Result<User> {
    let row = db.get(id).coded(DbError::NOT_FOUND)?;          // Option or Result
    if !row.active {
        bail!(UserError::INACTIVE, "user {id} is inactive");
    }
    Ok(row.into())
}

match load_user(id) {
    Err(e) if e.code() == Some(DbError::TIMEOUT) => retry_later(),
    Err(e) => return Err(e.context("in the account page")),
    Ok(user) => user,
}

let (status, text) = (error.category().http_status(), error.public_message().unwrap_or("error"));
```

### Formatting

```rust
impl Display for Error;
impl Debug for Error;
```

Since `0.3` (ERR-17); the blocks after `Location:` from `0.4` and `0.5`.

For an error created by

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

The `{:?}` report, block by block, each printed only when it has content: the outermost message; `Caused by:` (one unnumbered line for one cause, numbered as anyhow does for several); `Causes (n):` the children as a tree; `Note:`, `Warning:`, `Suggestion:` lines in the order added; `Suppressed:`; `Code: DB-0042 (unavailable, severity 17)`; `Location:`; `Trace:` when there is more than the origin; `Attachments:` (only those added with `attach_display`); `Fields:` (from the creation hook); `Omitted:` (what a budget dropped); `Backtrace:` when one was captured.

- **De-duplication.** In `{:#}` and in `Caused by:`, a layer is skipped when the layer before it already ends with `": "` followed by exactly its text, or is exactly its text, so an error whose `Display` already includes its source is not printed twice. Only adjacent layers are compared.
- **Flags.** Width, fill, and precision are ignored; only `#` changes the output.
- **Cost.** `{}`, `{:#}`, and `{:?}` do not allocate (`{:?}` may, from `0.4`, to resolve backtrace symbols) [measured by `tests/alloc.rs`]. For an error with three layers: `{}` 7.5 ns (anyhow 8.3), `{:#}` 54.7 ns (anyhow 27.3: the de-duplication rule formats the root into a buffer to compare it), `{:?}` 106.8 ns (anyhow 92.4, which prints no `Location:`) [measured].
- **Guarded paths.** `{:?}` through [`ExitReport`](#exitreport), [`report`](#report-and-is_reported), [`record`](#record), [`render`](#render-and-renderstyle), and `unraisable` survive a `Display` that panics (with `std` and unwinding), fails, or never stops writing: the layer prints `<display panicked>` or `<display failed>`, and output stops at [`Limits::max_render`](#limits) with `(report truncated)`. `{}` and `{:#}` are not guarded; they behave like any other `Display`.

## `Result` and `Ok`

```rust
pub type Result<T, E = Error> = core::result::Result<T, E>;

pub fn Ok<T>(value: T) -> Result<T>;
```

| Id | Item | Tier | Since |
|---|---|---|---|
| ROOT-02 | `Result` | 1 | 0.3 |
| ROOT-04 | `Ok` | 1 | 0.4 |

`Result<T, MyError>` still works where a typed error is wanted. `Ok` is anyhow's helper for closures and blocks whose error type cannot be inferred; it is not in the prelude, because a glob import would shadow `core::result::Result::Ok` in patterns.

```rust
let parse = |s: &str| -> report_err::Result<u32> { Ok(s.trim().parse()?) };
let value = (|| { let n: u32 = "7".parse()?; report_err::Ok(n * 2) })()?;
```

## `Context` on `Result` and `Option`

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
    fn with_cleanup<F, R>(self, cleanup: F) -> Result<T, Error>
        where F: FnOnce() -> core::result::Result<(), R>, R: Into<Error>;
}

impl<T, E> Context<T, E> for core::result::Result<T, E> where E: core::error::Error + Send + Sync + 'static;
impl<T> Context<T, Error> for core::result::Result<T, Error>;
impl<T> Context<T, core::convert::Infallible> for Option<T>;
```

The trait is sealed, as anyhow's is, so the two `Result` impls can never overlap. Every method is `#[track_caller]`. On `Ok` and `Some` every method returns the value untouched and no closure runs; nothing is allocated.

| Id | Methods | Tier | Since | Notes |
|---|---|---|---|---|
| CTX-01 | `context`, `with_context` on `Result` | 1 | 0.3 | `io_result.context("literal")?` is one allocation, the same as anyhow; the closure of `with_context` runs only on `Err` |
| CTX-02 | `context`, `with_context` on `Option` | 1 | 0.3 | `None` becomes an error whose root is the context message |
| CTX-03 | `coded` | 1 | 0.4 | on `None`: an error with that code whose message is the code's catalog title when [catalogs](#configure_catalogs) are configured, else the code itself (`DB-0042`) |
| CTX-04 | `note`, `warning`, `suggestion` and their `with_` forms | 1 | 0.4 | [sections](#note-warning-suggestion) |
| CTX-05 | `attach` | 2 | 0.4 | [attachments](#attach-attachment-attachments-attach_display) |
| CTX-06 | `trace` | 2 | 0.4 | [trace points](#trace-and-backtrace) |
| CTX-07 | `with_cleanup` | 2 | 0.5 | [cleanup](#suppressed-errors-and-cleanup) |

```rust
use report_err::Context;

let text = std::fs::read_to_string(path).context("reading the config file")?;
let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
let home = std::env::var_os("HOME").context("HOME is not set")?;
let parent = path.parent().context("entry file has no parent directory")?;   // was .ok_or_else(|| anyhow!(..))
let user = cache.get(&id).coded(UserError::NOT_FOUND)?;
let bytes = read_blob(id)
    .note(format!("blob {id} is referenced by {count} pages"))
    .suggestion("run `hive repair --blobs`")?;
let plan = optimise(plan).trace()?;
```

## Macros: `err!`, `bail!`, `ensure!`

```rust
err!(...)          // an Error
format_err!(...)   // the same as err!, under anyhow's and eyre's other name
bail!(...)         // return Err(err!(...))
ensure!(cond, ...) // if !cond { bail!(...) }
```

| Id | Item | Tier | Since |
|---|---|---|---|
| MAC-01 | `err!` (and `format_err!`) | 1 | 0.3 |
| MAC-02 | `bail!` | 1 | 0.3 |
| MAC-03 | `ensure!` | 1 | 0.3 |
| MAC-04 | code-first arms of all three | 1 | 0.4 |

Arms, tried in this order (shown for `err!`):

| Arm | Example | Result |
|---|---|---|
| a string literal | `err!("no input")` | a message root; a literal with no `{}` arguments is stored as `&'static str` (no allocation beyond the error itself) |
| a format string with arguments | `err!("bad port {}", p)`, `err!("bad port {p}")` | a formatted message root (`String`) |
| a code, then a format string | `err!(DbError::TIMEOUT, "after {ms} ms")` | as above, with the code set; the first argument must be a define-err `Code` (a type error otherwise, with a note saying so) |
| any other expression | `err!(io_error)`, `err!(message_string)` | an error value becomes the root (`Error::new`); any other `Display + Debug` value becomes a message root (`Error::msg`); chosen at compile time |

`ensure!(cond)` with no message produces `condition failed: ` followed by the condition's source text. A condition that contains braces is printed literally, never read as a format string. Inline captures (`{name}`) work in every format arm.

Cost: `err!("literal")` and `bail!("literal")` one allocation; `err!("value {x}")` two (the `String` and the error) [est., as anyhow].

```rust
use report_err::{bail, ensure, err, Result};

fn check_header(h: &Header) -> Result<()> {
    ensure!(h.magic == MAGIC, "bad magic {:#x}", h.magic);
    ensure!(h.version <= 3, FormatError::UNSUPPORTED_VERSION, "version {} is newer than 3", h.version);
    ensure!(h.len > 0);                                  // "condition failed: h.len > 0"
    if h.flags & RESERVED != 0 {
        bail!("reserved flags set: {:#b}", h.flags & RESERVED);
    }
    Ok(())
}

let error = err!("unexpected token {token:?}");
let sigs = load_signatures().map_err(|e| err!(e)).context("invalid FFI signature table")?;
```

## Chain tools

### `chain` and `Chain`

```rust
impl Error {
    pub fn chain(&self) -> Chain<'_>;
}

#[derive(Clone)]
pub struct Chain<'a> { /* private */ }
impl<'a> Iterator for Chain<'a> { type Item = &'a (dyn core::error::Error + 'static); }
impl DoubleEndedIterator for Chain<'_>;
impl ExactSizeIterator for Chain<'_>;
impl FusedIterator for Chain<'_>;
```

| Id | Item | Tier | Since |
|---|---|---|---|
| CHN-01 | `chain`, `Chain` | 1 | 0.3 |
| CHN-06 | `DoubleEndedIterator` (`rev`) | 3 | 0.5 |

Yields, outermost first: every context layer (newest to oldest), the root error, then the root's own `source()` chain. These are exactly the links a hand-written `source()` walk from `&*error` visits; `chain()` reads the context layers from the error's own store instead of following `source()` [measured: 9.3 ns for nine layers and the root, anyhow 18.4 ns]. Bounded by [`Limits::max_chain`](#limits) (128): a `source()` that returns itself or never ends stops there, and `len()` stays exact. No allocation; `rev` keeps a bounded buffer on the stack.

```rust
for (index, cause) in error.chain().enumerate() {
    if index > 0 { combined.push_str("\ncaused by: "); }
    combined.push_str(&cause.to_string());
}
```

### `root_cause`

```rust
impl Error {
    pub fn root_cause(&self) -> &(dyn core::error::Error + 'static);
}
```

The last link of the chain. Since `0.3` (CHN-02).

```rust
log::error!("{error} (root: {})", error.root_cause());
```

### `find`, `find_all`, `find_in_suppressed`

```rust
impl Error {
    pub fn find<T: core::error::Error + 'static>(&self) -> Option<&T>;
    pub fn find_all<T: core::error::Error + 'static>(&self) -> FindAll<'_, T>;
    pub fn find_in_suppressed<T: core::error::Error + 'static>(&self) -> Option<&T>;
}
```

| Id | Method | Tier | Since |
|---|---|---|---|
| CHN-03 | `find` | 2 | 0.5 |
| CHN-04 | `find_all` | 3 | 0.5 |
| CHN-05 | `find_in_suppressed` | 3 | 0.5 |

`find` searches the whole tree, depth first: the chain of this error, then each child's tree. Suppressed errors are not causes and are searched only by `find_in_suppressed`. The walk is iterative and bounded by `max_depth` and `max_chain`, so a deep tree cannot overflow the stack. No allocation beyond a bounded stack.

```rust
if let Some(io) = error.find::<std::io::Error>() {
    eprintln!("I/O failure: {}", io.kind());
}
let timeouts = error.find_all::<DbError>().filter(|e| matches!(e, DbError::Timeout { .. })).count();
```

## Attachments and sections

### `attach`, `attachment`, `attachments`, `attach_display`

```rust
impl Error {
    pub fn attach<A: Send + Sync + 'static>(self, value: A) -> Error;
    pub fn attachment<A: 'static>(&self) -> Option<&A>;
    pub fn attachments<A: 'static>(&self) -> Attachments<'_, A>;   // Iterator<Item = &A>, newest first
    pub fn attach_display<A: Display + Send + Sync + 'static>(self, value: A) -> Error;
}
```

| Id | Method | Tier | Since |
|---|---|---|---|
| ATT-01 | `attach` | 2 | 0.4 |
| ATT-02 | `attachment` | 2 | 0.4 |
| ATT-03 | `attachments` | 3 | 0.4 |
| ATT-04 | `attach_display` | 2 | 0.4 |

Any value, read back by type on stable Rust. Attachments are **opaque**: never printed, never reported as text, unless added with `attach_display`, which also prints them under `Attachments:`. Bounded by `max_attachments` (32) and, for the value's own size, `max_bytes`; past either, the new value is dropped and counted. Cost [est.]: one allocation for the value, plus the extension block if it does not exist yet.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequestId(u64);

fn handle(req: &Request) -> Result<Response> {
    let order = load_order(req.order).attach(RequestId(req.id))?;
    Ok(render(order))
}

if let Err(error) = handle(&req) {
    let id = error.attachment::<RequestId>().copied();
    respond_error(id, error.category());
    error.report();
}
```

### `note`, `warning`, `suggestion`

```rust
impl Error {
    pub fn note<S: Display + Send + Sync + 'static>(self, text: S) -> Error;
    pub fn warning<S: Display + Send + Sync + 'static>(self, text: S) -> Error;
    pub fn suggestion<S: Display + Send + Sync + 'static>(self, text: S) -> Error;
    pub fn with_note<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> Error;
    pub fn with_warning<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> Error;
    pub fn with_suggestion<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> Error;
}
```

Text for the person reading the report, printed under the cause list in the order added. Since `0.4` (ATT-05). A `&'static str` is stored as is; anything else is formatted once, when added, into bounded text (`max_bytes`, cut at a character boundary with `...(truncated)`), so a section can never panic while a report is printed.

```rust
let error = error
    .note("the cache was rebuilt 3 times in the last minute")
    .warning("data written after 12:04 may be missing")
    .suggestion("check disk health with `smartctl -a /dev/nvme0`");
```

### `sections`, `Section`, `SectionKind`

```rust
impl Error {
    pub fn sections(&self) -> Sections<'_>;              // Iterator<Item = Section<'_>>, in order added
}

#[non_exhaustive]
pub struct Section<'a> { pub kind: SectionKind, pub text: &'a str }

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum SectionKind { Note, Warning, Suggestion }
```

Since `0.4` (ATT-06, ATT-07). For renderers that print sections their own way.

```rust
for section in error.sections() {
    match section.kind {
        SectionKind::Suggestion => eprintln!("hint: {}", section.text),
        _ => eprintln!("{}", section.text),
    }
}
```

## Several causes

### `group`, `children`, `push_child`

```rust
impl Error {
    #[track_caller]
    pub fn group<M, I>(message: M, errors: I) -> Error
        where M: Display + Debug + Send + Sync + 'static, I: IntoIterator<Item = Error>;
    pub fn children(&self) -> &[Error];
    pub fn push_child(&mut self, child: impl Into<Error>) -> bool;
}
```

| Id | Method | Tier | Since |
|---|---|---|---|
| GRP-01 | `group` | 2 | 0.5 |
| GRP-02 | `children` | 2 | 0.5 |
| GRP-03 | `push_child` | 3 | 0.5 |

An error made of several failures (a validation run that failed in twelve places, three of ten shards). The tree is bounded in total size (`max_tree`, 1,024 errors including nested children) and depth (`max_depth`, 16); a child whose tree does not fit is refused as a whole, counted, and `push_child` returns `false`.

```rust
let error = Error::group("3 of 10 shards failed", shard_errors);
for child in error.children() {
    eprintln!("  {child:#}");
}
```

### `split` and `flatten`

```rust
impl Error {
    pub fn split<P: FnMut(&Error) -> bool>(self, predicate: P) -> (Option<Error>, Option<Error>);
    pub fn flatten(self) -> Vec<Error>;
}
```

Since `0.5` (GRP-04, GRP-05). `split` divides a group's children into those that match and those that do not; each half keeps the group's message, code, and sections, and an empty half is `None` (Python's `ExceptionGroup.split`). `flatten` returns the leaves of the tree, depth first.

```rust
let (retryable, permanent) = error.split(|e| e.category().is_transient());
for leaf in permanent.map(Error::flatten).unwrap_or_default() {
    eprintln!("{leaf:#}");
}
```

### `Errors`

```rust
#[derive(Default)]
pub struct Errors { /* private */ }
impl Errors {
    pub fn new() -> Errors;
    pub fn push(&mut self, error: impl Into<Error>);
    pub fn push_result<T, E: Into<Error>>(&mut self, result: core::result::Result<T, E>) -> Option<T>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    #[track_caller]
    pub fn finish<M: Display + Debug + Send + Sync + 'static>(self, message: M) -> Result<()>;
    #[track_caller]
    pub fn finish_with<T, M: Display + Debug + Send + Sync + 'static>(self, value: T, message: M) -> Result<T>;
    #[track_caller]
    pub fn into_error<M: Display + Debug + Send + Sync + 'static>(self, message: M) -> Option<Error>;
}
```

Gathers failures, then turns them into one error, or `Ok` when nothing failed. Since `0.5` (GRP-06). `len` is the exact number of failures pushed, also past the budget; the stored list is bounded by `max_tree`.

```rust
use report_err::{err, Context, Errors, Result};

fn validate(user: &NewUser) -> Result<()> {
    let mut errors = Errors::new();
    if user.name.trim().is_empty() { errors.push(err!("name is empty")); }
    if !user.email.contains('@') { errors.push(err!("email {:?} has no @", user.email)); }
    let age = errors.push_result(user.age.parse::<u8>().context("age is not a number"));
    if let Some(age) = age { if age < 13 { errors.push(err!("age {age} is under 13")); } }
    errors.finish("invalid sign-up form")
}
```

### `CollectAll`

```rust
pub trait CollectAll<T, E>: Iterator<Item = core::result::Result<T, E>> {
    #[track_caller]
    fn collect_all<B: FromIterator<T>>(self) -> Result<B>;
}
impl<I, T, E> CollectAll<T, E> for I where I: Iterator<Item = core::result::Result<T, E>>, E: Into<Error>;
```

Collects every failure, not only the first. Since `0.5` (GRP-07). Consumes the whole iterator; `Ok(collection)` when every item succeeded, else a group whose message is `"{failed} of {total} items failed"` and whose children are the failures in order (bounded by `max_tree`, counts exact).

```rust
use report_err::CollectAll;

let configs: Vec<Config> = paths.iter().map(|p| load(p)).collect_all()?;
// on failure: "2 of 9 items failed", with both load errors as children
```

## Suppressed errors and cleanup

```rust
impl Error {
    pub fn suppress(self, other: impl Into<Error>) -> Error;
    pub fn suppressed(&self) -> &[Error];
}

pub fn unraisable(error: Error) -> bool;

// on Result, through Context:
fn with_cleanup<F, R>(self, cleanup: F) -> Result<T, Error>
    where F: FnOnce() -> core::result::Result<(), R>, R: Into<Error>;
```

| Id | Item | Tier | Since |
|---|---|---|---|
| SUP-01 | `suppress` | 2 | 0.5 |
| SUP-02 | `suppressed` | 2 | 0.5 |
| SUP-03 | `unraisable` | 2 | 0.5 |
| SUP-04 | `with_cleanup` | 2 | 0.5 |

A suppressed error is a failure that happened while handling another one (Java's `addSuppressed`). It is shown under `Suppressed:` but is not a cause: `find`, `code`, and `downcast` ignore it. Bounded by `max_suppressed` (16).

`with_cleanup` runs the cleanup either way and returns the main error with the cleanup's error suppressed in it when both fail, the cleanup's error when only the cleanup fails, and the main result otherwise.

`unraisable` is for an error with nowhere to go (in `Drop`, in a thread nobody joins): it reports it through define-err's facade as unhandled, and returns `true` if a reporter received it. With no reporter installed and `std` on, it writes one guarded, escaped line to standard error, so the error is never lost silently.

```rust
if let Err(error) = tx.apply(batch) {
    let error = match tx.rollback() {
        Ok(()) => error,
        Err(rollback) => error.suppress(rollback),
    };
    return Err(error.context("applying batch"));
}

let written = write_all(&mut file, &data).with_cleanup(|| file.sync_all())?;

impl Drop for Segment {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            report_err::unraisable(Error::new(e).context("flushing segment on drop"));
        }
    }
}
```

## Trace and backtrace

```rust
impl Error {
    #[track_caller]
    pub fn trace(self) -> Error;
    pub fn trail(&self) -> Trail<'_>;
    pub fn backtrace(&self) -> Option<&std::backtrace::Backtrace>;     // std
    pub fn with_backtrace(self) -> Error;                               // std
}

pub struct Trail<'a> { /* private */ }
impl<'a> Iterator for Trail<'a> { type Item = Step<'a>; }
impl Trail<'_> { pub fn omitted(&self) -> u32; }

#[non_exhaustive]
pub struct Step<'a> {
    pub location: &'static core::panic::Location<'static>,
    pub kind: StepKind,
    pub context: Option<&'a (dyn core::error::Error + 'static)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum StepKind { Origin, Context, Trace }
```

| Id | Item | Tier | Since |
|---|---|---|---|
| TRC-01 | `trace` (and `Context::trace`) | 2 | 0.4 |
| TRC-02 | `trail`, `Trail`, `Step` | 2 | 0.4 |
| TRC-03 | `StepKind` | 3 | 0.4 |
| TRC-04 | `backtrace` | 2 | 0.4 |
| TRC-05 | `with_backtrace` | 2 | 0.4 |

The **trail** answers "which way did this error travel": the origin, every context layer's location, and every `.trace()` point, in the order they happened. It is cheaper than a backtrace and present in release builds. A `?` that does not change the error type cannot be recorded (Rust calls the identity `From`, which cannot be tracked), so `.trace()` exists to record that line: one store, plus the extension block the first time. Trace points keep the first 4 and the latest 11, with a count of those dropped between (`omitted`).

**Backtraces** are captured only when the root error is created, only when `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE` enables them (std's rules; `RUST_LIB_BACKTRACE=0` turns them off for errors only), or when `with_backtrace` forces one; symbols are resolved only when printed. Context never captures one. Cost [measured on the design machine]: 1.8 ns for the disabled check, about 0.8 us to capture, about 357 us to resolve and print.

```rust
for step in error.trail() {
    println!("{:?} at {}", step.kind, step.location);
}
// Origin at src/store.rs:88:21
// Context at src/store.rs:88:30
// Trace at src/engine.rs:204:41
// Context at src/main.rs:17:22

let error = err!("invariant broken in compaction").with_backtrace();
```

## Configuration

### `Limits`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub struct Limits {
    pub max_frames: u32,        // 64        (4 to 4,096)
    pub max_attachments: u32,   // 32        (0 to 1,024)
    pub max_sections: u32,      // 32        (0 to 1,024)
    pub max_suppressed: u32,    // 16        (0 to 1,024)
    pub max_tree: u32,          // 1,024     (1 to 1,048,576)
    pub max_depth: u32,         // 16        (1 to 64)
    pub max_bytes: u32,         // 65,536    (1,024 to 16,777,216)
    pub max_chain: u32,         // 128       (8 to 4,096)
    pub max_render: u32,        // 1,048,576 (4,096 to 67,108,864)
}
impl Limits {
    pub const DEFAULT: Limits;
}
impl Default for Limits;
```

Every growable part of an error has a budget, safe for servers by default. Since `0.5` (CFG-01). Change a copy, as with define-err's `RecordLimits`:

| Field | Bounds | At the limit |
|---|---|---|
| `max_frames` | context layers per error | keep the innermost 8 and the newest; drop the oldest of the middle; count |
| `max_attachments`, `max_sections`, `max_suppressed` | per error | refuse the new one; count |
| `max_tree` | errors in one tree, nested children included | refuse the child as a whole; count (`Errors::len` stays exact) |
| `max_depth` | nesting of children | refuse a child that would exceed it; count |
| `max_bytes` | text and shallow value bytes held by one error's extension block | cut text at a character boundary with `...(truncated)`; refuse values; count |
| `max_chain` | links walked in one chain | stop; print `(chain truncated)` |
| `max_render` | bytes written by one guarded rendering | stop; print `(report truncated)` |

Trace points are fixed at 15 (the first 4 and the latest 11). Budgets are checked before the allocation or the work; what was dropped is printed under `Omitted:`.

### `configure` and `limits`

```rust
pub fn configure(limits: Limits) -> core::result::Result<(), ConfigureError>;
pub fn limits() -> &'static Limits;
```

| Id | Item | Tier | Since |
|---|---|---|---|
| CFG-02 | `configure` | 2 | 0.5 |
| CFG-03 | `limits` | 3 | 0.5 |

Write once, at the start of `main`, before other threads create errors. Every field is checked against its range first; one bad field refuses the whole value and the defaults stay (fail closed). A second call returns `AlreadyConfigured`. Reading costs one acquire load [est. under 1 ns]; there is no lock anywhere.

```rust
let mut limits = report_err::Limits::DEFAULT;
limits.max_frames = 32;
limits.max_bytes = 16 * 1024;
report_err::configure(limits)?;
```

### `configure_catalogs`

```rust
pub fn configure_catalogs(catalogs: &'static [&'static [define_err::CodeInfo]])
    -> core::result::Result<(), ConfigureError>;
```

Optional, write once. Lets an explicit code (`coded`, a code-first macro) find its title, severity, category, kinds, and public message through define-err's `catalog::find`. Since `0.4` (CFG-05).

```rust
static CATALOGS: &[&[define_err::CodeInfo]] = &[DbError::CATALOG, UserError::CATALOG];
report_err::configure_catalogs(CATALOGS)?;
```

### `ConfigureError`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum ConfigureError {
    AlreadyConfigured,
    OutOfRange { field: &'static str, value: u64, min: u64, max: u64 },
}
impl Display for ConfigureError;
impl core::error::Error for ConfigureError;
```

Since `0.4` (`AlreadyConfigured`, for `configure_catalogs`) and `0.5` (CFG-04). Each message says what to do:

| Variant | `Display` |
|---|---|
| `AlreadyConfigured` | `limits were already configured; call configure once, at the start of main, before other threads start` (or `catalogs were already configured; ...`) |
| `OutOfRange` | `max_frames = 2 is outside 4..=4096; choose a value in that range` |

## `main`

### `fn main() -> Result<()>`

Nothing to call: std's `Termination` for `Result<(), E: Debug>` prints `Error: ` followed by the `{:?}` report and exits with code 1, as with anyhow. Since `0.3` (MAIN-01). This path prints raw text; [`ExitReport`](#exitreport) escapes it.

```rust
fn main() -> report_err::Result<()> {
    run()
}

#[tokio::main]
async fn main() -> report_err::Result<()> {      // any runtime works
    run().await
}
```

### `ExitReport`

```rust
pub struct ExitReport { /* private */ }
impl std::process::Termination for ExitReport;
impl<E: Into<Error>> From<core::result::Result<(), E>> for ExitReport;
impl From<Error> for ExitReport;
```

**std**. Since `0.4` (MAIN-02). On `Err`, writes `Error: ` and the guarded, bounded `{:?}` report to standard error, escaping terminal control characters (C0 except newline and tab, DEL, C1) as `\u{..}` and indenting continuation lines, so a message cannot fake a new report block or move the cursor; a closed or broken standard error is ignored. Then exits with the error's [exit code](#exit_code-and-with_exit_code). `Ok` exits 0.

```rust
fn main() -> report_err::ExitReport {
    run().into()
}

#[tokio::main]
async fn main() -> report_err::ExitReport {
    run().await.into()
}
```

### `exit_code` and `with_exit_code`

```rust
impl Error {
    pub fn exit_code(&self) -> u8;
    pub fn with_exit_code(self, code: u8) -> Error;
}
```

Since `0.4` (MAIN-03). The exit code, in this order:

1. an explicit `with_exit_code(n)` (`0` is not a failure code and becomes 1);
2. 101 for an error made by a panic (`boundary`, `from_panic`), the code Rust uses for a panicking `main`;
3. the category's exit code (define-err's `Category::exit_code`: sysexits values, 64 usage, 65 data, 66 no input, 69 unavailable, 70 software, 73 cannot create, 74 I/O, 75 temporary failure, 77 permission, 78 configuration, 130 cancelled), when the category is known from a code, a catalog, or an `io::Error` root;
4. otherwise 1, what anyhow users get today.

```rust
let error = err!("usage: tool <file>").with_exit_code(64);
assert_eq!(error.exit_code(), 64);
```

### `run`

```rust
pub fn run<F: FnOnce() -> Result<()> + UnwindSafe>(app: F) -> ExitReport;
```

**std**. `boundary` and `ExitReport` in one call: a panic in `app` becomes a report and exit code 101 when panics unwind. Since `0.5` (MAIN-04).

```rust
fn main() -> report_err::ExitReport {
    report_err::run(|| {
        let config = load_config("app.noml")?;
        serve(config)
    })
}
```

## Panic boundary

```rust
pub fn boundary<T, F: FnOnce() -> Result<T> + UnwindSafe>(f: F) -> Result<T>;   // std
pub const PANICS_CAUGHT: bool;
impl Error {
    #[track_caller]
    pub fn from_panic(payload: Box<dyn core::any::Any + Send + 'static>) -> Error;
}
```

| Id | Item | Tier | Since |
|---|---|---|---|
| BND-01 | `boundary` | 1 | 0.5 |
| BND-02 | `PANICS_CAUGHT` | 2 | 0.5 |
| BND-03 | `Error::from_panic` | 3 | 0.5 |

`boundary` calls `f` inside `catch_unwind`. A caught panic becomes an error whose root is the panic message (or `panicked with a non-string payload`), with `is_panic`, `Kinds::PANIC`, category `Internal`, exit code 101, and the location of the `boundary` call. Std's panic hook still runs first (report-err never installs one). The closure must be `UnwindSafe`, as std requires; wrap state you have checked in `AssertUnwindSafe`.

**Under `panic = "abort"` nothing is caught**: the process aborts at the panic, as `catch_unwind` documents. The release profile of most of this portfolio's applications is `panic = "abort"`, so there `boundary` is a plain call. `PANICS_CAUGHT` (`cfg!(panic = "unwind")`) says which applies. Cost with no panic [est.]: under 2 ns over a plain call.

`from_panic` turns a `JoinHandle::join` or `catch_unwind` payload into an error. A payload whose own `Drop` panics is dropped inside a second guard and, if that panics too, leaked instead of propagated.

```rust
let output = report_err::boundary(|| plugin.render(&page))
    .context("plugin `gallery` failed")?;

let total = worker.join().map_err(Error::from_panic)??;
```

## Reporting

### `report` and `is_reported`

```rust
impl Error {
    #[track_caller]
    pub fn report(&self) -> bool;
    pub fn is_reported(&self) -> bool;
}
```

| Id | Method | Tier | Since |
|---|---|---|---|
| HOOK-02 | `report` | 2 | 0.5 |
| HOOK-03 | `is_reported` | 2 | 0.5 |

Hands the error to define-err's reporting facade, once: the first call sets an atomic flag and returns `true`, later calls (from any thread, on the same error) return `false` and do nothing, so outer layers never report twice. With no reporter installed the cost is the flag and one load, and nothing is allocated. With one installed, the reporter receives an event carrying the error's `ErrorRecord` (define-err's `report_record`), so it sees every context layer, the sections, the trail, and the code, severity, category, and kinds resolved [above](#codes-severity-category-kinds); messages are written through the guarded renderer, so a reporter never calls a `Display` unguarded. Errors with `Kinds::SECURITY` are reported like every other: nothing in report-err keeps an error local.

Report at boundaries (a request handler, a job runner, `main`), not at every `?`.

```rust
if let Err(error) = handle(req) {
    error.report();
    return problem_response(&error);       // the render library builds it from code and category
}
```

### `record`

```rust
impl Error {
    pub fn record(&self) -> define_err::ErrorRecord;
}
```

An owned, size-capped snapshot of the error for anything that outlives it: queued listeners, a crash ring, a message to another process or node. Since `0.5` (HOOK-04). Built from `ErrorRecord::from_error` over `&*self` (every context layer, then the root's sources), with the resolved code, severity, category, kinds, and public message, the hook's fields, the trail, and the backtrace text; within define-err's `RecordLimits`. An `Error` itself never crosses a process boundary (it holds trait objects and locations of one binary); its record does.

```rust
let record = error.record();
queue.push(record);                         // Send + Sync + Clone
```

### `info`

```rust
impl Error {
    #[track_caller]
    pub fn from_info<E>(error: E) -> Error where E: define_err::ErrorInfo + Send + Sync + 'static;
    pub fn info(&self) -> Option<&dyn define_err::ErrorInfo>;
}
```

| Id | Item | Tier | Since |
|---|---|---|---|
| ERR-18 | `from_info` | 3 | 0.4 |
| HOOK-05 | `info` | 3 | 0.4 |

`from_info` creates an error like `Error::new` and keeps the root reachable as `&dyn ErrorInfo`, so per-value metadata the identity probe cannot carry (`retry_after`, `#[location]` fields) survives. `info` returns it, or `None` for an error built any other way.

```rust
let error = Error::from_info(DbError::Busy { retry_after: Duration::from_millis(250) });
if let Some(after) = error.info().and_then(|i| i.retry_after()) {
    sleep(after);
}
```

### The creation hook

Installed with define-err's `set_creation_hook`; report-err calls the hook once for every `Error` it creates (not for context layers). Since `0.5` (HOOK-01). The hook receives the origin location, the explicit code, and the root's type name, and may add fields (shown under `Fields:` and kept in the record) and kinds. report-err owns the guards: a hook that creates an `Error` on the same thread is not called again for it (with `std`); a hook that panics leaves the field `creation_hook = "panicked"` and the error is still created (with `std` and unwinding). Without `std` the hook's contract forbids both. With no hook installed the cost is one load [est. under 1 ns].

```rust
use define_err::{Annotate, CreationHook, CreationInfo};

struct RequestScope;
impl CreationHook for RequestScope {
    fn on_create(&self, _info: &CreationInfo<'_>, out: &mut dyn Annotate) {
        if let Some(id) = current_request_id() {
            out.field("request_id", &id);
        }
    }
}

static SCOPE: RequestScope = RequestScope;
define_err::set_creation_hook(&SCOPE)?;
```

## Standard-library interop

```rust
impl Error {
    pub fn into_std(self) -> Box<dyn core::error::Error + Send + Sync + 'static>;
    #[track_caller]
    pub fn from_boxed(error: Box<dyn core::error::Error + Send + Sync + 'static>) -> Error;
}
impl From<Error> for Box<dyn core::error::Error + Send + Sync + 'static>;
impl From<Error> for Box<dyn core::error::Error + Send + 'static>;
impl From<Error> for Box<dyn core::error::Error + 'static>;
```

| Id | Item | Tier | Since |
|---|---|---|---|
| STD-01 | `into_std` | 3 | 0.3 |
| STD-02 | `From<Error> for Box<dyn Error ...>` | 3 | 0.3 |
| STD-03 | `from_boxed` | 3 | 0.3 |

`into_std` gives an owned standard error with the same `Display` and the same `source()` chain: one allocation in all, however many layers there are. `from_boxed` takes a boxed error back; a box made by `into_std` returns the original `Error` unchanged (no double wrapping). Any other box becomes the root: `downcast_ref::<Box<dyn Error + Send + Sync>>()` answers with it, and its concrete error is the root link of `chain()` and of the `source()` walk, so `root_cause().downcast_ref::<T>()` finds it (as will [`find`](#find-find_all-find_in_suppressed) in `0.5`). `err!(boxed)` does the same.

```rust
let boxed: Box<dyn std::error::Error + Send + Sync> = error.into_std();
library_that_wants_a_box(boxed)?;

let error = Error::from_boxed(plugin_result.unwrap_err());
```

## `render` and `RenderStyle`

```rust
impl Error {
    pub fn render(&self, out: &mut dyn core::fmt::Write, style: RenderStyle) -> core::fmt::Result;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum RenderStyle { Message, Chain, Report }
```

The guarded, bounded form of `{}` (`Message`), `{:#}` (`Chain`), and `{:?}` (`Report`), for code that must not fail while it prints an error (a panic hook, a crash path). Since `0.5` (STD-04). Returns `Err` only when `out` itself fails.

```rust
let mut text = String::new();
error.render(&mut text, RenderStyle::Report)?;
```

## `testing`

Feature `testing`. Since `0.5` (TST-01).

```rust
pub mod testing {
    #[track_caller] pub fn assert_chain_contains(error: &Error, text: &str);
    #[track_caller] pub fn assert_code(error: &Error, code: Code);
    #[track_caller] pub fn assert_section(error: &Error, kind: SectionKind, text: &str);
}
```

Each panics with the full report when the check fails, so a failing test shows the whole error.

```rust
#[test]
fn test_load_missing_file_reports_path() -> report_err::Result<()> {
    let error = load("missing.noml").unwrap_err();
    report_err::testing::assert_chain_contains(&error, "missing.noml");
    report_err::testing::assert_code(&error, ConfigError::MISSING);
    Ok(())
}
```

## Crate root

```rust
pub const VERSION: &str;
pub mod prelude;
```

`VERSION` is the crate version, from Cargo. Since `0.1` (ROOT-01). The [prelude](#the-prelude) since `0.3` (ROOT-03).

```rust
println!("report-err {}", report_err::VERSION);
```

## Item index

| Id | Item | Tier | Grade | Since |
|---|---|---|---|---|
| ROOT-01 | `VERSION` | 3 | P0 | 0.1 |
| ROOT-02 | `Result` | 1 | P0 | 0.3 |
| ROOT-03 | `prelude` | 1 | P0 | 0.3 |
| ROOT-04 | `Ok` | 1 | P2 | 0.4 |
| ERR-01 | `Error` | 1 | P0 | 0.3 |
| ERR-02 | `From<E> for Error` | 1 | P0 | 0.3 |
| ERR-03 | `Error::new` | 1 | P0 | 0.3 |
| ERR-04 | `Error::msg` | 1 | P0 | 0.3 |
| ERR-05 | `Error::context` | 1 | P0 | 0.3 |
| ERR-06 | `downcast_ref` | 1 | P0 | 0.3 |
| ERR-07 | `downcast_mut` | 2 | P1 | 0.3 |
| ERR-08 | `downcast` | 2 | P1 | 0.3 |
| ERR-09 | `is` | 1 | P1 | 0.3 |
| ERR-10 | `location` | 1 | P1 | 0.3 |
| ERR-11 | `code` | 1 | P0 | 0.4 |
| ERR-12 | `coded` | 1 | P0 | 0.4 |
| ERR-13 | `severity`, `category`, `kinds` | 2 | P1 | 0.4 |
| ERR-14 | `with_severity`, `with_kind` | 2 | P2 | 0.4 |
| ERR-15 | `public_message` | 2 | P1 | 0.4 |
| ERR-16 | `is_panic` | 2 | P2 | 0.5 |
| ERR-17 | `Display`, `Debug` | 1 | P0 | 0.3 |
| ERR-18 | `from_info` | 3 | P2 | 0.4 |
| ERR-19 | `Deref`, `DerefMut`, `AsRef` to `dyn Error` | 1 | P0 | 0.3 |
| CTX-01 | `Context::context`, `with_context` on `Result` | 1 | P0 | 0.3 |
| CTX-02 | the same on `Option` | 1 | P0 | 0.3 |
| CTX-03 | `Context::coded` | 1 | P0 | 0.4 |
| CTX-04 | `note`, `warning`, `suggestion` (and `with_`) | 1 | P1 | 0.4 |
| CTX-05 | `Context::attach` | 2 | P1 | 0.4 |
| CTX-06 | `Context::trace` | 2 | P1 | 0.4 |
| CTX-07 | `Context::with_cleanup` | 2 | P2 | 0.5 |
| MAC-01 | `err!`, `format_err!` | 1 | P0 | 0.3 |
| MAC-02 | `bail!` | 1 | P0 | 0.3 |
| MAC-03 | `ensure!` | 1 | P0 | 0.3 |
| MAC-04 | code-first arms | 1 | P0 | 0.4 |
| MAC-05 | `ensure!` operand values | 1 | P3 | after 1.0 |
| CHN-01 | `chain`, `Chain` | 1 | P0 | 0.3 |
| CHN-02 | `root_cause` | 1 | P1 | 0.3 |
| CHN-03 | `find` | 2 | P1 | 0.5 |
| CHN-04 | `find_all` | 3 | P2 | 0.5 |
| CHN-05 | `find_in_suppressed` | 3 | P2 | 0.5 |
| CHN-06 | `Chain::rev` | 3 | P2 | 0.5 |
| ATT-01 | `attach` | 2 | P1 | 0.4 |
| ATT-02 | `attachment` | 2 | P1 | 0.4 |
| ATT-03 | `attachments` | 3 | P2 | 0.4 |
| ATT-04 | `attach_display` | 2 | P2 | 0.4 |
| ATT-05 | `note`, `warning`, `suggestion` on `Error` | 1 | P1 | 0.4 |
| ATT-06 | `sections`, `Section` | 3 | P1 | 0.4 |
| ATT-07 | `SectionKind` | 3 | P1 | 0.4 |
| GRP-01 | `Error::group` | 2 | P1 | 0.5 |
| GRP-02 | `children` | 2 | P1 | 0.5 |
| GRP-03 | `push_child` | 3 | P2 | 0.5 |
| GRP-04 | `split` | 3 | P2 | 0.5 |
| GRP-05 | `flatten` | 3 | P2 | 0.5 |
| GRP-06 | `Errors` | 2 | P1 | 0.5 |
| GRP-07 | `CollectAll` | 2 | P1 | 0.5 |
| SUP-01 | `suppress` | 2 | P1 | 0.5 |
| SUP-02 | `suppressed` | 2 | P1 | 0.5 |
| SUP-03 | `unraisable` | 2 | P2 | 0.5 |
| SUP-04 | `with_cleanup` | 2 | P2 | 0.5 |
| TRC-01 | `trace` | 2 | P1 | 0.4 |
| TRC-02 | `trail`, `Trail`, `Step` | 2 | P1 | 0.4 |
| TRC-03 | `StepKind` | 3 | P1 | 0.4 |
| TRC-04 | `backtrace` | 2 | P1 | 0.4 |
| TRC-05 | `with_backtrace` | 2 | P2 | 0.4 |
| CFG-01 | `Limits` | 2 | P1 | 0.5 |
| CFG-02 | `configure` | 2 | P1 | 0.5 |
| CFG-03 | `limits` | 3 | P2 | 0.5 |
| CFG-04 | `ConfigureError` | 2 | P1 | 0.5 |
| CFG-05 | `configure_catalogs` | 2 | P2 | 0.4 |
| MAIN-01 | `fn main() -> Result<()>` | 1 | P0 | 0.3 |
| MAIN-02 | `ExitReport` | 1 | P1 | 0.4 |
| MAIN-03 | `exit_code`, `with_exit_code` | 2 | P2 | 0.4 |
| MAIN-04 | `run` | 1 | P2 | 0.5 |
| BND-01 | `boundary` | 1 | P1 | 0.5 |
| BND-02 | `PANICS_CAUGHT` | 2 | P1 | 0.5 |
| BND-03 | `Error::from_panic` | 3 | P2 | 0.5 |
| HOOK-01 | the creation hook (define-err's slot) | 3 | P1 | 0.5 |
| HOOK-02 | `report` | 2 | P1 | 0.5 |
| HOOK-03 | `is_reported` | 2 | P2 | 0.5 |
| HOOK-04 | `record` | 2 | P1 | 0.5 |
| HOOK-05 | `info` | 3 | P2 | 0.4 |
| STD-01 | `into_std` | 3 | P1 | 0.3 |
| STD-02 | `From<Error> for Box<dyn Error ...>` | 3 | P1 | 0.3 |
| STD-03 | `from_boxed` | 3 | P1 | 0.3 |
| STD-04 | `render`, `RenderStyle` | 3 | P2 | 0.5 |
| TST-01 | `testing` assertions | 2 | P2 | 0.5 |
| TST-02 | `testing::CountingHook` | 3 | P3 | after 1.0 |

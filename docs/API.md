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
        <a href="./MIGRATING.md" title="Migrating"><b>MIGRATING</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./SECURITY.md" title="Security"><b>SECURITY</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./PERFORMANCE.md" title="Performance"><b>PERFORMANCE</b></a>
    </sup>
</div>
<br>

A task-by-task tour of the 0.8 API, covering every public item. The rustdoc on
[docs.rs](https://docs.rs/report-err) has the same items with more examples;
every example here is compiled and run as a doc test with all features enabled.

- [Setup](#setup)
  - [Features and `no_std`](#features-and-no_std)
  - [The prelude](#the-prelude)
- [Tier 1 on one screen](#tier-1-on-one-screen)
- [Creating an error](#creating-an-error)
  - [`err!` and `format_err!`](#err-and-format_err)
  - [`bail!` and `ensure!`](#bail-and-ensure)
  - [`Error::new`, `Error::msg`, and `?`](#errornew-errormsg-and-)
  - [`Result` and `Ok`](#result-and-ok)
- [Adding context](#adding-context)
  - [`Context` on `Result` and `Option`](#context-on-result-and-option)
  - [`Error::context`](#errorcontext)
- [Notes, warnings, and suggestions](#notes-warnings-and-suggestions)
- [Attachments](#attachments)
- [Codes, severity, category, and kinds](#codes-severity-category-and-kinds)
  - [Codes](#codes)
  - [Severity, category, and kinds](#severity-category-and-kinds)
  - [Public messages and `ErrorInfo`](#public-messages-and-errorinfo)
  - [Catalogs](#catalogs)
- [Reading an error](#reading-an-error)
  - [The `source()` chain](#the-source-chain)
  - [Downcasting](#downcasting)
  - [Searching a tree](#searching-a-tree)
  - [Location](#location)
- [Printing an error](#printing-an-error)
  - [`{}`, `{:#}`, and `{:?}`](#--and-)
  - [`render` and `RenderStyle`](#render-and-renderstyle)
- [Several causes](#several-causes)
  - [`Error::group` and `children`](#errorgroup-and-children)
  - [`Errors`](#errors)
  - [`collect_all`](#collect_all)
  - [`split` and `flatten`](#split-and-flatten)
- [Suppressed errors and cleanup](#suppressed-errors-and-cleanup)
- [The trail and backtraces](#the-trail-and-backtraces)
- [`main` and exit codes](#main-and-exit-codes)
  - [`fn main() -> Result<()>`](#fn-main---result)
  - [`ExitReport`](#exitreport)
  - [`run`](#run)
  - [Exit codes](#exit-codes)
- [Panics](#panics)
- [Reporting](#reporting)
  - [`report` and `is_reported`](#report-and-is_reported)
  - [`record`](#record)
  - [`report_unhandled`](#report_unhandled)
  - [The creation hook](#the-creation-hook)
- [Limits](#limits)
- [Standard-library interop](#standard-library-interop)
- [Testing helpers](#testing-helpers)
- [Crate root](#crate-root)
- [What can fail](#what-can-fail)
- [Examples in the repository](#examples-in-the-repository)

## Setup

```toml
[dependencies]
report-err = "0.8"
```

### Features and `no_std`

| Feature | Default | Adds |
|---|---|---|
| `std` | yes | backtraces, `ExitReport`, `run`, `boundary`, the `io::Error` category, panic isolation of the creation hook |
| `testing` | no | the [`testing`](#testing-helpers) assertions |

With `default-features = false` the crate is `no_std` with `alloc`, on any
target with pointer-sized atomics:

```toml
report-err = { version = "0.8", default-features = false }
```

### The prelude

`use report_err::prelude::*;` brings in `bail!`, `ensure!`, `err!`,
`CollectAll`, `Context`, `Error`, and `Result`: everything Tier 1 needs.

```rust
use report_err::prelude::*;

fn parse(text: &str) -> Result<u16> {
    ensure!(!text.is_empty(), "the port is empty");
    text.parse().context("the port is not a number")
}

assert_eq!(parse("8080").unwrap(), 8080);
assert_eq!(parse("").unwrap_err().to_string(), "the port is empty");
```

## Tier 1 on one screen

Each common task is one line. The names are anyhow's, so code written for
anyhow reads the same.

```rust
use report_err::{bail, ensure, Context, Result};

#[derive(Debug)]
struct Config {
    port: u16,
}

fn parse(text: &str) -> Result<Config> {
    let port = text.trim().parse().context("the port is not a number")?;
    Ok(Config { port })
}

fn load_config(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {path}"))?;
    ensure!(!text.is_empty(), "config file {path} is empty");
    let config = parse(&text).context("parsing config")?;
    if config.port == 0 {
        bail!("port 0 is not allowed");
    }
    Ok(config)
}

let error = load_config("no/such/app.noml").unwrap_err();
assert_eq!(error.to_string(), "reading config file no/such/app.noml");
```

`main` ends with a report in one line too: `fn main() -> Result<()>` prints it
and exits with 1, and `fn main() -> ExitReport { run(app) }` exits with the
error's own code and turns a panic into a report
([`main` and exit codes](#main-and-exit-codes)).

## Creating an error

### `err!` and `format_err!`

`err!(...)` makes an `Error`. `format_err!` is the same macro under anyhow's
other name. The arms, tried in this order:

| Arm | Example | Result |
|---|---|---|
| a string literal | `err!("no input")` | a message; a literal with no arguments is stored as `&'static str` |
| a format string and arguments | `err!("bad port {port}")` | a formatted message |
| a code, then a format string | `err!(DB_DOWN, "after {ms} ms")` | the same, with the code set; the first argument must be a define-err `Code` |
| any other expression | `err!(io_error)` | an error value becomes the root (as `Error::new`); any other `Display + Debug` value becomes a message (as `Error::msg`) |

```rust
use report_err::err;

let port = 0;
assert_eq!(err!("no input").to_string(), "no input");
assert_eq!(err!("bad port {port}").to_string(), "bad port 0");
assert_eq!(report_err::format_err!("bad port {}", port).to_string(), "bad port 0");

let code = define_err::code!("APP", 7);
let error = err!(code, "pool empty after {} ms", 250);
assert_eq!(error.code(), Some(code));
assert_eq!(error.to_string(), "pool empty after 250 ms");

let io = std::io::Error::other("disk full");
assert!(err!(io).is::<std::io::Error>());
```

### `bail!` and `ensure!`

`bail!(...)` returns `Err(err!(...))` from the function. `ensure!(cond, ...)`
does so when `cond` is false; with no message the error says
`condition failed: ` and the condition's source text. Both take the same arms
as `err!`, code first included.

```rust
use report_err::{bail, ensure, Result};

fn check(len: usize, flags: u8) -> Result<()> {
    ensure!(len > 0);
    ensure!(len <= 64, "length {len} is over 64");
    if flags & 0x80 != 0 {
        bail!("reserved flag set: {flags:#x}");
    }
    Ok(())
}

assert_eq!(check(0, 0).unwrap_err().to_string(), "condition failed: len > 0");
assert_eq!(check(65, 0).unwrap_err().to_string(), "length 65 is over 64");
assert_eq!(check(1, 0x80).unwrap_err().to_string(), "reserved flag set: 0x80");
assert!(check(1, 0).is_ok());
```

### `Error::new`, `Error::msg`, and `?`

`Error::new(error)` wraps any `std::error::Error + Send + Sync + 'static`; `?`
and `From` do the same for you. `Error::msg(message)` makes an error from any
`Display + Debug + Send + Sync + 'static` value. Each records the caller's
location.

```rust
use report_err::{Error, Result};

fn read() -> Result<Vec<u8>> {
    Ok(std::fs::read("no/such/file")?)            // io::Error becomes an Error
}

let wrapped = Error::new(std::fmt::Error);
assert!(wrapped.is::<std::fmt::Error>());
assert_eq!(Error::msg("no input").to_string(), "no input");
assert!(read().unwrap_err().is::<std::io::Error>());
```

### `Result` and `Ok`

`Result<T, E = Error>` is `core::result::Result` with `Error` as the default.
`report_err::Ok(value)` is `Ok` with the error type fixed, for the closures
and blocks where inference cannot find it. It is not in the prelude.

```rust
let parse = |text: &str| {
    let n: u8 = text.parse()?;
    report_err::Ok(n)
};
assert_eq!(parse("7").unwrap(), 7);
assert!(parse("x").is_err());
```

## Adding context

### `Context` on `Result` and `Option`

`Context` is a sealed trait on `Result<T, E>` (any `E` that becomes an `Error`)
and on `Option<T>`. Each method turns the failure into an `Error` and adds to
it; on success it returns the value untouched, and the `with_` forms do not
run their closure.

| Method | Adds |
|---|---|
| `context(c)` / `with_context(\|\| c)` | a context layer, the new outermost message |
| `coded(code)` | the error's code ([Codes](#codes)) |
| `note(t)`, `warning(t)`, `suggestion(t)`, and their `with_` forms | a section ([Notes, warnings, and suggestions](#notes-warnings-and-suggestions)) |
| `attach(value)` | an opaque value ([Attachments](#attachments)) |
| `trace()` | a step of the trail ([The trail and backtraces](#the-trail-and-backtraces)) |
| `with_cleanup(\|\| ...)` | runs a cleanup and keeps its failure ([Suppressed errors and cleanup](#suppressed-errors-and-cleanup)) |

On `Option`, `None` becomes an error whose message is the context; with only
`coded`, it is the code's catalog title, or the code itself.

```rust
use std::collections::HashMap;
use report_err::{Context, Result};

fn port(settings: &HashMap<&str, &str>) -> Result<u16> {
    let text = settings.get("port").context("no port setting")?;
    text.parse()
        .with_context(|| format!("port {text:?} is not a number"))
}

let mut settings = HashMap::new();
assert_eq!(port(&settings).unwrap_err().to_string(), "no port setting");
settings.insert("port", "80x");
let error = port(&settings).unwrap_err();
assert_eq!(format!("{error:#}"), "port \"80x\" is not a number: invalid digit found in string");
settings.insert("port", "80");
assert_eq!(port(&settings).unwrap(), 80);
```

### `Error::context`

`error.context(c)` adds a layer to an `Error` you already hold. `c` is any
`Display + Debug + Send + Sync + 'static` value. The first layer is stored in
the error's own block; layers two to five share one more block.

```rust
let error = report_err::err!("disk full").context("saving the file").context("closing");
assert_eq!(error.to_string(), "closing");
assert_eq!(format!("{error:#}"), "closing: saving the file: disk full");
```

## Notes, warnings, and suggestions

Sections are text for the person reading the report, printed as `Note:`,
`Warning:`, and `Suggestion:` lines under the causes, in the order added.
`Error` has `note`, `warning`, and `suggestion`; `Context` has those and
`with_note`, `with_warning`, and `with_suggestion`, whose closure runs only on
failure. A `&'static str` is stored as it is; anything else is formatted once,
within the error's byte budget.

`sections()` returns a [`Sections`] iterator of [`Section`] values, each with
a `kind` ([`SectionKind`]: `Note`, `Warning`, `Suggestion`) and its `text`.

```rust
use report_err::{err, Context, SectionKind};

let error = err!("sync stopped")
    .note("the server closed the connection")
    .warning("data written after 12:04 may be missing")
    .suggestion("run `tool sync --resume`");
let report = format!("{error:?}");
assert!(report.contains("Note: the server closed the connection"));
assert!(report.contains("Warning: data written after 12:04 may be missing"));
assert!(report.contains("Suggestion: run `tool sync --resume`"));

let kinds: Vec<SectionKind> = error.sections().map(|s| s.kind).collect();
assert_eq!(kinds, [SectionKind::Note, SectionKind::Warning, SectionKind::Suggestion]);

let tries = 3;
let failed: Result<(), std::io::Error> = Err(std::io::Error::other("timed out"));
let error = failed.with_note(|| format!("{tries} tries")).unwrap_err();
assert_eq!(error.sections().next().map(|s| s.text), Some("3 tries"));
```

## Attachments

An attachment is a value of any `Send + Sync + 'static` type, read back by
type. `attach` keeps it opaque: never printed, never reported as text.
`attach_display` also prints it under `Attachments:`. `attachment::<A>()` is
the newest of type `A`; `attachments::<A>()` returns an [`Attachments`]
iterator over all of them, newest first. `Context::attach` attaches on
failure. Up to 32 are kept, within the byte budget; past that a new one is
dropped and counted under `Omitted:`.

```rust
use report_err::{err, Context};

#[derive(Debug, PartialEq)]
struct RequestId(u64);

let error = err!("order not found").attach(RequestId(7)).attach_display("tenant 42");
assert_eq!(error.attachment::<RequestId>(), Some(&RequestId(7)));
let report = format!("{error:?}");
assert!(!report.contains("RequestId"));
assert!(report.contains("Attachments:\n    tenant 42"));

let error = err!("x").attach(1u32).attach(2u32);
assert_eq!(error.attachments::<u32>().copied().collect::<Vec<_>>(), [2, 1]);

let failed: Result<(), std::fmt::Error> = Err(std::fmt::Error);
let error = failed.attach(RequestId(9)).unwrap_err();
assert_eq!(error.attachment::<RequestId>(), Some(&RequestId(9)));
```

## Codes, severity, category, and kinds

These come from define-err; report-err re-exports [`Code`], [`Severity`],
[`Category`], and [`Kinds`], and never invents a value of its own.

### Codes

`code()` is the code set with `coded` or a code-first macro; else the innermost
coded error in the chain (a define-err type with `#[code]`); else the root's
`ErrorInfo` code. `coded(code)` sets it, on an `Error` or through `Context`,
and wins over a code found in the chain. The report prints it on a `Code:`
line.

```rust
use report_err::{Context, Result};

define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(7, name = TIMEOUT, help = "Raise the timeout.")]
        #[category(Unavailable)]
        Timeout => "query timed out",
    }
}

fn query() -> Result<()> {
    Err(DbError::Timeout).context("loading the user")
}

let error = query().unwrap_err();
assert_eq!(error.code(), Some(DbError::TIMEOUT));
assert!(format!("{error:?}").contains("Code: DB-0007"));

let code = define_err::code!("APP", 12);
let error = report_err::err!("disk full").coded(code);
assert_eq!(error.code().map(|c| c.to_string()).as_deref(), Some("APP-0012"));
```

### Severity, category, and kinds

| Method | Returns |
|---|---|
| `severity()` | `with_severity`'s value; else the code's; else `Severity::FATAL` for a panic; else the category's default |
| `category()` | the code's; else the category of an `io::Error` root's kind; else `Internal` for a panic; else `Unknown` |
| `kinds()` | the kinds added with `with_kinds` and the code's, `PANIC` for a panic; `Kinds::ERROR` when there are none |
| `with_severity(severity)` | the error, with its severity overridden |
| `with_kinds(kinds)` | the error, with every kind in `kinds` added (combined with OR) |

```rust
use report_err::{err, Category, Kinds, Severity};

let error = err!("x");
assert_eq!(error.severity(), Severity::ERROR);
assert_eq!(error.category(), Category::Unknown);
assert_eq!(error.kinds(), Kinds::ERROR);

let error = err!("login refused")
    .with_severity(Severity::CRITICAL)
    .with_kinds(Kinds::SECURITY | Kinds::AUDIT);
assert_eq!(error.severity(), Severity::CRITICAL);
assert!(error.kinds().contains(Kinds::SECURITY | Kinds::AUDIT));

let missing = report_err::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
assert_eq!(missing.category(), Category::NotFound);
```

### Public messages and `ErrorInfo`

`public_message()` is text a client may see, for an error with a code: the
code's catalog entry; else the root's `ErrorInfo`; else the category's default
public message. It is `None` without a code, and never comes from a
`Display`, which is a private message for logs. A variant's `#[public]` text
is used only from a catalog entry or a `from_info` root, never from the
identity probe of an error in the chain, because any error's `description()`
can claim to be a defined error. `record().public` carries the same text.

`Error::from_info(error)` wraps a define-err error like `Error::new` and keeps
it reachable as `&dyn ErrorInfo` through `info()`, so per-value metadata the
code alone cannot carry (`retry_after`, a `#[location]` field) survives.

```rust
define_err::define_errors! {
    pub enum Busy {
        Pool { #[retry_after] wait: core::time::Duration } => "pool is busy",
    }
}

let error = report_err::err!("db password rejected for root");
assert_eq!(error.public_message(), None);
let error = error.coded(define_err::code!("APP", 1));
assert_eq!(error.public_message(), Some("An unexpected error occurred."));

let wait = core::time::Duration::from_millis(250);
let error = report_err::Error::from_info(Busy::Pool { wait });
assert_eq!(error.info().and_then(|i| i.retry_after()), Some(wait));
assert!(report_err::err!("x").info().is_none());
```

### Catalogs

A code set with `coded` or a code-first macro is only a number. Give
`configure_catalogs` your define-err catalogs, once, at the start of `main`,
and such a code finds its title, severity, category, kinds, and public message
in them. A second call returns `ConfigureError::AlreadyConfigured`.

```rust
define_err::define_errors! {
    namespace = "DB";
    pub enum DbError {
        #[code(1, name = TIMEOUT, help = "Raise the timeout.", title = "Query timed out")]
        #[category(Unavailable)]
        Timeout => "query timed out",
    }
}

static CATALOGS: &[&[define_err::CodeInfo]] = &[DbError::CATALOG];
report_err::configure_catalogs(CATALOGS).unwrap();

let error = report_err::err!("no reply").coded(DbError::TIMEOUT);
assert_eq!(error.category(), report_err::Category::Unavailable);
assert_eq!(report_err::configure_catalogs(CATALOGS), Err(report_err::ConfigureError::AlreadyConfigured));
```

## Reading an error

### The `source()` chain

`Error` dereferences (`Deref`, `DerefMut`, `AsRef`) to
`dyn std::error::Error + Send + Sync`, so it works wherever a `&dyn Error` is
expected. Its `source()` walks every context layer, then the root, then the
root's own sources. `chain()` returns a [`Chain`] iterator over the same links,
outermost first, that can also run backwards (`chain().rev()`).
`root_cause()` is the last link.

```rust
use std::error::Error as _;

let error = report_err::Error::new(std::fmt::Error).context("rendering").context("saving");
let links: Vec<String> = error.chain().map(|l| l.to_string()).collect();
assert_eq!(links, ["saving", "rendering", "an error occurred when formatting an argument"]);
assert_eq!(error.chain().rev().next().map(|l| l.to_string()), links.last().cloned());
assert!(error.root_cause().is::<std::fmt::Error>());
assert_eq!(error.source().map(|s| s.to_string()).as_deref(), Some("rendering"));

fn takes_std(error: &(dyn std::error::Error + 'static)) -> String {
    error.to_string()
}
assert_eq!(takes_std(&*error), "saving");
```

### Downcasting

`downcast_ref::<T>()` and `downcast_mut::<T>()` look at each context value and
the root, as anyhow does; `is::<T>()` says whether one matches. `downcast::<T>()`
takes the root out by value, or returns the error unchanged.

```rust
use report_err::Error;

let mut error = Error::new(std::io::Error::other("disk full")).context("saving");
assert!(error.is::<std::io::Error>());
assert_eq!(error.downcast_ref::<&str>(), Some(&"saving"));
assert!(error.downcast_mut::<std::io::Error>().is_some());

let io: std::io::Error = Error::new(std::io::Error::other("x")).downcast().unwrap();
assert_eq!(io.to_string(), "x");
let still = Error::msg("not io").downcast::<std::io::Error>().unwrap_err();
assert_eq!(still.to_string(), "not io");
```

### Searching a tree

`find::<T>()` is the first `T` anywhere in the tree, depth first: the context
values, the root, the root's sources, then each child's tree. `find_all::<T>()`
returns a [`FindAll`] iterator over every one, in the same order. Suppressed
errors are not causes and are not searched; search them with
`error.suppressed().iter().find_map(|e| e.find::<T>())`.

```rust
use report_err::{err, Context, Error};

let io = || Error::new(std::io::Error::other("x")).context("reading");
let error = Error::group("load failed", [err!("bad row"), io(), io()]);
assert!(error.find::<std::io::Error>().is_some());
assert_eq!(error.find_all::<std::io::Error>().count(), 2);
assert!(error.downcast_ref::<std::io::Error>().is_none()); // not in the group's own chain

let error = err!("write failed").suppress(Error::new(std::fmt::Error));
assert!(error.find::<std::fmt::Error>().is_none());
assert!(error.suppressed().iter().find_map(|e| e.find::<std::fmt::Error>()).is_some());
```

### Location

`location()` is where the error was created: the line of `err!`, `bail!`,
`ensure!`, `Error::new`, or the `?` that converted it.

```rust
let error = report_err::err!("x");
assert_eq!(error.location().line(), line!() - 1);
assert!(error.location().file().ends_with(".rs"));
```

## Printing an error

### `{}`, `{:#}`, and `{:?}`

| Format | Prints |
|---|---|
| `{}` | the outermost message |
| `{:#}` | the chain on one line, joined by `": "` |
| `{:?}` | the report, as `main` prints it |
| `{:#?}` | the error's fields, for debugging report-err itself |

The report, block by block, each only when it has content: the outermost
message; `Caused by:` (one line for one cause, numbered for several);
`Causes (n):` with the children as a tree; the `Note:`, `Warning:`, and
`Suggestion:` lines; `Suppressed:`; `Code:`; `Location:`; `Trace:` when the
error travelled; `Attachments:`; `Fields:` from the creation hook; `Omitted:`
for what a budget dropped; `Backtrace:` when one was captured.

In `{:#}` and `Caused by:`, a layer whose text the layer above already ends
with is skipped, so an error whose `Display` repeats its source is not printed
twice. Width, fill, and precision are ignored. `{:?}` writes control
characters other than newline and tab, and the Unicode bidirectional controls,
as `\u{..}` text, so a message cannot move the cursor or rewrite a line; `{}`
and `{:#}` write the text as it is. `{}`, `{:#}`, and `{:?}` are not
guarded: a `Display` that misbehaves misbehaves here as anywhere else. Use
[`render`](#render-and-renderstyle), `ExitReport`, `report`, or `record` where
it must not.

```rust
use report_err::Context;

let error = std::fs::read_to_string("no/such/app.noml")
    .context("reading config")
    .context("starting server")
    .unwrap_err();
assert_eq!(error.to_string(), "starting server");
assert!(format!("{error:#}").starts_with("starting server: reading config: "));

let report = format!("{error:?}");
assert!(report.starts_with("starting server\n\nCaused by:\n    0: reading config\n    1: "));
assert!(report.contains("\nLocation: "));
```

### `render` and `RenderStyle`

`render(out, style)` writes `{}`, `{:#}`, or `{:?}` ([`RenderStyle`]:
`Message`, `Chain`, `Report`) in a form that cannot fail on the error's
account, for a panic hook or a crash path. A `Display` that panics prints
`<display panicked>` (with `std` and unwinding), one that fails prints
`<display failed>`, and the output stops at `max_render` bytes with
`(report truncated)`. Control characters are escaped as in `{:?}`, in all
three styles. It returns `Err` only when `out` itself fails.

```rust
use std::fmt;
use report_err::RenderStyle;

struct Broken;
impl fmt::Display for Broken {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Err(fmt::Error)
    }
}

let error = report_err::err!("root").context(Broken);
let mut text = String::new();
error.render(&mut text, RenderStyle::Chain).unwrap();
assert_eq!(text, "<display failed>: root");

text.clear();
report_err::err!("x").context("y").render(&mut text, RenderStyle::Message).unwrap();
assert_eq!(text, "y");
```

## Several causes

### `Error::group` and `children`

`Error::group(message, errors)` is an error made of several failures: the
message is its root, the errors its children, in order. `children()` is the
slice of them (empty for an error that is not a group). `push_child(child)`
adds one more to any error and returns `false` when its tree does not fit the
budget (`max_tree` errors, `max_depth` levels); a refused child is counted
under `Omitted:`.

```rust
use report_err::{err, Error};

let mut error = Error::group("2 of 3 shards failed", [err!("shard 1"), err!("shard 3")]);
assert_eq!(error.to_string(), "2 of 3 shards failed");
assert_eq!(error.children().len(), 2);
assert!(error.push_child(err!("shard 4")));
assert!(format!("{error:?}").contains("Causes (3):"));
assert!(err!("x").children().is_empty());
```

### `Errors`

[`Errors`] collects the failures of a loop. `push(error)` adds one;
`push_result(result)` adds the error of a failed result and returns `None`,
or returns the value. `len()` and `is_empty()` count every failure pushed,
including any the budget did not keep. `finish(message)` is `Ok(())`, or a
group of every failure; `into_error(message)` is the group, or `None`.
`Errors::new()` allocates nothing until the first failure.

```rust
use report_err::{err, Errors, Result};

fn import(rows: &[&str]) -> Result<u32> {
    let mut errors = Errors::new();
    let mut total = 0;
    for row in rows {
        if let Some(value) = errors.push_result(row.parse::<u32>()) {
            total += value;
        }
    }
    errors.finish("the import failed")?;
    Ok(total)
}

assert_eq!(import(&["1", "2"]).unwrap(), 3);
let error = import(&["1", "x", "y"]).unwrap_err();
assert_eq!(error.to_string(), "the import failed");
assert_eq!(error.children().len(), 2);

let mut errors = Errors::new();
assert!(errors.is_empty());
errors.push(err!("a"));
assert_eq!(errors.len(), 1);
assert!(errors.into_error("all").is_some());
```

### `collect_all`

`collect_all()` ([`CollectAll`], on any iterator of results) consumes the
whole iterator: `Ok` of the collected values when every item succeeded, else a
group whose message is `"{failed} of {total} items failed"`, with the failures
as children in order.

```rust
use report_err::{CollectAll, Result};

let parsed: Result<Vec<u8>> = ["1", "x", "3", "y"].iter().map(|s| s.parse::<u8>()).collect_all();
let error = parsed.unwrap_err();
assert_eq!(error.to_string(), "2 of 4 items failed");
assert_eq!(error.children().len(), 2);

let fine: Result<Vec<u8>> = ["1", "2"].iter().map(|s| s.parse::<u8>()).collect_all();
assert_eq!(fine.unwrap(), [1, 2]);
```

### `split` and `flatten`

`split(predicate)` divides a group's direct children into those that match
and those that do not; each half keeps the group's message, location, code,
severity, exit code, kinds, and sections, and an empty half is `None`.
`flatten()` returns the leaves of the tree, depth first.

```rust
use report_err::{err, Error};

let error = Error::group("import failed", [err!("row 1"), err!("row 2"), err!("row 3")]);
let (kept, rest) = error.split(|e| e.to_string() != "row 2");
assert_eq!(kept.unwrap().children().len(), 2);
assert_eq!(rest.unwrap().children().len(), 1);

let inner = Error::group("b", [err!("b1"), err!("b2")]);
let error = Error::group("all", [err!("a"), inner, err!("c")]);
let leaves: Vec<String> = error.flatten().iter().map(|e| e.to_string()).collect();
assert_eq!(leaves, ["a", "b1", "b2", "c"]);
```

## Suppressed errors and cleanup

`suppress(other)` keeps a failure that happened while handling this one (a
rollback that failed too). It is printed under `Suppressed:` but is not a
cause: `find`, `code`, and `downcast` ignore it. `suppressed()` is the slice of
them, in order. Up to 16 are kept by default (`max_suppressed`).

`Context::with_cleanup(cleanup)` runs `cleanup` whatever the result was: when
both fail, the cleanup's error is suppressed in the main one; when only the
cleanup fails, its error is the result.

```rust
use report_err::{err, Context};

let error = err!("write failed").suppress(err!("rollback failed"));
assert_eq!(error.suppressed().len(), 1);
assert!(format!("{error:?}").contains("Suppressed:"));

let written: Result<(), std::io::Error> = Err(std::io::Error::other("disk full"));
let error = written.with_cleanup(|| Err(err!("sync failed"))).unwrap_err();
assert_eq!(error.to_string(), "disk full");
assert_eq!(error.suppressed()[0].to_string(), "sync failed");

let fine: Result<u8, std::io::Error> = Ok(1);
assert_eq!(fine.with_cleanup(|| Ok::<(), report_err::Error>(())).unwrap(), 1);
```

## The trail and backtraces

The trail is the way the error travelled: its origin, every context layer, and
every `.trace()` point. A `?` that returns an `Error` where an `Error` is
expected cannot record its line (Rust calls the identity conversion), so
`trace()`, on an `Error` or through `Context`, records it. `trail()` returns a
[`Trail`] of [`Step`] values, each with a `location`, a `kind` ([`StepKind`]:
`Origin`, `Context`, `Trace`), and the `context` added at that step. The first
4 and the latest 11 points are kept; `Trail::omitted()` counts those dropped
between.

`backtrace()` is the backtrace captured when the error was created: only at
the root, only when `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE` turns them on, or
when `with_backtrace()` forces one. Symbols are resolved only when it is
printed.

```rust
use report_err::{err, Context, Result, StepKind};

fn inner() -> Result<()> {
    Err(err!("disk full"))
}
fn outer() -> Result<()> {
    inner().trace()?;
    Ok(())
}

let error = outer().unwrap_err().context("closing");
let kinds: Vec<StepKind> = error.trail().map(|s| s.kind).collect();
assert_eq!(kinds, [StepKind::Origin, StepKind::Trace, StepKind::Context]);
assert_eq!(error.trail().omitted(), 0);

let error = err!("invariant broken").with_backtrace();
assert!(error.backtrace().is_some());
assert!(format!("{error:?}").contains("Backtrace:"));
```

## `main` and exit codes

### `fn main() -> Result<()>`

The standard library prints `Error: ` and the report (`{:?}`) to standard error
and exits with 1, as with anyhow. Control characters are escaped, but a
message's continuation lines are not indented, so a message with a newline
can print a line that looks like another; where an attacker may control the
text, return `ExitReport` instead.

```rust,no_run
fn main() -> report_err::Result<()> {
    std::fs::read("app.noml")?;
    Ok(())
}
```

### `ExitReport`

`ExitReport` is a return type for `main` (feature `std`). On `Err` it writes
`Error: ` and the report to standard error, with terminal control characters
escaped and a message's continuation lines indented, so no message can move
the cursor or fake a block of the report; the report is cut at 1 MiB. Then it
exits with [`Error::exit_code`](#exit-codes). A closed standard error is
ignored. It is made with `From` from a `Result<(), E>` or an `Error`.

```rust,no_run
fn app() -> report_err::Result<()> {
    Ok(())
}

fn main() -> report_err::ExitReport {
    app().into()
}
```

### `run`

`run(app)` is [`boundary`](#panics) and `ExitReport` in one call: a panic in
`app` becomes a report and exit code 101, when panics unwind.

```rust,no_run
fn main() -> report_err::ExitReport {
    report_err::run(|| {
        println!("serving");
        Ok(())
    })
}
```

### Exit codes

`exit_code()` is the code `ExitReport` exits with: `with_exit_code`'s value;
else 101 for a panic; else the category's exit code, when a code, a catalog,
or an `io::Error` root says what the category is; otherwise 1.
`with_exit_code(code)` sets it; 0 is not a failure code and becomes 1.

```rust
let error = report_err::err!("usage: tool <file>");
assert_eq!(error.exit_code(), 1);
let error = error.with_exit_code(64);
assert_eq!(error.exit_code(), 64);
assert_eq!(report_err::err!("x").with_exit_code(0).exit_code(), 1);

let missing = report_err::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound));
assert_eq!(missing.exit_code(), report_err::Category::NotFound.exit_code());
```

## Panics

`boundary(f)` runs `f` and turns a panic in it into an `Error` (feature
`std`): the panic message is the root, `is_panic()` is true, the category is
`Internal`, the severity `FATAL`, and the exit code 101. It catches a panic
only when panics unwind; under `panic = "abort"` the process aborts at the
panic. `PANICS_CAUGHT` says which applies to this build. `f` must be
`UnwindSafe`; wrap a closure that borrows mutable state in
`std::panic::AssertUnwindSafe`.

`Error::from_panic(payload)` makes the same error from a payload you caught
yourself (`std::panic::catch_unwind`, `JoinHandle::join`).

```rust
let caught = report_err::boundary(|| -> report_err::Result<u8> { panic!("plugin bug") });
if report_err::PANICS_CAUGHT {
    let error = caught.unwrap_err();
    assert!(error.is_panic());
    assert_eq!(error.to_string(), "plugin bug");
    assert_eq!(error.exit_code(), 101);
}
assert_eq!(report_err::boundary(|| Ok(7)).unwrap(), 7);

let payload = std::thread::spawn(|| panic!("worker died")).join().unwrap_err();
let error = report_err::Error::from_panic(payload);
assert_eq!(error.to_string(), "worker died");
assert!(!report_err::err!("x").is_panic());
```

## Reporting

### `report` and `is_reported`

`report()` hands the error to define-err's reporting facade, once: the first
call, from any thread, returns `true`; later calls return `false` and do
nothing, so outer layers never report it twice. When a reporter is installed
(define-err's `set_reporter`) it receives the error's [`record`](#record).
With none, the cost is the flag and one load. `is_reported()` says whether
`report` has run.

```rust
let error = report_err::err!("request failed");
assert!(!error.is_reported());
assert!(error.report());
assert!(!error.report());
assert!(error.is_reported());
```

### `record`

`record()` is an owned, size-capped [`ErrorRecord`] (define-err's), for
anything that outlives the error: a queue, a crash ring, another process. It
holds the chain, with each message written through the guarded renderer; the
code, severity, category, kinds, and public message; the sections and the
creation hook's fields; the counts of children and suppressed errors; the
trail; and the backtrace text when one was captured.

```rust
use report_err::Context;

let error = std::fs::read("no/such/file").context("loading").unwrap_err();
let record = error.record();
assert_eq!(record.chain.len(), 2);
assert_eq!(record.chain[0].message, "loading");
assert!(record.handled);
```

### `report_unhandled`

`report_unhandled(error)` reports an error that has nowhere to go (in a
`Drop`, in a thread nobody joins) as unhandled, and returns `true` when a
reporter received it. With no reporter installed and `std` on, it writes one
guarded, escaped line to standard error instead, starting
`Error (unhandled): `, so the error is never lost silently.

```rust
struct Segment;

impl Drop for Segment {
    fn drop(&mut self) {
        let flushed: Result<(), report_err::Error> = Err(report_err::err!("disk full"));
        if let Err(error) = flushed {
            report_err::report_unhandled(error.context("flushing a segment on drop"));
        }
    }
}

drop(Segment);
```

### The creation hook

A hook installed with define-err's `set_creation_hook` is called once for
every `Error` report-err creates (not for context layers). It receives the
location, the explicit code, and the root's type name, and may add fields
(printed under `Fields:` and kept in the record) and kinds. report-err guards
it: a hook that creates an `Error` itself is not called again for that one,
and a hook that panics leaves the field `creation_hook = "panicked"` (both
with `std`). With no hook installed the cost is one load.

```rust
use define_err::{Annotate, CreationHook, CreationInfo};

struct RequestScope;
impl CreationHook for RequestScope {
    fn on_create(&self, _info: &CreationInfo<'_>, out: &mut dyn Annotate) {
        out.field("request_id", &42);
    }
}

static SCOPE: RequestScope = RequestScope;
define_err::set_creation_hook(&SCOPE).unwrap();
let error = report_err::err!("x");
assert!(format!("{error:?}").contains("request_id"));
```

## Limits

Everything that grows has a budget. [`Limits`] holds them, `Limits::DEFAULT`
the defaults; `configure(limits)` sets them once per process, at the start of
`main`, and `limits()` reads the ones in force.

| Field | Default | Range | Bounds |
|---|---|---|---|
| `max_frames` | 64 | 4 to 4,096 | context layers per error; past it the oldest above the innermost eight is dropped and counted |
| `max_attachments` | 32 | 0 to 1,024 | attachments per error |
| `max_sections` | 32 | 0 to 1,024 | sections per error |
| `max_suppressed` | 16 | 0 to 1,024 | suppressed errors per error |
| `max_tree` | 1,024 | 1 to 1,048,576 | errors in one tree, children and suppressed errors included |
| `max_depth` | 16 | 1 to 64 | nesting of children and suppressed errors |
| `max_bytes` | 65,536 | 1,024 to 16,777,216 | text and value bytes one error's extras hold |
| `max_chain` | 128 | 8 to 4,096 | links walked in one chain; also stops a `source()` that never ends |
| `max_render` | 1,048,576 | 4,096 to 67,108,864 | bytes one guarded rendering writes |

`configure` returns `ConfigureError::OutOfRange` for a value outside its range
(nothing is changed) and `ConfigureError::AlreadyConfigured` on a second call.

```rust
use report_err::{configure, limits, ConfigureError, Limits};

let mut wanted = Limits::DEFAULT;
wanted.max_frames = 32;
assert!(configure(wanted).is_ok());
assert_eq!(limits().max_frames, 32);
assert_eq!(configure(wanted), Err(ConfigureError::AlreadyConfigured));

```

A refused value is checked before anything is set, so the first good call
still works:

```rust
use report_err::{configure, ConfigureError, Limits};

let mut limits = Limits::DEFAULT;
limits.max_depth = 0;
let refused = configure(limits).unwrap_err();
assert!(matches!(refused, ConfigureError::OutOfRange { field: "max_depth", value: 0, min: 1, max: 64 }));
assert_eq!(refused.to_string(), "max_depth = 0 is outside 1..=64; choose a value in that range");

limits.max_depth = 8;
assert!(configure(limits).is_ok());
```

## Standard-library interop

`From<Error>` is implemented for `Box<dyn Error + Send + Sync>`,
`Box<dyn Error + Send>`, and `Box<dyn Error>`, so `?` and `.into()` give code
that wants a boxed standard error the same `Display` and the same `source()`
chain, in one allocation however many layers there are.
`Error::from_boxed(boxed)` goes back: a box made from an `Error` gives the
original `Error` back; any other box becomes the root. `?` does not convert a
`Box<dyn Error + Send + Sync>` by itself (as with anyhow), because the box is
not itself an `Error` type; use `from_boxed`.

```rust
let error = report_err::Error::msg("disk full").context("saving");
let boxed: Box<dyn std::error::Error + Send + Sync> = error.into();
assert_eq!(boxed.to_string(), "saving");
assert_eq!(boxed.source().map(|s| s.to_string()).as_deref(), Some("disk full"));

let back = report_err::Error::from_boxed(boxed);
assert_eq!(format!("{back:#}"), "saving: disk full");

let foreign: Box<dyn std::error::Error + Send + Sync> = Box::new(std::fmt::Error);
let error = report_err::Error::from_boxed(foreign);
assert!(error.root_cause().is::<std::fmt::Error>());
```

## Testing helpers

With the `testing` feature, `report_err::testing` has three assertions that
panic with the report when they fail:

| Function | Passes when |
|---|---|
| `assert_chain_contains(&error, text)` | some link of the chain contains `text` |
| `assert_code(&error, code)` | the error's code is `code` |
| `assert_section(&error, kind, text)` | a section of `kind` contains `text` |

```rust
use report_err::{testing, Context, SectionKind};

let error = std::fs::read("missing.noml").context("loading missing.noml").unwrap_err();
testing::assert_chain_contains(&error, "missing.noml");

let code = define_err::code!("APP", 3);
testing::assert_code(&report_err::err!(code, "x"), code);

let error = report_err::err!("x").suggestion("run the migration");
testing::assert_section(&error, SectionKind::Suggestion, "migration");
```

## Crate root

| Item | What it is |
|---|---|
| `VERSION` | the crate's version, `env!("CARGO_PKG_VERSION")` |
| `Result<T, E = Error>` | the result type ([`Result` and `Ok`](#result-and-ok)) |
| `Ok(value)` | `Ok` with the error type fixed |
| `prelude` | the Tier 1 imports ([The prelude](#the-prelude)) |
| `PANICS_CAUGHT` | whether `boundary` catches panics in this build |
| `Code`, `Severity`, `Category`, `Kinds` | re-exported from define-err |

```rust
assert_eq!(report_err::VERSION, env!("CARGO_PKG_VERSION"));
assert_eq!(report_err::PANICS_CAUGHT, cfg!(panic = "unwind"));
```

## What can fail

report-err never panics on its own account, and only one function returns an
error type of its own:

| Item | Fails with |
|---|---|
| `configure`, `configure_catalogs` | [`ConfigureError`]: `AlreadyConfigured` on a second call; `OutOfRange { field, value, min, max }` for a budget outside its range (`configure` only) |
| `render` | `fmt::Error`, only when the writer fails |
| `downcast` | the error itself, unchanged, when the root is not a `T` |
| `push_child` | `false` when the child's tree does not fit; the child is counted under `Omitted:` |

`ConfigureError` is `#[non_exhaustive]`, as are `RenderStyle`, `SectionKind`,
`StepKind`, and `Step`.

## Examples in the repository

| Example | Shows |
|---|---|
| `quick_start` | context, `bail!`, `ensure!`, and the report |
| `migrate_from_anyhow` | anyhow code with only the crate name changed |
| `codes` | codes, catalogs, and exit codes |
| `attachments` | attachments and sections |
| `inspect` | the chain, downcasting, and searching a tree |
| `exit_report` | `ExitReport` and exit codes |
| `panic_boundary` | `boundary` and `run` |

Run one with `cargo run --example <name>`.

[`Attachments`]: https://docs.rs/report-err/latest/report_err/struct.Attachments.html
[`Category`]: https://docs.rs/report-err/latest/report_err/enum.Category.html
[`Chain`]: https://docs.rs/report-err/latest/report_err/struct.Chain.html
[`Code`]: https://docs.rs/report-err/latest/report_err/struct.Code.html
[`CollectAll`]: https://docs.rs/report-err/latest/report_err/trait.CollectAll.html
[`ConfigureError`]: https://docs.rs/report-err/latest/report_err/enum.ConfigureError.html
[`ErrorRecord`]: https://docs.rs/define-err/latest/define_err/struct.ErrorRecord.html
[`Errors`]: https://docs.rs/report-err/latest/report_err/struct.Errors.html
[`FindAll`]: https://docs.rs/report-err/latest/report_err/struct.FindAll.html
[`Kinds`]: https://docs.rs/report-err/latest/report_err/struct.Kinds.html
[`Limits`]: https://docs.rs/report-err/latest/report_err/struct.Limits.html
[`RenderStyle`]: https://docs.rs/report-err/latest/report_err/enum.RenderStyle.html
[`Section`]: https://docs.rs/report-err/latest/report_err/struct.Section.html
[`SectionKind`]: https://docs.rs/report-err/latest/report_err/enum.SectionKind.html
[`Sections`]: https://docs.rs/report-err/latest/report_err/struct.Sections.html
[`Severity`]: https://docs.rs/report-err/latest/report_err/struct.Severity.html
[`Step`]: https://docs.rs/report-err/latest/report_err/struct.Step.html
[`StepKind`]: https://docs.rs/report-err/latest/report_err/enum.StepKind.html
[`Trail`]: https://docs.rs/report-err/latest/report_err/struct.Trail.html

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

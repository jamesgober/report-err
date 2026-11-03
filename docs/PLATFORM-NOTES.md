<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>report-err</b><br>
    <sub><sup>PLATFORM NOTES</sup></sub>
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

What changes from one target, toolchain, or build profile to another.

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>

## Targets

| Target | Status |
|---|---|
| Linux, macOS, Windows, on x86_64 and aarch64 | tested in CI on stable and 1.81 |
| `no_std` with `alloc` (`default-features = false`), such as `thumbv7em-none-eabihf` | built in CI |
| targets without pointer-sized atomic compare-and-swap (`thumbv6m-none-eabi`) | refused at compile time with a message: the write-once settings and the reported flag need it |

## `std` and `no_std`

Without `std` (`default-features = false`):

- there are no backtraces, no `ExitReport`, no `boundary`, and no `run`;
- an `io::Error` root gives no category (there is no `io::Error`);
- a `Display` that panics is not caught by the guarded renderer, since `catch_unwind` needs `std`; the guard still turns a failing `Display` into `<display failed>`;
- the creation hook has no recursion or panic guard: define-err's contract forbids a hook from creating an error or panicking there;
- `from_panic` still turns a payload into an error, but cannot contain a payload whose `Drop` panics.

## `panic = "abort"`

`boundary` and `run` catch a panic only when panics unwind. Under `panic = "abort"`, the usual release profile in this portfolio, the process aborts at the panic, as `std::panic::catch_unwind` documents, and `boundary` is a plain call. `PANICS_CAUGHT` says which applies to the build. The guarded renderer and the creation hook's panic guard rely on unwinding in the same way. A CI job builds an example with `-C panic=abort` and checks that it aborts.

## Backtraces

A backtrace is captured when an error is created, only when `RUST_LIB_BACKTRACE` or `RUST_BACKTRACE` turns them on (std's rules; `RUST_LIB_BACKTRACE=0` turns them off for errors only), or when `with_backtrace` forces one. std reads the variables once per process. Symbol names depend on the platform's debug information: a release build without debug information prints addresses.

## Type names

The creation hook and the record receive the root's type name from `core::any::type_name`, whose text is not stable across compiler versions (`std::io::Error` has been printed as `std::io::error::Error` and as `core::io::error::Error`). Match on it loosely, if at all.

## Toolchains

MSRV 1.81 (edition 2021), checked in CI. Nothing in the crate depends on a nightly feature; Miri and the sanitizers run on nightly in CI only.

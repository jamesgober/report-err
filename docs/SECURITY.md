<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>SECURITY</b>
</h1>

An error library sits on every failure path, so it is reached with data an attacker chose: a file name, a request body, a value that failed to parse. This file says what report-err defends against, how, what each defence costs, and which test proves it. The design behind it is `dev/PLAN.md` section 10.

## Reporting a vulnerability

Email me@jamesgober.com with the version, a description, and a way to reproduce it. Please do not open a public issue for a vulnerability before a fix is released. You will get an answer within seven days.

| Version | Supported |
|---|---|
| 0.7.x | yes |
| below 0.7 | no: upgrade |

## Keeping secrets out of reports

- An error's `Display` and `Debug` are for the program's own logs. Never send them to a client; send `public_message()` and the category, which never come from `Display` (D-015).
- Wrap passwords, tokens, and keys in define-err's `Secret<T>`: it prints `[redacted]` in `Display` and `Debug`, so formatting it into a context or a note is safe.
- `attach` keeps a value for code that asks for it by type and never prints it. Only `attach_display` prints, and only what you chose to show.
- report-err cannot read intent: a secret formatted into a context with its plain `Display` is printed like any other text.
- Locations name your source files. Build release binaries with `--remap-path-prefix` so they carry no build-machine paths:

```bash
RUSTFLAGS="--remap-path-prefix=$HOME=~" cargo build --release
```

## Threats and defences

Each defence's cost is measured where it says so, and otherwise an estimate [est.]. Each test is in `tests/` unless it names a module.

| Id | Threat | Defence | Test | Cost |
|---|---|---|---|---|
| T1 | A retry loop or a recursive walk adds a context layer on every pass until memory runs out | `max_frames` (64): the innermost 8 and the newest kept, the oldest of the middle dropped and counted; `max_bytes` caps text | `core.rs` `test_attack_million_layers_stay_bounded`: 1,000,000 layers, 64 kept, 999,936 counted | one compare per context [est. under 1 ns] |
| T2 | Large or many attachments, or `collect_all` over an endless stream of failures | `max_attachments` (32), `max_bytes` (64 KiB, counting the heap bytes of `String`, `Vec<u8>`, `Box<str>`, `Box<[u8]>`), `max_tree` (1,024), `max_depth` (16) | `attack.rs` `test_attack_t2_many_large_attachments_stay_bounded`; `groups.rs` `collect_all` over 10,000,000 failures | one compare per push |
| T3 | A password or token in a context or an attachment, printed in a log or a report | `Secret<T>`; opaque attachments; context printed through `Display` only | `attack.rs` `test_attack_t3_secret_canary_never_printed`: the canary through every format, `render`, and `record` | none |
| T4 | A `Display` that panics, fails, writes without end, or writes 100 MiB, inside `main`'s report, a `Drop`, or a panic hook | guarded rendering for `ExitReport`, `render`, `report`, `record`, `unraisable`: each `Display` behind `catch_unwind`, a failure printed as `<display panicked>` or `<display failed>`; output capped at `max_render` (1 MiB), and once full every write fails, so an endless `Display` stops; a section's text and a hook field are formatted when added, into `max_bytes`, whose writer fails once full in the same way | `attack.rs` `test_attack_t4_misbehaving_display_in_every_guarded_path` (also as a note, warning, and suggestion); the `display_adversary` fuzz target | one `catch_unwind` frame per `Display` (free unless it panics), one add per write |
| T5 | A `source()` that returns itself or never ends | `max_chain` (128) | `core.rs` `test_attack_cyclic_and_endless_chains_stop` | one compare per link |
| T6 | Escape sequences, carriage returns, or a fake `Error:` line in text an attacker controls | `ExitReport` and `unraisable` escape C0 controls (except newline and tab), DEL, C1 controls, and the Unicode bidirectional controls as `\u{..}`, and indent continuation lines | `exit.rs`: messages with `ESC ] 0 ;`, `ESC [ 2 J`, `\r`, `\u{9b}`, `\u{202e}`, and `"\nError: fake"` | one scan per byte written to standard error |
| T7 | Millions of errors a second to exhaust memory or CPU in error handling | each error bounded (T1, T2); creation is one allocation and takes no lock; `report()` never blocks | `attack.rs` `test_attack_t7_flood_from_many_threads`; the 8-hour soak (below) | none beyond the operations |
| T8 | Unsoundness in the `unsafe` core: type confusion, double free, use after free, aliasing | `unsafe` only in `src/raw.rs`, each block tied to an invariant (`docs/UNSAFE.md`) | Miri under Stacked Borrows and Tree Borrows with strict provenance on every test; AddressSanitizer on every test; the `ops` fuzz target | none at run time |
| T9 | Allocation failure while creating an error | the allocation failure aborts through `handle_alloc_error`, as everywhere in `alloc`; no emergency pool | `oom.rs`: a failing allocator in a child process, which aborts and does not hang | none |
| T10 | A `Drop` that panics, of a root error, an attachment, or a panic payload | a user `Drop` that panics leaves `drop` like any value's; a panic payload's `Drop` runs inside a guard, up to four times, then is leaked | `attack.rs` `test_attack_t10_root_whose_drop_panics`; `panic.rs` `test_boundary_payload_whose_drop_panics_is_contained` | none unless it happens |
| T11 | Stack exhaustion from deep trees, long chains, or a recursive drop | every walk is iterative with a stack bounded by `max_depth`; drop recursion is bounded by `max_depth` | `attack.rs` `test_attack_t11_full_tree_on_a_64_kib_stack`: build, render, search, record, and drop a full tree on a 64 KiB stack | none |
| T12 | Build paths, backtraces, or private messages in what a client sees | `public_message()` and the category are the client-facing data and never come from `Display`; `--remap-path-prefix` (above) | `attack.rs` `test_attack_t12_public_message_never_carries_private_text` | none |
| T13 | A creation hook that panics, creates errors, or is slow | a thread-local reentrancy guard and `catch_unwind` around the hook (with `std`); a panic leaves the field `creation_hook = "panicked"` | `report.rs` | one acquire load with no hook installed (0.24 to 0.55 ns measured) |
| T14 | A security failure handled locally and never reported | no "local only" mode: `report()` and `unraisable()` always go through define-err's facade; `Kinds::SECURITY` errors are reported like any other | `report.rs`: a reporter receives a `SECURITY` error with its fields | none |
| T15 | Budgets set so high they bound nothing, or to zero by mistake | `configure` checks every field against its range and changes nothing when one is out (`OutOfRange` names it) | `limits.rs`: each field at min - 1, min, max, max + 1; the `limits` fuzz target | at start only |
| T16 | A report that fills a log disk or a terminal | `max_render` (1 MiB) ends guarded output with `(report truncated)` | `exit.rs`, `groups.rs` `test_render_stops_at_max_render` | one add per write |

## Budgets

| Budget | Default | Allowed | What happens at the limit |
|---|---|---|---|
| `max_frames` | 64 | 4 to 4,096 | the oldest middle layer is dropped and counted |
| `max_attachments` | 32 | 0 to 1,024 | the new attachment is dropped and counted |
| `max_sections` | 32 | 0 to 1,024 | the new section is dropped and counted |
| `max_suppressed` | 16 | 0 to 1,024 | the new suppressed error is dropped and counted |
| `max_tree` | 1,024 | 1 to 1,048,576 | the new child is dropped and counted |
| `max_depth` | 16 | 1 to 64 | the new child is dropped and counted |
| `max_bytes` | 64 KiB | 1 KiB to 16 MiB | text is cut at a character boundary with `...(truncated)`; values are refused |
| `max_chain` | 128 | 8 to 4,096 | the walk stops; the report says `(chain truncated)` |
| `max_render` | 1 MiB | 4 KiB to 64 MiB | guarded output stops with `(report truncated)` |

Every dropped item is counted and printed under `Omitted:`. Set them once with `report_err::configure`.

## What report-err cannot defend against

- A secret formatted into text with its plain `Display`.
- A `Display` that panics under `panic = "abort"`: the process aborts at the panic, before any guard can run.
- Memory an attached value owns beyond its own size, for types other than `String`, `Vec<u8>`, `Box<str>`, and `Box<[u8]>`.
- `{}`, `{:#}`, and `{:?}` are not guarded, like any other `Display`: a `Display` that panics or never stops does so there too. Use `render` on paths that must not fail.
- Failures that never become an `Error`.

## How it is tested

- **Miri** with `-Zmiri-strict-provenance`, under Stacked Borrows and under Tree Borrows, on every unit and integration test that does not start a process.
- **AddressSanitizer** on every test, on Linux nightly.
- **Fuzzing** (`fuzz/`, cargo-fuzz): `ops` runs operation sequences and checks the invariants after every step; `display_adversary` renders errors whose `Display` misbehaves at points the fuzzer picks; `limits` runs arbitrary budgets through `configure`. Each ran 24 hours for 0.7.0 with no finding open.
- **Soak** (`soak/`): 8 hours of 64 threads doing every operation, the second half with a creation hook and a reporter installed, on Windows and Linux; every minute the live heap bytes, counted exactly by the soak's own allocator, and the resident memory are sampled. The live heap stays within 1 MiB of where it was after a 5-minute warm-up, and is back within 64 KiB of its starting value when the workers stop. Resident memory stays within 1% after a warm-up of a quarter of each half.
- **`panic = "abort"`**: a CI job builds an example with it and checks that the process aborts.

<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>PERFORMANCE</b>
</h1>

What report-err costs, measured against anyhow 1.0.104, eyre 0.6.14 (its default handler), error-stack 0.8.0, and `Box<dyn Error + Send + Sync>` as the floor. The targets are those of the design (PLAN 11.1); this file says, for each, what was measured and whether it was met.

## How it is measured

- **Where.** In CI, by the `Benchmarks` workflow (`.github/workflows/bench.yml`), on GitHub's hosted runners: Linux x86_64 (`ubuntu-latest`), Windows x86_64 (`windows-latest`), and Linux aarch64 (`ubuntu-24.04-arm`). Each run writes the runner's CPU, OS, and rustc version to its summary; the figures below are from the run named under each table.
- **What is compared.** Every figure in brackets is a ratio: report-err's median time over the other library's, measured in the same run on the same runner. A busy or slow runner moves both sides, so the ratio holds where absolute times do not. Below 1.00, report-err is faster.
- **Build.** `cargo bench` on stable Rust, `opt-level = 3`, fat LTO, one codegen unit; the system allocator (B18 also with mimalloc, except on Windows); backtraces off except in B14. criterion 0.8, 1 s warm-up, 3 s measurement.
- **The code.** `benches/benches/compare.rs` (B1 to B13, the same operation written the usual way in each library), `extra.rs` (B15 to B18, code reads), `backtrace.rs` (B14, a binary of its own because backtraces must be turned on before anything captures), `rd14.rs` (the RD-14 model). `benches/ratios.py` turns criterion's results into the tables.
- **Allocation counts** are not inferred from times: `tests/alloc.rs` counts them with a counting global allocator, one test per claim.
- **The regression gate.** On a pull request the workflow measures the base branch and the change in turn, three times each, on the same runner, and fails when one of report-err's own times is more than 5% and more than 1 ns slower than the base's median in all three rounds (D-065). The two B16 rows that time only the standard library's `collect` are a reference, not gated (D-071). A single comparison failed four runs in a row on a different row and platform each time: shared runners and code alignment move a row by 5% to 20% between builds and between minutes. A slowdown in one or two rounds is printed, not failed. It runs when the base branch has the same suite (D-058). On a quiet machine, the strict form is criterion's own comparison of one base run with one run of the change, failing at 5% at the lower end of the 95% confidence interval (`benches/ratios.py` explains both). A stored ratio cannot be the gate on its own: the hosted runners do not keep one CPU model from run to run, and the shortest rows moved by 30% between runs of the same code. `benches/baseline.json` keeps every ratio for each platform as measured for 0.6.0, and a ratio more than 5% above it is printed as a warning (D-056).

To run it locally:

```bash
cargo bench --manifest-path benches/Cargo.toml -- --noplot
python benches/ratios.py benches/target/criterion local
```

## Footprint

| Measure | Value |
|---|---|
| `size_of::<Error>()`, `size_of::<Result<(), Error>>()`, `size_of::<Option<Error>>()` | 8 bytes (anyhow, eyre, error-stack: 8; `Box<dyn Error>`: 16) |
| heap for a plain error | one allocation: an 80-byte header and the root error inline |
| heap for an error with 1 context layer | the same one allocation |
| heap for 2 to 5 layers | plus one 192-byte block |
| heap for 6 layers and up | plus a vector of 40-byte layers |
| heap for attachments, sections, trace points, a backtrace, children, suppressed errors, or fields | plus the extras, one 472-byte block made on first use (and the 192-byte block if there is none) |

Until 0.6 the extras were inside the layers' block, so an error with two or three layers allocated, zeroed, and freed 656 bytes it mostly did not use (D-052).

## Results for 0.6.0

From the `Benchmarks` run of 2026-11-05 on pull request 5: Linux x86_64 on an Intel Xeon 6975P-C (4 vCPUs), Windows x86_64 on an AMD EPYC 7763 (4 vCPUs), Linux aarch64 on an Arm Neoverse-N2 (4 vCPUs); rustc 1.99.1. Each cell is Linux x86_64 / Windows / Linux aarch64.

### Against anyhow, by target

| Id | Benchmark | report-err / anyhow | Target | Result |
|---|---|---|---|---|
| B1 | `size_of::<Result<(), Error>>()` | 8 / 8 bytes | equal | met |
| B2 | success through a 5-deep `?` chain | 1.00 / 1.00 / 1.03 | within 5% | met |
| B3 | create from `io::Error` through `?` | 0.76 / 0.84 / 1.11 | within 5% | met on x86_64; **missed on aarch64** (11% slower) |
| B4 | `err!("literal")` | 0.87 / 0.92 / 1.09 | within 5% | met on x86_64; **missed on aarch64** (9% slower) |
| B5 | `err!("value {x}")` | 0.96 / 0.92 / 1.08 | within 5% | met on x86_64; **missed on aarch64** (8% slower) |
| B6 | `io_result.context("literal")?` | 0.92 / 1.03 / 1.26 | within 5% | met on x86_64; **missed on aarch64** (26% slower) |
| B7 | `?` then `.context("literal")` | 0.60 / 0.55 / 0.78 | at least 20% faster | met |
| B8 | create plus 3 / 5 / 9 layers | 3: 0.72 / 0.64 / 0.91; 5: 0.64 / 0.52 / 0.71; 9: 0.73 / 0.54 / 0.70 | at least 20% faster | met, except 3 layers on aarch64 (9% faster) |
| B9 | drop of an error with 3 layers | 0.72 / 0.71 / 0.58 | at least 20% faster | met (was 1.30 / 1.00 / 1.04 before D-052) |
| B10 | `downcast_ref` to the root after 3 layers | 0.47 / 0.45 / 0.29 | at least 20% faster | met |
| B11 | `{}` of B8's error | 0.88 / 0.92 / 0.92 | within 5% | met (faster) |
| B12 | `{:#}` of B8's error | 1.33 / 1.55 / 1.51 | at least 20% faster | **missed**: the de-duplication rule formats the root into a buffer to compare it with the layer before it; anyhow does no de-duplication (D-022) |
| B13 | `{:?}` of B8's error | 1.31 / 1.31 / 1.30 | at most 20% slower | **missed**: report-err also prints `Location:` and a four-line `Trace:` block for this error, and runs the de-duplication pass (D-038) |
| B14 | create with backtraces on | 1.00 / 1.05 / 0.95 | within 5% | met (capture-bound: 1 to 4 µs) |
| B15 | creation hook cost, none installed | 0.55 / 0.24 / 0.49 ns | under 1 ns | met |
| B16 | `collect_all` over 10,000 items, 1% failing | 33.4 / 31.1 / 38.7 µs; with none failing 30.1 / 24.8 / 33.1 µs against `collect`'s 31.0 / 25.2 / 32.3 µs | reported | `collect_all` keeps every failure; with none it costs what `collect` does |
| B17 | `boundary` with no panic, over a plain call | 0.00 / -0.17 / 0.00 ns | under 2 ns | met |
| B18 | 64 threads creating and dropping, system allocator | 1.09 / 0.95 / 1.01 | within 5% | met on Windows and aarch64; **missed on Linux x86_64** (9% slower) |
| B18 | the same with mimalloc | 0.92 / - / 1.13 | within 5% | met on x86_64; **missed on aarch64** (13% slower) |

Every missed row is on the pre-1.0 polish list in `dev/ROADMAP.md`, to be fixed, or measured again and listed with its reason, before 1.0.

### Against eyre, error-stack, and `Box<dyn Error>`

| Benchmark | / eyre | / error-stack | / `Box<dyn Error>` |
|---|---|---|---|
| B2 success, 5-deep | 1.03 / 1.00 / 0.68 | 1.03 / 1.00 / 0.47 | 1.06 / 1.03 / 0.52 |
| B3 `?` from `io::Error` | 0.63 / 0.53 / 0.70 | 0.32 / 0.22 / 0.31 | 1.09 / 0.84 / 1.76 |
| B4 `err!("literal")` | 0.48 / 0.36 / 0.51 | 0.33 / 0.22 / 0.32 | 0.76 / 0.51 / 0.96 |
| B5 `err!("value {x}")` | 0.78 / 0.72 / 0.87 | 0.57 / 0.20 / 0.57 | 1.09 / 0.26 / 1.29 |
| B6 context on a result | 0.79 / 0.66 / 0.84 | 0.29 / 0.19 / 0.28 | |
| B7 context after `?` | 0.58 / 0.42 / 0.63 | 0.33 / 0.19 / 0.29 | |
| B8 create plus 3 layers | 0.70 / 0.60 / 0.85 | 0.47 / 0.27 / 0.40 | |
| B9 drop of 3 layers | 0.60 / 0.63 / 0.52 | 0.48 / 0.30 / 0.36 | |
| B10 downcast after 3 layers | 0.41 / 0.40 / 0.30 | 0.03 / 0.01 / 0.02 | 1.40 / 1.17 / 1.39 |
| B11 `{}` | 0.82 / 0.85 / 0.88 | 0.12 / 0.06 / 0.10 | |
| B12 `{:#}` | 1.37 / 1.59 / 1.50 | | |
| B13 `{:?}` | 0.93 / 0.94 / 0.95 | 0.12 / 0.07 / 0.11 | |
| B14 backtraces on | 0.98 / 1.02 / 0.94 | | |

The design expected report-err to be at least 20% faster than eyre on B3 to B8, since eyre allocates a handler with every error: met everywhere except B5 and B6 on aarch64 (13% and 16% faster). `Box<dyn Error>` is the floor: it records no location and no context, makes no backtrace check, and its `Result` is two words.

### The source chain

The cost of walking every context layer through `source()`, the property that sets report-err apart (each layer is a real link, D-003). Cells: 1, 3, 5, 9 layers.

| Benchmark | Linux x86_64 | Windows | Linux aarch64 |
|---|---|---|---|
| hand-written `source()` walk / anyhow | 1.13, 0.91, 0.84, 0.78 | 1.07, 0.88, 0.84, 0.74 | 1.30, 0.74, 0.69, 0.67 |
| `chain().count()` / anyhow | 1.32, 0.67, 0.61, 0.48 | 1.39, 0.83, 0.56, 0.44 | 1.08, 0.58, 0.54, 0.48 |

At one layer report-err is 7% to 39% slower (2 to 3 ns); from three layers it is faster everywhere.

### What 0.6 changed

The first run of these benchmarks (2026-11-03) showed the drop of an error with three layers at 1.30 times anyhow's time on Linux x86_64. Every error with a second layer allocated, zeroed, and freed 656 bytes, 472 of them for extras it seldom had; the extras now have a block of their own (D-052).

| Benchmark | first run | 0.6.0 |
|---|---|---|
| B9 drop of 3 layers / anyhow | 1.30 / 1.00 / 1.04 | 0.72 / 0.71 / 0.58 |
| B8 create plus 3 layers / anyhow | 0.86 / 0.85 / 1.10 | 0.72 / 0.64 / 0.91 |
| B8 create plus 5 layers / anyhow | 0.80 / 0.69 / 0.90 | 0.64 / 0.52 / 0.71 |
| B13 `{:?}` / anyhow | 1.20 / 1.55 / 1.48 | 1.31 / 1.31 / 1.30 |

Two other changes were measured and taken out again: the extras pointer in the header (88 bytes) made B3 and B4 7 to 24 points worse on every platform, and a bufferless repeat check made B12 2.2 to 2.4 times anyhow (D-052).

### One-allocation formatted messages (RD-14)

Formatting `err!("value {x}")` into the error's own block (measure, allocate once, write) instead of into a `String` and the node:

| | Linux x86_64 | Windows | Linux aarch64 |
|---|---|---|---|
| two allocations (as shipped) | 43.96 ns | 61.40 ns | 47.65 ns |
| one allocation, two formatting passes | 48.89 ns | 46.65 ns | 44.59 ns |

Not adopted (D-053): the rule was a gain of at least 20% on every platform, and it was 11% slower on Linux x86_64. It would also have added a dynamically sized node to the `unsafe` core.

### Codes

| Operation | Linux x86_64 / Windows / Linux aarch64 |
|---|---|
| `err!(code, "literal")` | 19.0 / 28.8 / 22.8 ns |
| `code()` set by hand | 0.82 / 0.43 / 1.17 ns |
| `code()` found under 3 layers | 32.9 / 24.7 / 38.5 ns |
| `severity()` found under 3 layers | 40.3 / 29.7 / 49.8 ns |
| `report()` with no reporter installed | 17.7 / 19.7 / 18.7 ns |

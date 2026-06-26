# Real compute-unit benchmarking (`cu-profiler bench`)

The default `cu-profiler` build profiles **recorded logs** — deterministic, fast,
and buildable on every platform. To measure **real** compute units, `bench` runs a
compiled SBF program through [`mollusk-svm`](https://github.com/anza-xyz/mollusk)
and feeds the genuine `compute_units_consumed` into the same report pipeline.

That real measurement lives in the detached
[`integration/cu-profiler-mollusk`](../integration/cu-profiler-mollusk) crate (it
links the full Solana stack, which won't build on Windows). So `bench` is a
**two-binary** design:

- `cu-profiler bench` — ships in the main CLI, on every platform. It validates the
  [bench plan](#the-bench-plan) and, when asked to measure, delegates to…
- `cu-profiler-bench` — the **Linux-only executor** that performs the real Mollusk
  run. It is built from the `cu-profiler-mollusk` crate.

This keeps the main CLI pure-Rust and Windows-buildable while still offering a
one-command path to real CU on Linux/CI.

## 1. Install the executor (Linux)

The executor depends on the workspace crates by path, so it is built from a clone
rather than installed from crates.io:

```sh
git clone https://github.com/MerlijnW70/cu-profiler
cargo install --path cu-profiler/integration/cu-profiler-mollusk --bin cu-profiler-bench
```

`cargo install` puts `cu-profiler-bench` on your `PATH` (`~/.cargo/bin`). You also
need the **Solana SBF toolchain** (`cargo build-sbf`) to compile a program — install
it from <https://release.anza.xyz/stable/install> if you don't have it.

> On a platform other than Linux, `cu-profiler bench --program-name …` fails with a
> clear error naming exactly these commands — it never silently pretends to measure.

## 2. Write a bench plan

A bench plan is a TOML file (default: `bench.toml`; `cu-profiler init` scaffolds a
starter one) listing the instructions to measure:

```toml
[[instruction]]
scenario   = "swap"
program_id = "11111111111111111111111111111111"
data       = "01ab"          # hex-encoded instruction data
# accounts = [ … ]           # optional account metas
```

Validate it on any platform (no executor needed):

```sh
cu-profiler bench --fixtures bench.toml
# bench plan OK: 1 instruction(s)
#   - swap → program 1111… (0 account(s), 2 data byte(s))
```

## 3. Measure for real (Linux)

```sh
# Optionally compile the program first:
cu-profiler bench --build --manifest-path path/to/program --program-name my_program

# Or measure a program that is already built:
cu-profiler bench --fixtures bench.toml --program-name my_program
```

`bench` shells out to `cu-profiler-bench`, which runs each instruction through
Mollusk and reports the real `compute_units_consumed` through the same
table/JSON/Markdown/JUnit/HTML pipeline as a recorded-log run — so a metered run
and a replayed run produce the same shape of report.

## Flags

| Flag | Meaning |
| --- | --- |
| `--fixtures <FILE>` | Bench plan TOML (default `bench.toml`). |
| `--program-name <NAME>` | Measure for real via the executor. Omit to validate only. |
| `--build` | Run `cargo build-sbf` in `--manifest-path` before measuring. |
| `--manifest-path <DIR>` | Program crate to build with `--build`. |

## See also

- [Architecture](architecture.md) — how the backends share one pipeline.
- [CI](ci.md) — exit codes and gating.
- [`integration/cu-profiler-mollusk`](../integration/cu-profiler-mollusk) — the executor crate.

# cu-profiler

[![CI](https://github.com/MerlijnW70/cu-profiler/actions/workflows/ci.yml/badge.svg)](https://github.com/MerlijnW70/cu-profiler/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

**Compute-unit profiling, regression testing and budget enforcement for Solana programs — 100% Rust.**

`cu-profiler` measures how many compute units (CU) your Solana program uses,
explains *where* the compute goes (per instruction, per CPI, per marked scope),
compares runs against a baseline, and fails CI when compute regresses or exceeds
budget. Think *gas snapshots for Solana*, with scenarios, CPI call trees, scope
markers, baselines, budget policies, and JSON/Markdown/JUnit/HTML output.

```
Solana compute observability
+ regression testing
+ budget enforcement
+ scenario intelligence
+ CPI attribution
+ CI-native reporting
```

## Project status

`cu-profiler` is in active development. The default build profiles **recorded
Solana logs** — deterministic, fast, CI-friendly, and the substrate the whole
parser/report/baseline/budget pipeline is built and tested on.

**Real compute-unit metering** is available today via the **Mollusk backend**
([`integration/cu-profiler-mollusk`](integration/cu-profiler-mollusk)): it runs a
compiled **SBF** program through [`mollusk-svm`](https://github.com/anza-xyz/mollusk)
and feeds genuine `compute_units_consumed` into the same pipeline. A live
[`solana-program-test`](integration/cu-profiler-program-test) backend also exists.
Both live in **detached crates** (the Solana stack is heavy and won't build on
Windows) and are verified on Linux CI — so the core crates stay pure Rust.

> **Why recorded logs in the default build?** They keep the core deterministic,
> fast, and buildable everywhere; the live Solana backends build on the *same*
> core pipeline, so a metered run and a replayed run produce the same report.

## Workspace

| Crate | Purpose |
| --- | --- |
| [`cu-profiler-core`](crates/cu-profiler-core) | Domain model, log parser, CPI tree, scope markers, budget engine, baselines, confidence, diagnostics |
| [`cu-profiler-report`](crates/cu-profiler-report) | Rendering: table, JSON, Markdown, JUnit, HTML |
| [`cu-profiler-cli`](crates/cu-profiler-cli) | `cu-profiler` binary (thin wrapper over the library) |
| [`cu-profiler-instrumentation`](crates/cu-profiler-instrumentation) | Opt-in scope markers for your program |
| [`integration/cu-profiler-mollusk`](integration/cu-profiler-mollusk) | Live `mollusk-svm` backend — **real CU metering** of an SBF program (Linux) |
| [`integration/cu-profiler-program-test`](integration/cu-profiler-program-test) | Live `solana-program-test` backend (Linux) |

## Quickstart

```sh
cargo run -p cu-profiler-cli -- init                 # scaffold config + example logs
cargo run -p cu-profiler-cli -- run                  # reads recorded logs from .cu/logs by default
cargo run -p cu-profiler-cli -- run --logs-dir .cu/logs   # ...or point at logs explicitly
cargo run -p cu-profiler-cli -- baseline save
cargo run -p cu-profiler-cli -- compare              # fail (exit 1) on regression
```

> The default CLI reads **recorded logs** (`.cu/logs/<scenario>.log`). For
> **real metered CU**, drive the same pipeline from the
> [Mollusk backend](integration/cu-profiler-mollusk). See [Project status](#project-status).

### Profiling a real transaction

`init` scaffolds **demo** logs (a `run` on them prints a warning to stderr).
To profile a *real* on-chain transaction, import its logs. Fetch them live by
signature (rustls — no OpenSSL):

```sh
cu-profiler import --signature <SIGNATURE> --rpc https://your-rpc        # live fetch
# ...or from a getTransaction JSON file:
solana confirm -v <SIGNATURE> --output json > tx.json
cu-profiler import tx.json --name my_swap
# then add `[scenario.<name>]` to cu-profiler.toml and:
cu-profiler run --scenario <name>
```

`--signature` requires the `remote` feature (on by default). Public RPCs are
rate-limited — pass your own `--rpc` for reliable fetches.

cu-profiler reconstructs the real CPI call tree and per-program CU from nothing
but the logs — no live validator or Solana toolchain needed.

### Example output

```
Scenario         Actual CU   Budget  Delta  Status
initialize_pool     78,902   80,000      -  WARN
swap_exact_in       96,812  100,000      -  WARN

2 scenario(s): 0 passed, 2 warned, 0 failed — 175,714 total CU
```

## How v1 works

v1 drives scenarios from **recorded logs** (`.cu/logs/<scenario>.log`) through the
`RecordedLogsBackend`. This makes the parser, reports and CI logic fully testable
without a live runtime. Live backends (`solana-program-test`, `BanksClient`) have
defined interfaces and land in a later release — see [docs/architecture.md](docs/architecture.md).

## CI

The quickest path is the **reusable GitHub Action** — one line, no toolchain setup
in your workflow:

```yaml
- uses: actions/checkout@v4
- uses: MerlijnW70/cu-profiler@v1
  with:
    command: ci
    args: --config cu-profiler.toml
- uses: actions/upload-artifact@v4
  with: { name: cu-profiler-report, path: target/cu-profiler/ }
```

The action installs the published `cu-profiler` CLI and runs the subcommand you
give it (`command` defaults to `ci`, `args` to `--config cu-profiler.toml`; pin the
CLI with `version:`). Or call the CLI directly if you already manage Rust in CI:

```yaml
- run: cargo run -p cu-profiler-cli -- ci --config cu-profiler.toml
```

Exit codes are stable and documented in [docs/ci.md](docs/ci.md):
`0` success · `1` budget/regression · `2` config · `3` simulation · `4` stale baseline · `5` parser/report · `6` low confidence (strict).

## Confidence

Every measurement carries a confidence score (`High`/`Medium`/`Low`/`Unknown`) and
always explains *why* it is not `High`. The tool is deliberately honest about what
it can and cannot know.

## What it does not do

`cu-profiler` does **not** claim automatic source-line attribution.
Function/module attribution is only reported when explicit profiler markers or
reliable runtime logs support it. Where the evidence isn't there, the tool says
so via the confidence score rather than inventing precision.

## Limitations

- **Function-level attribution requires explicit markers.** There is no automatic
  source-line profiling.
- **`program-test` results may differ from mainnet** runtime conditions.
- **Instrumentation adds overhead**, which the profiler reports.
- **Baselines are only valid when their fingerprints match.**

## Documentation

[Reference](docs/reference.md) ·
[Architecture](docs/architecture.md) ·
[Real-CU bench](docs/bench.md) ·
[Scenarios](docs/scenarios.md) ·
[Baselines](docs/baselines.md) ·
[CI](docs/ci.md) ·
[Instrumentation](docs/instrumentation.md) ·
[Report schema](docs/report-schema.md) ·
[Config](docs/config.md) ·
[Anchor](docs/anchor.md) ·
[Roadmap](ROADMAP.md)

## Development

```sh
cargo build  --workspace --all-features
cargo test   --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
```

## License

MIT OR Apache-2.0.

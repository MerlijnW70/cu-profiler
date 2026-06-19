# cu-profiler

**Compute-unit profiling, regression testing and budget enforcement for Solana programs — 100% Rust.**

`cu-profiler` measures how many compute units (CU) your Solana program uses,
explains *where* the compute goes (per instruction, per CPI, per marked scope),
compares runs against a baseline, and fails CI when compute regresses or exceeds
budget. Think *gas snapshots for Solana*, with scenarios, CPI call trees, scope
markers, baselines, budget policies, and JSON/Markdown/JUnit output.

```
Solana compute observability
+ regression testing
+ budget enforcement
+ scenario intelligence
+ CPI attribution
+ CI-native reporting
```

## Project status

`cu-profiler` is in active development. The current **v1** pipeline profiles
**recorded Solana logs** and provides the parser, reporting, baseline, budget and
CI behaviour. Live simulation backends (`solana-program-test`, `BanksClient`) are
designed but **not yet enabled by default** — don't expect live on-chain
simulation yet.

> **Why recorded logs first?** Recorded logs make the first release
> deterministic, fast, CI-friendly and easy to test. The live Solana simulation
> backends build on the same core pipeline later.

## Workspace

| Crate | Purpose |
| --- | --- |
| [`cu-profiler-core`](crates/cu-profiler-core) | Domain model, log parser, CPI tree, scope markers, budget engine, baselines, confidence, diagnostics |
| [`cu-profiler-report`](crates/cu-profiler-report) | Rendering: table, JSON, Markdown, JUnit |
| [`cu-profiler-cli`](crates/cu-profiler-cli) | `cu-profiler` binary (thin wrapper over the library) |
| [`cu-profiler-instrumentation`](crates/cu-profiler-instrumentation) | Opt-in scope markers for your program |

## Quickstart

```sh
cargo run -p cu-profiler-cli -- init                 # scaffold config + example logs
cargo run -p cu-profiler-cli -- run                  # reads recorded logs from .cu/logs by default
cargo run -p cu-profiler-cli -- run --logs-dir .cu/logs   # ...or point at logs explicitly
cargo run -p cu-profiler-cli -- baseline save
cargo run -p cu-profiler-cli -- compare              # fail (exit 1) on regression
```

> v1 reads **recorded logs** (`.cu/logs/<scenario>.log`); it does not run a live
> validator. See [Project status](#project-status).

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

```yaml
- run: cargo run -p cu-profiler-cli -- ci --config cu-profiler.toml
- uses: actions/upload-artifact@v4
  with: { name: cu-profiler-report, path: target/cu-profiler/ }
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
[Scenarios](docs/scenarios.md) ·
[Baselines](docs/baselines.md) ·
[CI](docs/ci.md) ·
[Instrumentation](docs/instrumentation.md) ·
[Report schema](docs/report-schema.md) ·
[Config](docs/config.md) ·
[Anchor](docs/anchor.md)

## Development

```sh
cargo build  --workspace --all-features
cargo test   --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
```

## License

MIT OR Apache-2.0.

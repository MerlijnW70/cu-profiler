# Architecture

`cu-profiler` is **library-first**: all logic lives in `cu-profiler-core`, the CLI
is a thin wrapper, and rendering is isolated in `cu-profiler-report`.

## Crate boundaries

```
cu-profiler-cli  ──uses──►  cu-profiler-core  ◄──renders── cu-profiler-report
                                  ▲
                                  │ emits markers parsed by core
                       cu-profiler-instrumentation
```

- **core** depends on no CLI code and no live Solana runtime by default.
- **report** depends only on core's data model; it formats, it does not analyse.
- **cli** depends on core + report; it parses args, picks an exit code, prints.
- **instrumentation** has no Solana dependency; it shares the marker wire format
  with core's parser so the two cannot drift.

## Data flow

```
Scenario ─► ExecutionBackend ─► logs ─► parser::analyze ─► ParseAnalysis
                                                              │
        ┌─────────────────────────────────────────────────────┤
        ▼                ▼                 ▼              ▼      ▼
   Measurement   budget::evaluate   confidence::score   baseline   diagnostics
        └────────────────────────── ScenarioReport ──────────────────┘
                                       │
                                    Report ─► cu-profiler-report ─► table/json/md/junit
```

## Execution backends

The core abstracts execution behind the `ExecutionBackend` trait:

| Backend | Status in v1 |
| --- | --- |
| `RecordedLogsBackend` | ✅ fully implemented (tests, fixtures, `inspect`) |
| `ProgramTestBackend` (live) | ✅ implemented in the detached `integration/cu-profiler-program-test` crate (real `solana-program-test` runtime) |
| `ProgramTestBackend` / `BanksClientBackend` (core stubs) | interface defined; return `BackendUnimplemented` in the Solana-free default build |
| `RpcSimulationBackend` | designed for later |

> The live `program-test` backend lives in a **workspace-detached** crate
> because the Solana stack is heavy and `openssl-sys` does not build on Windows.
> The core crates and the local quality gate stay Solana-free; a dedicated Linux
> CI job builds and tests the live backend.

Keeping the Solana dependency out of the default build keeps the core pure Rust
and fast to compile, and lets the entire pipeline be developed against recorded
logs first.

## Module map (`cu-profiler-core`)

```
error          typed, actionable errors
scenario       the first-class Scenario benchmark
config         cu-profiler.toml parsing
metadata       run/backend/instrumentation metadata
program_registry  program-ID → label
parser/        solana_logs · compute_budget · cpi_tree · scope_markers · analyze
budget/        policy · result · evaluation engine
baseline/      fingerprint · compare · store
confidence     scoring
diagnostics/   rules · engine
model          the serializable Report aggregate
profiler       the orchestrator
backend/       recorded · program_test · banks_client
```

## Principles

library-first · CLI thin wrapper · typed domain model · structured reports ·
stable schema · **no fake precision** · explicit confidence · feature-gated
integrations · deterministic CI behaviour · clear errors · minimal global state ·
no hidden side effects.

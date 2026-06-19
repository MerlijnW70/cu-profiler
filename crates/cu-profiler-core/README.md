# cu-profiler-core

Core engine for [`cu-profiler`](https://github.com/MerlijnW70/cu-profiler) — the
Solana compute-unit profiling, regression-testing and budget-enforcement toolkit.

This crate owns the domain model, the Solana log parser (CPI call tree, scope
markers, compute-budget heuristics), the budget policy engine, baselines with
input fingerprinting, confidence scoring, and the diagnostic engine. It depends on
no CLI code and no live Solana runtime by default: the `RecordedLogsBackend`
drives the whole pipeline from logs.

```rust
use cu_profiler_core::backend::RecordedLogsBackend;
use cu_profiler_core::metadata::RunMetadata;
use cu_profiler_core::scenario::Scenario;
use cu_profiler_core::Profiler;

let mut backend = RecordedLogsBackend::new();
backend.insert_blob(
    "swap",
    "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
    true,
);
let report = Profiler::new().run(
    &backend, &[Scenario::new("swap")], None, RunMetadata::recorded(cu_profiler_core::VERSION),
);
assert_eq!(report.scenarios[0].measurement.total_cu, 1000);
```

See the [project README](https://github.com/MerlijnW70/cu-profiler) for the full
picture. Licensed under MIT OR Apache-2.0.

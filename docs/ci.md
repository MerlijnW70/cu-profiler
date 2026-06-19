# CI

`cu-profiler ci` is a deterministic mode for pipelines: no interactive prompts,
stable exit codes, and artifacts written to the paths in `cu-profiler.toml`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Budget or regression failure |
| `2` | Configuration error |
| `3` | Simulation failure (e.g. a scenario's logs were missing) |
| `4` | Stale or missing baseline |
| `5` | Parser / report error |
| `6` | Low confidence in strict mode |

A scenario that cannot be simulated outranks every soft signal and returns `3`.

## What fails the run

Configured by `[defaults]` in `cu-profiler.toml`, and tightenable per-invocation:

- `fail_on_budget` — an absolute budget was exceeded.
- `fail_on_regression` — compute regressed past the policy versus baseline.
- `fail_on_stale_baseline` — the baseline fingerprint no longer matches.
- `--strict` / `--fail-on-low-confidence` — a measurement is low-confidence.

## GitHub Actions

```yaml
name: CU Profiler

on:
  pull_request:
  push:
    branches: [main]

jobs:
  cu-profiler:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --workspace
      - run: cargo test --workspace
      - run: cargo run -p cu-profiler-cli -- ci --config cu-profiler.toml
      - uses: actions/upload-artifact@v4
        with:
          name: cu-profiler-report
          path: target/cu-profiler/
```

`cu-profiler init --workflow` scaffolds this file for you.

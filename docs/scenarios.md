# Scenarios

A **scenario** is not just a test — it is a reproducible compute benchmark with an
expected outcome, a budget policy, tags, and metadata used for fingerprinting.

## Fields

| Field | Meaning |
| --- | --- |
| `name` | Stable, hierarchical name, e.g. `swap/referral_enabled` |
| `description` | What the scenario exercises |
| `tags` | Free-form labels for `--tag` filtering |
| `criticality` | `critical` / `normal` / `low` |
| `owner` | Team or person for triage |
| `expected` | `success` or `failure` |
| `budget` | The budget policy (see [config](config.md)) |
| `samples` | Number of samples (≥ 1) |

## Failure paths are first-class

A failing instruction that burns a lot of CU matters for both performance and
security. Mark such scenarios `expected = "failure"`; the profiler treats an
unexpected outcome (success when failure was expected, or vice-versa) as a
`FAIL`, and flags an *expensive failure path* diagnostic when a rejected
transaction consumes significant compute.

## Example names

```
swap/happy_path
swap/large_pool
swap/referral_enabled
swap/token_2022
swap/failure_invalid_owner
initialize_pool/minimal
initialize_pool/full_config
liquidate/max_position
liquidate/stale_oracle_failure
```

## Defining scenarios

In v1, scenarios are declared in `cu-profiler.toml` under `[scenario.<name>]` and
driven from recorded logs in `.cu/logs/<name>.log`. `examples/scenarios.rs` shows
how the same scenarios will be built programmatically once a live backend is wired
in.

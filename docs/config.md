# Configuration (`cu-profiler.toml`)

Configuration is parsed strictly — unknown keys are rejected — and every failure
becomes a clear error message.

## Full example

```toml
[project]
name = "my-solana-program"
program_id = "..."        # optional
mode = "recorded"         # recorded | program-test | banks-client

[defaults]
warn_at_budget_pct = 90
max_regression_pct = 5
fail_on_budget = true
fail_on_regression = true
fail_on_stale_baseline = false

[output]
default_format = "table"  # table | json | markdown | junit
json_path = "target/cu-profiler/report.json"
markdown_path = "target/cu-profiler/report.md"
junit_path = "target/cu-profiler/junit.xml"

[program_labels]
"11111111111111111111111111111111" = "System Program"

[scenario.swap_exact_in]
budget = 100000
warn_at_budget_pct = 90
max_regression_pct = 5
critical = true
tags = ["swap", "hot-path", "user-facing"]

[scenario.initialize_pool]
budget = 80000
max_regression_pct = 3
critical = true
tags = ["admin", "setup"]

# Optional: label the program from its Anchor IDL (build with --features anchor).
[anchor]
idl = "target/idl/my_program.json"
```

## Sections

- **`[project]`** — identity and execution `mode` (default `recorded`). The CLI
  profiles **recorded logs**; setting `mode` to a live backend
  (`program-test`/`banks-client`/`mollusk`/`rpc-simulation`) prints a note that
  those run library-only (the `integration/*` crates), and `run`/`ci` still
  profile recorded logs. An unknown `mode` is a config error.
- **`[defaults]`** — the baseline budget policy and CI gating switches. Per-scenario
  settings overlay these.
- **`[output]`** — default render format and artifact paths used by `ci`.
- **`[program_labels]`** — extra program-ID → label entries, merged over the
  built-in well-known IDs (System, Compute Budget, SPL Token, Token-2022, ATA,
  Memo). Unknown programs render as `Unknown Program <pubkey>`.
- **`[scenario.<name>]`** — per-scenario budget, thresholds, criticality and tags.
- **`[anchor]`** — optional Anchor IDL path; takes effect when the binary is
  built with `--features anchor` (see [anchor](anchor.md)).

## Effective policy

A scenario's effective [budget policy](baselines.md) is the project defaults
overlaid by its own settings: any field set on the scenario wins, the rest flows
through from `[defaults]`.

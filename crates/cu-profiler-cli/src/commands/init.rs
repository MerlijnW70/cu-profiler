//! `cu-profiler init` — scaffold a config, example recorded logs, an example
//! scenario file and an optional CI workflow.

use std::path::Path;

use cu_profiler_core::Result;

use crate::args::InitArgs;
use crate::exit::ExitCode;

const CONFIG_TOML: &str = r#"[project]
name = "my-solana-program"
mode = "recorded"

[defaults]
warn_at_budget_pct = 90
max_regression_pct = 5
fail_on_budget = true
fail_on_regression = true
fail_on_stale_baseline = false

[output]
default_format = "table"
json_path = "target/cu-profiler/report.json"
markdown_path = "target/cu-profiler/report.md"
junit_path = "target/cu-profiler/junit.xml"
html_path = "target/cu-profiler/report.html"

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
"#;

const SWAP_LOG: &str = "# DEMO_DATA_ONLY - scaffolded example, not a real measurement
Program SwapPRogram1111111111111111111111111111 invoke [1]
Program log: CU_PROFILER_BEGIN name=swap::validate_accounts
Program log: CU_PROFILER_END name=swap::validate_accounts
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 4200 of 195000 compute units
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success
Program SwapPRogram1111111111111111111111111111 consumed 96812 of 200000 compute units
Program SwapPRogram1111111111111111111111111111 success
";

const INIT_POOL_LOG: &str = "# DEMO_DATA_ONLY - scaffolded example, not a real measurement
Program PoolPRogram1111111111111111111111111111 invoke [1]
Program System11111111111111111111111111111111 invoke [2]
Program System11111111111111111111111111111111 success
Program PoolPRogram1111111111111111111111111111 consumed 78902 of 200000 compute units
Program PoolPRogram1111111111111111111111111111 success
";

const EXAMPLE_SCENARIOS: &str = r#"//! Example scenarios.
//!
//! In v1 the CLI drives scenarios from recorded logs under `.cu/logs/`. This
//! file shows how you would build `Scenario` values programmatically once a live
//! `program-test` backend is wired in.

use cu_profiler_core::budget::BudgetPolicy;
use cu_profiler_core::scenario::{Criticality, Scenario};

pub fn swap_exact_in() -> Scenario {
    let mut s = Scenario::new("swap_exact_in");
    s.description = "Single-hop exact-in swap on the happy path".to_string();
    s.tags = vec!["swap".into(), "hot-path".into()];
    s.criticality = Criticality::Critical;
    s.budget = BudgetPolicy {
        absolute_max_cu: Some(100_000),
        warn_at_budget_pct: Some(90.0),
        max_regression_pct: Some(5.0),
        ..Default::default()
    };
    s
}
"#;

const WORKFLOW: &str = r#"name: CU Profiler

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
"#;

/// Execute the `init` command.
pub fn run(args: &InitArgs, quiet: bool) -> Result<ExitCode> {
    let dir = &args.dir;
    write_file(
        &dir.join("cu-profiler.toml"),
        CONFIG_TOML,
        args.force,
        quiet,
    )?;
    write_file(
        &dir.join(".cu/logs/swap_exact_in.log"),
        SWAP_LOG,
        args.force,
        quiet,
    )?;
    write_file(
        &dir.join(".cu/logs/initialize_pool.log"),
        INIT_POOL_LOG,
        args.force,
        quiet,
    )?;
    write_file(
        &dir.join("examples/scenarios.rs"),
        EXAMPLE_SCENARIOS,
        args.force,
        quiet,
    )?;
    if args.workflow {
        write_file(
            &dir.join(".github/workflows/cu-profiler.yml"),
            WORKFLOW,
            args.force,
            quiet,
        )?;
    }
    if !quiet {
        println!("cu-profiler initialised. Try: cu-profiler run");
    }
    Ok(ExitCode::Success)
}

fn write_file(path: &Path, contents: &str, force: bool, quiet: bool) -> Result<()> {
    if path.exists() && !force {
        if !quiet {
            println!("skip (exists): {}", path.display());
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, contents)?;
    if !quiet {
        println!("wrote {}", path.display());
    }
    Ok(())
}

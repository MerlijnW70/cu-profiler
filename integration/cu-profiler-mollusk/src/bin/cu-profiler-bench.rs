//! `cu-profiler-bench` — turnkey real-CU measurement from a declarative bench plan.
//!
//! Linux-only (it links the Solana/Mollusk stack, which does not build on Windows).
//! Reads a `bench.toml`, runs every instruction through Mollusk to meter real compute
//! units, and renders the report. This is the one-command path that keeps the main
//! `cu-profiler` CLI Solana-free.
//!
//! ```text
//! cu-profiler-bench --fixtures bench.toml --program-name my_program [--format table]
//! ```
//! The program is loaded by name from `$SBF_OUT_DIR` (build it with `cargo build-sbf`).

use std::process::ExitCode;

use cu_profiler_core::bench::BenchPlan;
use cu_profiler_mollusk::run_plan;
use cu_profiler_report::{render, Format};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cu-profiler-bench: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fixtures = flag(&args, "--fixtures").unwrap_or_else(|| "bench.toml".to_string());
    let program_name = flag(&args, "--program-name").ok_or_else(|| {
        "missing --program-name <so-stem> (the program built with `cargo build-sbf`)".to_string()
    })?;
    let format = flag(&args, "--format").unwrap_or_else(|| "table".to_string());

    let text =
        std::fs::read_to_string(&fixtures).map_err(|e| format!("cannot read `{fixtures}`: {e}"))?;
    let plan = BenchPlan::from_toml(&text).map_err(|e| e.to_string())?;
    let report = run_plan(&plan, &program_name).map_err(|e| e.to_string())?;
    let fmt: Format = format
        .parse()
        .map_err(|e: cu_profiler_core::Error| e.to_string())?;
    let rendered = render(&report, fmt).map_err(|e| e.to_string())?;
    print!("{rendered}");
    Ok(())
}

/// The value following `name` in `args`, if present.
fn flag(args: &[String], name: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == name)?;
    args.get(pos + 1).cloned()
}

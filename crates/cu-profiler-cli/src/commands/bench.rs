//! `cu-profiler bench` — turnkey real-CU path.
//!
//! `bench` validates a declarative [`BenchPlan`],
//! optionally builds the program with `cargo build-sbf`, then **delegates the real
//! Mollusk measurement** to the Linux-only `cu-profiler-bench` executor, found over
//! `PATH` (a runtime sibling, never a build dependency — so the main CLI keeps the
//! Solana/Mollusk stack out and stays Windows-buildable).
//!
//! - With `--program-name`: run the executor and forward its result; if the executor
//!   is not installed, fail with the exact command to run (no silent half-measure).
//! - Without `--program-name`: validate the plan and summarise it (a lint/prepare run).

use std::path::Path;
use std::process::Command;

use cu_profiler_core::bench::BenchPlan;
use cu_profiler_core::{Error, Result};

use crate::args::BenchArgs;
use crate::commands::{MAX_LOG_BYTES, read_to_string_capped};
use crate::exit::ExitCode;

/// The Linux-only sibling binary that performs the real Mollusk measurement.
const EXECUTOR: &str = "cu-profiler-bench";

/// How to obtain the executor. It lives in the detached `cu-profiler-mollusk`
/// crate (path-deps + the Solana stack), so it is built from a clone rather than
/// installed from crates.io — these are the exact, verified commands.
const INSTALL_HINT: &str = "install it on Linux with:\n  \
     git clone https://github.com/MerlijnW70/cu-profiler\n  \
     cargo install --path cu-profiler/integration/cu-profiler-mollusk --bin cu-profiler-bench\n  \
     (see docs/bench.md for the full walkthrough)";

/// Execute the `bench` command.
pub fn run(args: &BenchArgs, quiet: bool) -> Result<ExitCode> {
    let text = read_to_string_capped(&args.fixtures, MAX_LOG_BYTES)?;
    let plan = BenchPlan::from_toml(&text)?;

    if args.build {
        build_sbf(&args.manifest_path, quiet)?;
    }

    // With a program, measure for real via the executor; without one, validate only.
    let Some(program_name) = &args.program_name else {
        if !quiet {
            summarise(&plan);
        }
        return Ok(ExitCode::Success);
    };

    match delegate(&args.fixtures, program_name) {
        Some(code) => Ok(code),
        None => Err(Error::Simulation(format!(
            "plan is valid, but the `{EXECUTOR}` executor was not found on PATH, so no compute \
             units were measured. It is Linux-only (it links the Solana stack); {INSTALL_HINT}\n\
             then measure with:\n  {EXECUTOR} --fixtures {} --program-name {program_name}",
            args.fixtures.display()
        ))),
    }
}

/// Run `cargo build-sbf` in `dir` to compile the program to an `.so`.
fn build_sbf(dir: &Path, quiet: bool) -> Result<()> {
    if !quiet {
        eprintln!(
            "building program with `cargo build-sbf` in {}…",
            dir.display()
        );
    }
    let status = Command::new("cargo")
        .arg("build-sbf")
        .current_dir(dir)
        .status()
        .map_err(|e| {
            Error::Simulation(format!(
                "could not run `cargo build-sbf` (is the Solana SBF toolchain installed?): {e}"
            ))
        })?;
    if !status.success() {
        return Err(Error::Simulation(
            "`cargo build-sbf` failed — see its output above".to_string(),
        ));
    }
    Ok(())
}

/// Run the `cu-profiler-bench` executor, inheriting its stdout/stderr and returning a
/// mapped exit code — or `None` when the executor is not on `PATH`.
fn delegate(fixtures: &Path, program_name: &str) -> Option<ExitCode> {
    let status = Command::new(EXECUTOR)
        .arg("--fixtures")
        .arg(fixtures)
        .arg("--program-name")
        .arg(program_name)
        .status()
        .ok()?;
    Some(if status.success() {
        ExitCode::Success
    } else {
        ExitCode::Simulation
    })
}

/// Validate-only output: print the parsed plan and how to measure it.
fn summarise(plan: &BenchPlan) {
    println!("bench plan OK: {} instruction(s)", plan.instructions.len());
    for ix in &plan.instructions {
        println!(
            "  - {} → program {} ({} account(s), {} data byte(s))",
            ix.scenario,
            ix.program_id,
            ix.accounts.len(),
            ix.data.len() / 2,
        );
    }
    eprintln!(
        "note: plan validated. To measure real compute units, pass --program-name with the \
         `{EXECUTOR}` executor on PATH — {INSTALL_HINT}"
    );
}

//! `cu-profiler bench` — the turnkey real-CU path (scaffolding).
//!
//! `bench` reads a declarative [`BenchPlan`](cu_profiler_core::bench::BenchPlan) of
//! instructions, validates it, and (optionally) builds the program with
//! `cargo build-sbf`, resolving the compiled `.so`. It is the CLI surface for the
//! one SOTA soft-spot versus Mollusk: a one-command real-CU measurement.
//!
//! The **live Mollusk execution** that turns a validated plan into real
//! compute-unit numbers lives in the Linux-only `cu-profiler-mollusk` integration
//! crate (the Solana/SBF stack does not build on every host the core targets). This
//! command therefore prepares and validates the plan today; wiring the execution is
//! a focused follow-up that the Linux SBF CI job validates.

use std::path::{Path, PathBuf};
use std::process::Command;

use cu_profiler_core::bench::BenchPlan;
use cu_profiler_core::{Error, Result};

use crate::args::BenchArgs;
use crate::commands::{MAX_LOG_BYTES, read_to_string_capped};
use crate::exit::ExitCode;

/// Execute the `bench` command.
pub fn run(args: &BenchArgs, quiet: bool) -> Result<ExitCode> {
    let text = read_to_string_capped(&args.fixtures, MAX_LOG_BYTES)?;
    let plan = BenchPlan::from_toml(&text)?;

    if args.build {
        build_sbf(&args.manifest_path, quiet)?;
    }
    let artifact = resolve_artifact(args.program.as_deref(), args.program_name.as_deref());

    if !quiet {
        report_plan(&plan, artifact.as_deref());
    }
    Ok(ExitCode::Success)
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

/// Resolve the compiled program `.so`: an explicit `--program`, else the
/// `SBF_OUT_DIR`/`target/deploy` convention for `--program-name`. Returns the first
/// candidate that exists. Pure but for the env/exists checks, so it is unit-tested
/// via [`artifact_candidates`].
fn resolve_artifact(explicit: Option<&Path>, program_name: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let name = program_name?;
    let sbf_out = std::env::var("SBF_OUT_DIR").ok();
    artifact_candidates(sbf_out.as_deref(), Path::new("target/deploy"), name)
        .into_iter()
        .find(|p| p.exists())
}

/// The ordered `.so` lookup paths for `name`: `$SBF_OUT_DIR` first, then the
/// `target/deploy` convention. Pure — existence is checked by the caller.
fn artifact_candidates(sbf_out_dir: Option<&str>, deploy_dir: &Path, name: &str) -> Vec<PathBuf> {
    let file = format!("{name}.so");
    let mut out = Vec::new();
    if let Some(dir) = sbf_out_dir {
        out.push(Path::new(dir).join(&file));
    }
    out.push(deploy_dir.join(&file));
    out
}

/// Print the validated plan and the boundary note (execution is the Linux follow-up).
fn report_plan(plan: &BenchPlan, artifact: Option<&Path>) {
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
    match artifact {
        Some(p) => println!("program artifact: {}", p.display()),
        None => println!(
            "program artifact: not resolved (pass --program or --build with --program-name)"
        ),
    }
    eprintln!(
        "note: live compute-unit execution runs on the Linux `cu-profiler-mollusk` backend; \
         this build validates and prepares the plan only."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_program_wins() {
        let p = PathBuf::from("some/program.so");
        assert_eq!(resolve_artifact(Some(&p), None), Some(p));
    }

    #[test]
    fn candidates_prefer_sbf_out_dir_then_deploy() {
        let c = artifact_candidates(Some("/out"), Path::new("target/deploy"), "amm");
        assert_eq!(c.len(), 2);
        assert!(c[0].ends_with("amm.so"));
        assert!(c[0].to_string_lossy().contains("out"));
        assert!(c[1].ends_with("amm.so"));
        assert!(c[1].to_string_lossy().contains("deploy"));
    }

    #[test]
    fn candidates_without_sbf_out_dir_is_deploy_only() {
        let c = artifact_candidates(None, Path::new("target/deploy"), "amm");
        assert_eq!(c.len(), 1);
        assert!(c[0].ends_with("amm.so"));
    }
}

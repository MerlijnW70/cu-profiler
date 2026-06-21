//! `clap` argument definitions. This module is pure declaration; behaviour lives
//! in `commands`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// `cu-profiler` — compute-unit profiling, regression testing and budget
/// enforcement for Solana programs.
#[derive(Debug, Parser)]
#[command(name = "cu-profiler", version, about)]
pub struct Cli {
    /// Increase verbosity (`-v`, `-vv`).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all non-error output.
    #[arg(long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Emit full trace-level diagnostics for the tool itself.
    #[arg(long, global = true)]
    pub trace: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scaffold configuration, example logs and an optional CI workflow.
    Init(InitArgs),
    /// Run scenarios and render a report.
    Run(RunArgs),
    /// Run and compare against a baseline.
    Compare(CompareArgs),
    /// Manage baselines.
    Baseline(BaselineArgs),
    /// CI mode: deterministic output, artifacts, and stable exit codes.
    Ci(RunArgs),
    /// Explain the diagnostics for a single scenario.
    Explain(ExplainArgs),
    /// Analyse an existing report JSON without re-simulating.
    Inspect(InspectArgs),
    /// Import a real transaction's logs (from a `getTransaction` JSON) as a scenario log.
    Import(ImportArgs),
    /// Turnkey real-CU path: validate a bench plan and (optionally) build the program.
    Bench(BenchArgs),
}

/// Inputs shared by `run`, `ci` and `compare`.
#[derive(Debug, Args, Clone)]
pub struct CommonRun {
    /// Path to the configuration file.
    #[arg(long, default_value = "cu-profiler.toml")]
    pub config: PathBuf,

    /// Directory holding `<scenario>.log` recorded logs (v1 backend).
    #[arg(long, default_value = ".cu/logs")]
    pub logs_dir: PathBuf,

    /// Only run scenarios with these names (repeatable).
    #[arg(long = "scenario")]
    pub scenarios: Vec<String>,

    /// Only run scenarios carrying these tags (repeatable).
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}

/// `cu-profiler run` / `cu-profiler ci`.
#[derive(Debug, Args, Clone)]
pub struct RunArgs {
    #[command(flatten)]
    pub common: CommonRun,

    /// Output format.
    #[arg(long, value_parser = ["table", "json", "markdown", "junit", "html"])]
    pub format: Option<String>,

    /// Write the rendered report to this path instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Baseline file to compare against.
    #[arg(long)]
    pub baseline: Option<PathBuf>,

    /// Treat low-confidence measurements as failures.
    #[arg(long)]
    pub strict: bool,

    /// Fail when an absolute budget is exceeded.
    #[arg(long)]
    pub fail_on_budget: bool,

    /// Fail on a compute regression versus baseline.
    #[arg(long)]
    pub fail_on_regression: bool,

    /// Fail when a measurement's confidence is low.
    #[arg(long)]
    pub fail_on_low_confidence: bool,
}

/// `cu-profiler compare`.
#[derive(Debug, Args, Clone)]
pub struct CompareArgs {
    #[command(flatten)]
    pub common: CommonRun,

    /// Baseline file.
    #[arg(long, default_value = ".cu/baseline.json")]
    pub baseline: PathBuf,

    /// Output format.
    #[arg(long, value_parser = ["table", "json", "markdown", "junit", "html"])]
    pub format: Option<String>,
}

/// `cu-profiler baseline`.
#[derive(Debug, Args)]
pub struct BaselineArgs {
    #[command(subcommand)]
    pub command: BaselineCommand,
}

/// Baseline subcommands.
#[derive(Debug, Subcommand)]
pub enum BaselineCommand {
    /// Run scenarios and write their results as the new baseline.
    Save(BaselineSaveArgs),
    /// Mark a scenario's baseline record as approved.
    Approve(BaselineApproveArgs),
}

/// `cu-profiler baseline save`.
#[derive(Debug, Args)]
pub struct BaselineSaveArgs {
    #[command(flatten)]
    pub common: CommonRun,

    /// Baseline file to write.
    #[arg(long, default_value = ".cu/baseline.json")]
    pub baseline: PathBuf,
}

/// `cu-profiler baseline approve`.
#[derive(Debug, Args)]
pub struct BaselineApproveArgs {
    /// Scenario to approve.
    pub scenario: String,

    /// Baseline file.
    #[arg(long, default_value = ".cu/baseline.json")]
    pub baseline: PathBuf,
}

/// `cu-profiler explain`.
#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// Scenario name to explain.
    pub scenario: String,

    #[command(flatten)]
    pub common: CommonRun,
}

/// `cu-profiler inspect`.
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Path to a previously written report JSON.
    pub report: PathBuf,

    /// Output format.
    #[arg(long, default_value = "table", value_parser = ["table", "json", "markdown", "junit", "html"])]
    pub format: String,
}

/// `cu-profiler import`. Exactly one source: a JSON `<file>` or `--signature`.
#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("source").required(true).args(["file", "signature"])
))]
pub struct ImportArgs {
    /// A Solana `getTransaction --output json` response (or any JSON that
    /// contains a `logMessages` array).
    pub file: Option<PathBuf>,

    /// Fetch the transaction's logs live from an RPC by its signature
    /// (requires the `remote` feature, on by default).
    #[arg(long)]
    pub signature: Option<String>,

    /// RPC endpoint used with `--signature`.
    #[arg(long, default_value = "https://api.mainnet-beta.solana.com")]
    pub rpc: String,

    /// Commitment used with `--signature`.
    #[arg(long, default_value = "confirmed", value_parser = ["confirmed", "finalized"])]
    pub commitment: String,

    /// Scenario name. Defaults to the file stem, or a short form of the signature.
    #[arg(long)]
    pub name: Option<String>,

    /// Directory to write `<name>.log` into.
    #[arg(long, default_value = ".cu/logs")]
    pub logs_dir: PathBuf,
}

/// `cu-profiler bench` — turnkey real-CU path (scaffolding).
///
/// Validates a declarative bench plan and resolves/builds the program `.so`. Live
/// Mollusk execution is delivered by the Linux-only `cu-profiler-mollusk` crate.
#[derive(Debug, Args)]
pub struct BenchArgs {
    /// Bench fixture file (`[[instruction]]` declarations with accounts/data).
    #[arg(long, default_value = "bench.toml")]
    pub fixtures: PathBuf,

    /// Path to the already-compiled program `.so` (skips building).
    #[arg(long)]
    pub program: Option<PathBuf>,

    /// Program name (the `.so` stem) to locate under `$SBF_OUT_DIR`/`target/deploy`.
    #[arg(long)]
    pub program_name: Option<String>,

    /// Build the program with `cargo build-sbf` before benchmarking.
    #[arg(long)]
    pub build: bool,

    /// Directory to run `cargo build-sbf` in.
    #[arg(long, default_value = ".")]
    pub manifest_path: PathBuf,
}

/// `cu-profiler init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Directory to scaffold into.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Also write a GitHub Actions workflow.
    #[arg(long)]
    pub workflow: bool,

    /// Overwrite existing files.
    #[arg(long)]
    pub force: bool,
}

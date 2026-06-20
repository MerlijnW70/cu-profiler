//! `cu-profiler` CLI entry point — a thin wrapper over `cu-profiler-core` and
//! `cu-profiler-report`. All real logic lives in the library crates; this binary
//! only parses arguments, configures tracing, dispatches, and translates results
//! into the documented exit codes.
#![forbid(unsafe_code)]

mod args;
mod commands;
mod exit;

use clap::Parser;
use tracing::Level;

use args::{BaselineCommand, Cli, Command};
use cu_profiler_core::Result;
use exit::{ExitCode, code_for_error};

fn main() {
    let cli = Cli::parse();
    init_tracing(&cli);

    let code = match dispatch(&cli) {
        Ok(exit_code) => exit_code.code(),
        Err(err) => {
            eprintln!("error: {err}");
            code_for_error(&err).code()
        }
    };
    std::process::exit(code);
}

fn dispatch(cli: &Cli) -> Result<ExitCode> {
    let quiet = cli.quiet;
    match &cli.command {
        Command::Init(args) => commands::init(args, quiet),
        Command::Run(args) => commands::run(args, quiet),
        Command::Compare(args) => commands::compare(args, quiet),
        Command::Ci(args) => commands::ci(args, quiet),
        Command::Explain(args) => commands::explain(args, quiet),
        Command::Inspect(args) => commands::inspect(args, quiet),
        Command::Import(args) => commands::import(args, quiet),
        Command::Baseline(args) => match &args.command {
            BaselineCommand::Save(a) => commands::baseline_save(a, quiet),
            BaselineCommand::Approve(a) => commands::baseline_approve(a, quiet),
        },
    }
}

/// Configure tracing for the tool itself. Events go to stderr so stdout stays
/// reserved for report output.
fn init_tracing(cli: &Cli) {
    let level = if cli.trace {
        Level::TRACE
    } else if cli.quiet {
        Level::ERROR
    } else {
        match cli.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
            _ => Level::TRACE,
        }
    };
    // Ignore the error if a subscriber is already set (e.g. in tests).
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .try_init();
}

//! `cu-profiler ci` — deterministic CI mode that writes artifacts and returns a
//! stable exit code.

use cu_profiler_core::Result;
use cu_profiler_report::Format;

use crate::args::RunArgs;
use crate::commands::{load_config, profile};
use crate::exit::{DecisionFlags, ExitCode, code_for_report};

/// Execute the `ci` command.
pub fn run(args: &RunArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, _scenarios, _baseline) = profile(&loaded, &args.common, args.baseline.as_deref())?;

    // Write every artifact the config asks for; CI consumers pick what they want.
    let out = &loaded.config.output;
    if let Some(path) = &out.json_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Json)?)?;
    }
    if let Some(path) = &out.markdown_path {
        write_artifact(
            path,
            &cu_profiler_report::render(&report, Format::Markdown)?,
        )?;
    }
    if let Some(path) = &out.junit_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Junit)?)?;
    }
    if let Some(path) = &out.html_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Html)?)?;
    }

    // Deterministic table summary to stdout.
    if !quiet {
        print!("{}", cu_profiler_report::render(&report, Format::Table)?);
    }

    // In CI, enforce by default; explicit flags can still tighten.
    let flags = DecisionFlags {
        fail_on_budget: args.fail_on_budget || loaded.config.defaults.fail_on_budget,
        fail_on_regression: args.fail_on_regression || loaded.config.defaults.fail_on_regression,
        fail_on_stale_baseline: loaded.config.defaults.fail_on_stale_baseline,
        fail_on_low_confidence: args.fail_on_low_confidence || args.strict,
    };
    Ok(code_for_report(&report, flags))
}

fn write_artifact(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, contents)?;
    Ok(())
}

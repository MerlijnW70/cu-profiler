//! `cu-profiler run` — run scenarios and render a report.

use cu_profiler_core::Result;

use crate::args::RunArgs;
use crate::commands::{emit, load_config, profile, resolve_format, warn_if_demo};
use crate::exit::{DecisionFlags, ExitCode, code_for_report};

/// Execute the `run` command.
pub fn run(args: &RunArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, scenarios, _baseline) = profile(&loaded, &args.common, args.baseline.as_deref())?;

    warn_if_demo(&scenarios, &args.common.logs_dir, quiet);

    let format = resolve_format(&loaded.config, args.format.as_deref())?;
    let rendered = cu_profiler_report::render(&report, format)?;
    emit(&rendered, args.output.as_deref(), quiet)?;

    let flags = DecisionFlags {
        fail_on_budget: args.fail_on_budget || loaded.config.defaults.fail_on_budget,
        fail_on_regression: args.fail_on_regression || loaded.config.defaults.fail_on_regression,
        fail_on_stale_baseline: loaded.config.defaults.fail_on_stale_baseline,
        fail_on_low_confidence: args.fail_on_low_confidence || args.strict,
    };
    Ok(code_for_report(&report, flags))
}

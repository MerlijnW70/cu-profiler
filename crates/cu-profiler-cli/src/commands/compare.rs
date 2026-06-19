//! `cu-profiler compare` — run and compare against a baseline.

use cu_profiler_core::Result;

use crate::args::CompareArgs;
use crate::commands::{emit, load_config, profile, resolve_format};
use crate::exit::{DecisionFlags, ExitCode, code_for_report};

/// Execute the `compare` command.
pub fn run(args: &CompareArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, _scenarios, _baseline) = profile(&loaded, &args.common, Some(&args.baseline))?;

    let format = resolve_format(&loaded.config, args.format.as_deref())?;
    let rendered = cu_profiler_report::render(&report, format)?;
    emit(&rendered, None, quiet)?;

    // Comparison exists to catch regressions, so enforce them by default.
    let flags = DecisionFlags {
        fail_on_budget: loaded.config.defaults.fail_on_budget,
        fail_on_regression: true,
        fail_on_stale_baseline: loaded.config.defaults.fail_on_stale_baseline,
        fail_on_low_confidence: false,
    };
    Ok(code_for_report(&report, flags))
}

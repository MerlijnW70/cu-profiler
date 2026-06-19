//! `cu-profiler inspect <report.json>` — re-render and analyse an existing
//! report without re-simulating.

use cu_profiler_core::Result;
use cu_profiler_core::error::Error;

use crate::args::InspectArgs;
use crate::exit::ExitCode;

/// Execute the `inspect` command.
pub fn run(args: &InspectArgs, _quiet: bool) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.report).map_err(|e| {
        Error::Config(format!(
            "cannot read report `{}`: {e}",
            args.report.display()
        ))
    })?;
    let report = cu_profiler_report::json::parse(&text)?;
    let format = args.format.parse()?;
    print!("{}", cu_profiler_report::render(&report, format)?);
    // Inspection is read-only; it never gates CI.
    Ok(ExitCode::Success)
}

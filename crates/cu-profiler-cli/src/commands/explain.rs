//! `cu-profiler explain <scenario>` — a focused, human diagnosis of one scenario.

use std::fmt::Write as _;

use cu_profiler_core::Result;
use cu_profiler_core::error::Error;
use cu_profiler_core::model::ScenarioReport;
use cu_profiler_report::model::{scenario_budget, thousands};

use crate::args::ExplainArgs;
use crate::commands::{load_config, profile};
use crate::exit::ExitCode;

/// Execute the `explain` command.
pub fn run(args: &ExplainArgs, _quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;

    // Narrow the run to just the requested scenario.
    let mut common = args.common.clone();
    common.scenarios = vec![args.scenario.clone()];

    let (report, _scenarios, _) = profile(&loaded, &common, None)?;
    let sr = report
        .scenarios
        .iter()
        .find(|s| s.name == args.scenario)
        .ok_or_else(|| Error::Config(format!("scenario `{}` not found", args.scenario)))?;

    print!("{}", explain_text(sr));
    Ok(ExitCode::Success)
}

fn explain_text(sr: &ScenarioReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Scenario: {}  [{}]", sr.name, sr.status.label());
    let _ = writeln!(
        out,
        "Compute:  {} CU (CPIs: {}, depth: {})",
        thousands(sr.measurement.total_cu),
        sr.measurement.cpi_count,
        sr.measurement.cpi_depth,
    );
    if let Some(budget) = scenario_budget(sr) {
        let _ = writeln!(out, "Budget:   {} CU", thousands(budget));
    }
    if let Some(limit) = sr.measurement.requested_limit {
        let _ = writeln!(out, "Requested limit: {} CU", thousands(limit));
    }

    let _ = writeln!(out, "\nConfidence: {}", sr.confidence.level.label());
    for reason in &sr.confidence.reasons {
        let _ = writeln!(out, "  - {reason}");
    }

    if let Some(cmp) = &sr.baseline_comparison {
        let _ = writeln!(out, "\nBaseline: {}", cmp.summary());
        for r in &cmp.stale_reasons {
            let _ = writeln!(out, "  stale: {r}");
        }
    }

    if sr.scopes.is_empty() {
        let _ = writeln!(out, "\nScopes: none (no profiler markers detected)");
    } else {
        let _ = writeln!(out, "\nScopes:");
        for s in &sr.scopes {
            let parent = s.parent.as_deref().unwrap_or("-");
            let _ = writeln!(out, "  {} (parent: {parent})", s.name);
        }
    }

    if sr.diagnostics.is_empty() {
        let _ = writeln!(out, "\nDiagnostics: none");
    } else {
        let _ = writeln!(out, "\nDiagnostics:");
        for d in &sr.diagnostics {
            let _ = writeln!(out, "  [{:?}] {}", d.severity, d.title);
            let _ = writeln!(out, "    evidence: {}", d.evidence);
            let _ = writeln!(out, "    recommend: {}", d.recommendation);
        }
    }

    for w in &sr.parser_warnings {
        let _ = writeln!(out, "warning: {w}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::budget::BudgetPolicy;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    #[test]
    fn explain_text_mentions_status_and_confidence() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "swap",
            "Program P invoke [1]\nProgram P consumed 96000 of 100000 compute units\nProgram P success",
            true,
        );
        let mut scenario = Scenario::new("swap");
        scenario.budget = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        let report =
            Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"));
        let text = explain_text(&report.scenarios[0]);
        assert!(text.contains("Scenario: swap"));
        assert!(text.contains("Confidence:"));
        assert!(text.contains("near_budget_limit") || text.contains("near its compute budget"));
    }
}

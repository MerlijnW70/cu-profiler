//! Markdown output, intended for GitHub PR comments.

use crate::model::{Report, scenario_budget, scenario_delta_pct, thousands};

/// Render `report` as a Markdown document.
#[must_use]
pub fn render(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("## cu-profiler report\n\n");

    let sum = &report.summary;
    out.push_str(&format!(
        "**{}** scenario(s): {} passed · {} warned · {} failed — **{} total CU**\n\n",
        sum.total_scenarios,
        sum.passed,
        sum.warned,
        sum.failed,
        thousands(sum.total_cu),
    ));

    out.push_str("| Scenario | Actual CU | Budget | Delta | Status |\n");
    out.push_str("| --- | ---: | ---: | ---: | :---: |\n");
    for s in &report.scenarios {
        let budget = scenario_budget(s).map_or_else(|| "—".to_string(), thousands);
        let delta = scenario_delta_pct(s).map_or_else(|| "—".to_string(), |d| format!("{d:+.1}%"));
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} {} |\n",
            md_code(&s.name),
            thousands(s.measurement.total_cu),
            budget,
            delta,
            status_emoji(s.status),
            s.status.label(),
        ));
    }

    let diagnostics: Vec<_> = report
        .scenarios
        .iter()
        .flat_map(|s| &s.diagnostics)
        .collect();
    if !diagnostics.is_empty() {
        out.push_str("\n### Diagnostics\n\n");
        for d in diagnostics {
            out.push_str(&format!(
                "- **{}** (`{}`)\n  - {}\n  - _Recommendation:_ {}\n",
                md_text(&d.title),
                md_code(&d.scenario),
                md_text(&d.evidence),
                md_text(&d.recommendation),
            ));
        }
    }

    out
}

/// Sanitise a value for a Markdown table cell or inline text: collapse newlines
/// and escape the pipe that would otherwise split the row.
fn md_text(s: &str) -> String {
    s.replace(['\n', '\r'], " ").replace('|', "\\|")
}

/// Sanitise a value placed inside an inline code span (`` `…` ``): in addition to
/// [`md_text`], neutralise backticks that would close the span early.
fn md_code(s: &str) -> String {
    md_text(s).replace('`', "'")
}

fn status_emoji(status: cu_profiler_core::model::Status) -> &'static str {
    use cu_profiler_core::model::Status;
    match status {
        Status::Pass => "🟢",
        Status::Warn => "🟡",
        Status::Fail => "🔴",
        Status::Unknown => "⚪",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    #[test]
    fn renders_markdown_table() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "swap",
            "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
            true,
        );
        let report = Profiler::new().run(
            &backend,
            &[Scenario::new("swap")],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        let md = render(&report);
        assert!(md.contains("## cu-profiler report"));
        assert!(md.contains("| `swap` |"));
        assert!(md.contains("PASS"));
    }

    #[test]
    fn sanitises_pipes_backticks_and_newlines() {
        assert_eq!(md_text("a|b"), "a\\|b");
        assert_eq!(md_text("line1\nline2"), "line1 line2");
        assert_eq!(md_code("we`ird|name"), "we'ird\\|name");
    }

    #[test]
    fn malicious_scenario_name_does_not_break_table_row() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "evil|name`",
            "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
            true,
        );
        let report = Profiler::new().run(
            &backend,
            &[Scenario::new("evil|name`")],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        let md = render(&report);
        let row = md
            .lines()
            .find(|l| l.contains("evil"))
            .expect("data row present");
        // A 5-column row has 6 structural `|`. The pipe inside the name must be
        // escaped (`\|`), so structural = total pipes − escaped pipes = 6.
        let total_pipes = row.matches('|').count();
        let escaped_pipes = row.matches("\\|").count();
        assert_eq!(
            total_pipes - escaped_pipes,
            6,
            "unescaped pipe leaked into row: {row}"
        );
        assert!(row.contains("evil\\|name'"), "name not sanitised: {row}");
    }

    #[test]
    fn renders_diagnostics_section_and_status_emoji() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "swap",
            "Program User111 invoke [1]\n\
             Program log: CU_PROFILER_BEGIN name=validate cu=200000\n\
             Program log: CU_PROFILER_END name=validate cu=188000\n\
             Program User111 consumed 96000 of 100000 compute units\n\
             Program User111 success",
            true,
        );
        let mut scenario = Scenario::new("swap");
        scenario.budget = cu_profiler_core::budget::BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        let report =
            Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"));
        let md = render(&report);
        assert!(
            md.contains("### Diagnostics"),
            "diagnostics section missing: {md}"
        );
        assert!(md.contains('🟡'), "warn status emoji missing: {md}");
    }
}

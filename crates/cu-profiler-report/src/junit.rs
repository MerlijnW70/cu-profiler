//! JUnit XML output for CI test dashboards.
//!
//! Each scenario maps to a `<testcase>`; a budget/regression failure or an
//! unexpected simulation outcome maps to a `<failure>`, so existing CI tooling
//! can surface compute regressions as test failures.

use cu_profiler_core::model::{Report, ScenarioReport, Status};

/// Render `report` as a JUnit `<testsuite>` document.
#[must_use]
pub fn render(report: &Report) -> String {
    let sum = &report.summary;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuite name=\"cu-profiler\" tests=\"{}\" failures=\"{}\">\n",
        sum.total_scenarios, sum.failed,
    ));
    for s in &report.scenarios {
        push_case(&mut out, s);
    }
    out.push_str("</testsuite>\n");
    out
}

fn push_case(out: &mut String, s: &ScenarioReport) {
    out.push_str(&format!(
        "  <testcase name=\"{}\" classname=\"cu-profiler\">\n",
        escape(&s.name),
    ));
    match s.status {
        Status::Fail | Status::Unknown => {
            let message = failure_message(s);
            out.push_str(&format!(
                "    <failure message=\"{}\">{}</failure>\n",
                escape(&message),
                escape(&detail(s)),
            ));
        }
        Status::Warn => {
            out.push_str(&format!(
                "    <system-out>WARN: {}</system-out>\n",
                escape(&detail(s)),
            ));
        }
        Status::Pass => {}
    }
    out.push_str("  </testcase>\n");
}

fn failure_message(s: &ScenarioReport) -> String {
    s.policy_results
        .iter()
        .find(|p| matches!(p.status, cu_profiler_core::budget::PolicyStatus::Fail))
        .map_or_else(|| format!("{} did not pass", s.name), |p| p.message.clone())
}

fn detail(s: &ScenarioReport) -> String {
    let mut parts = vec![format!("status={}", s.status.label())];
    parts.push(format!("total_cu={}", s.measurement.total_cu));
    for d in &s.diagnostics {
        parts.push(format!("{}: {}", d.id, d.recommendation));
    }
    parts.join("\n")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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
    fn failing_scenario_becomes_failure_case() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "swap",
            "Program P invoke [1]\nProgram P consumed 120000 of 200000 compute units\nProgram P success",
            true,
        );
        let mut scenario = Scenario::new("swap");
        scenario.budget = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            ..Default::default()
        };
        let report =
            Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"));
        let xml = render(&report);
        assert!(xml.contains("<testsuite"));
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("<failure"));
    }

    #[test]
    fn escapes_special_characters() {
        assert_eq!(escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
    }
}

//! Self-contained HTML report.
//!
//! Produces a single static HTML document (inline CSS, no assets, no scripts) so
//! it can be uploaded as a CI artifact or pasted into a PR. It renders the same
//! data the other formats do — summary, per-scenario measurement, the CPI call
//! tree, scopes, diagnostics and confidence — purely as presentation.

use std::fmt::Write as _;

use cu_profiler_core::model::{Report, ScenarioReport, Status};
use cu_profiler_core::parser::CallNode;

use crate::model::{scenario_budget, scenario_delta_pct, thousands};

/// Render `report` as a complete HTML document.
#[must_use]
pub fn render(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>cu-profiler report</title>\n");
    out.push_str(STYLE);
    out.push_str("</head>\n<body>\n");
    out.push_str("<h1>cu-profiler report</h1>\n");

    let s = &report.summary;
    let _ = writeln!(
        out,
        "<p class=\"summary\">{} scenario(s): \
         <span class=\"pass\">{} passed</span> · \
         <span class=\"warn\">{} warned</span> · \
         <span class=\"fail\">{} failed</span> — <strong>{} total CU</strong></p>",
        s.total_scenarios,
        s.passed,
        s.warned,
        s.failed,
        thousands(s.total_cu),
    );

    push_overview_table(&mut out, report);
    for scenario in &report.scenarios {
        push_scenario(&mut out, scenario);
    }

    out.push_str("</body>\n</html>\n");
    out
}

fn push_overview_table(out: &mut String, report: &Report) {
    out.push_str("<table class=\"overview\">\n<thead><tr>");
    for h in ["Scenario", "Actual CU", "Budget", "Delta", "Status"] {
        let _ = write!(out, "<th>{h}</th>");
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for sc in &report.scenarios {
        let budget = scenario_budget(sc).map_or_else(|| "—".to_string(), thousands);
        let delta = scenario_delta_pct(sc).map_or_else(|| "—".to_string(), |d| format!("{d:+.1}%"));
        let _ = writeln!(
            out,
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"{}\">{}</td></tr>",
            esc(&sc.name),
            thousands(sc.measurement.total_cu),
            budget,
            delta,
            status_class(sc.status),
            sc.status.label(),
        );
    }
    out.push_str("</tbody>\n</table>\n");
}

fn push_scenario(out: &mut String, sc: &ScenarioReport) {
    let _ = writeln!(
        out,
        "<section class=\"scenario\">\n<h2>{} <span class=\"{}\">{}</span></h2>",
        esc(&sc.name),
        status_class(sc.status),
        sc.status.label(),
    );

    let m = &sc.measurement;
    let _ = writeln!(
        out,
        "<p class=\"meta\">{} CU · {} CPIs · depth {} · confidence {}</p>",
        thousands(m.total_cu),
        m.cpi_count,
        m.cpi_depth,
        esc(sc.confidence.level.label()),
    );
    if !sc.confidence.reasons.is_empty() {
        out.push_str("<ul class=\"reasons\">\n");
        for r in &sc.confidence.reasons {
            let _ = writeln!(out, "<li>{}</li>", esc(r));
        }
        out.push_str("</ul>\n");
    }

    if let Some(tree) = &sc.call_tree {
        out.push_str("<h3>Call tree</h3>\n");
        push_tree(out, tree);
    }

    if !sc.scopes.is_empty() {
        out.push_str("<h3>Scopes</h3>\n<ul class=\"scopes\">\n");
        for scope in &sc.scopes {
            let cu = match (scope.units_estimated, scope.percentage_of_total) {
                (Some(u), Some(p)) => format!(" — {} CU ({p:.1}%)", thousands(u)),
                _ => String::new(),
            };
            let _ = writeln!(out, "<li>{}{}</li>", esc(&scope.name), esc(&cu));
        }
        out.push_str("</ul>\n");
    }

    if !sc.diagnostics.is_empty() {
        out.push_str("<h3>Diagnostics</h3>\n<ul class=\"diagnostics\">\n");
        for d in &sc.diagnostics {
            let _ = writeln!(
                out,
                "<li><strong>{}</strong> — {}<br><em>{}</em></li>",
                esc(&d.title),
                esc(&d.evidence),
                esc(&d.recommendation),
            );
        }
        out.push_str("</ul>\n");
    }

    out.push_str("</section>\n");
}

fn push_tree(out: &mut String, node: &CallNode) {
    out.push_str("<ul class=\"tree\">\n");
    push_node(out, node);
    out.push_str("</ul>\n");
}

fn push_node(out: &mut String, node: &CallNode) {
    let label = node.label.as_deref().unwrap_or(&node.program_id);
    let units = node
        .units_consumed
        .map_or_else(String::new, |u| format!(" — {} CU", thousands(u)));
    let _ = write!(out, "<li>{}{}", esc(label), esc(&units));
    if !node.children.is_empty() {
        out.push_str("\n<ul>\n");
        for child in &node.children {
            push_node(out, child);
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</li>\n");
}

fn status_class(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::Unknown => "unknown",
    }
}

/// Escape the five HTML-significant characters.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = "<style>\n\
body{font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;margin:2rem auto;max-width:60rem;color:#1b1f24;padding:0 1rem}\n\
h1{font-size:1.5rem}h2{font-size:1.15rem;margin-top:2rem}h3{font-size:1rem;color:#444}\n\
table{border-collapse:collapse;width:100%;margin:1rem 0}th,td{padding:.4rem .6rem;border-bottom:1px solid #e2e6ea;text-align:left}\n\
td.num{text-align:right;font-variant-numeric:tabular-nums}\n\
.pass{color:#1a7f37;font-weight:600}.warn{color:#9a6700;font-weight:600}.fail{color:#cf222e;font-weight:600}.unknown{color:#57606a;font-weight:600}\n\
.summary{font-size:1.05rem}.meta{color:#57606a}\n\
ul.tree,ul.tree ul{list-style:none;padding-left:1.1rem;border-left:1px solid #e2e6ea}\n\
ul.scopes,ul.diagnostics,ul.reasons{padding-left:1.1rem}\n\
.scenario{border-top:1px solid #e2e6ea;padding-top:.5rem}\n\
</style>\n";

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::budget::BudgetPolicy;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    fn report(name: &str) -> Report {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            name,
            "Program User111 invoke [1]\n\
             Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]\n\
             Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success\n\
             Program User111 consumed 96000 of 100000 compute units\n\
             Program User111 success",
            true,
        );
        let mut scenario = Scenario::new(name);
        scenario.budget = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"))
    }

    #[test]
    fn renders_well_formed_document() {
        let html = render(&report("swap"));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>cu-profiler report</title>"));
        assert!(html.contains("cu-profiler report"));
        assert!(html.contains("swap"));
        assert!(html.contains("SPL Token")); // labelled CPI in the call tree
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn escapes_html_in_scenario_names() {
        let html = render(&report("<script>evil</script>"));
        assert!(!html.contains("<script>evil"));
        assert!(html.contains("&lt;script&gt;evil&lt;/script&gt;"));
    }
}

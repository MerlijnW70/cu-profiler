//! Human-facing aligned table for local CLI use.
//!
//! Hand-rolled (no table dependency) so the crate stays light and the output is
//! deterministic for snapshot tests.

use crate::model::{Report, scenario_budget, scenario_delta_pct, thousands};

const HEADERS: [&str; 5] = ["Scenario", "Actual CU", "Budget", "Delta", "Status"];

/// Render `report` as an aligned text table with a summary footer.
#[must_use]
pub fn render(report: &Report) -> String {
    let mut rows: Vec<[String; 5]> = Vec::with_capacity(report.scenarios.len());
    for s in &report.scenarios {
        rows.push([
            s.name.clone(),
            thousands(s.measurement.total_cu),
            scenario_budget(s).map_or_else(|| "-".to_string(), thousands),
            scenario_delta_pct(s).map_or_else(|| "-".to_string(), |d| format!("{d:+.1}%")),
            s.status.label().to_string(),
        ]);
    }

    // Column widths: max of header and all cells.
    let mut widths = HEADERS.map(str::len);
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let mut out = String::new();
    push_row(&mut out, &HEADERS.map(String::from), &widths);
    for row in &rows {
        push_row(&mut out, row, &widths);
    }

    out.push('\n');
    let sum = &report.summary;
    out.push_str(&format!(
        "{} scenario(s): {} passed, {} warned, {} failed — {} total CU\n",
        sum.total_scenarios,
        sum.passed,
        sum.warned,
        sum.failed,
        thousands(sum.total_cu),
    ));
    out
}

fn push_row(out: &mut String, row: &[String; 5], widths: &[usize; 5]) {
    // Column 0 (name) and column 4 (status) left-aligned; numerics right-aligned.
    out.push_str(&pad_left(&row[0], widths[0]));
    for i in 1..4 {
        out.push_str("  ");
        out.push_str(&pad_right(&row[i], widths[i]));
    }
    out.push_str("  ");
    out.push_str(&pad_left(&row[4], widths[4]));
    // Trim trailing spaces from the final left-aligned column.
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('\n');
}

fn pad_left(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

fn pad_right(s: &str, width: usize) -> String {
    format!("{s:>width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::budget::BudgetPolicy;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    fn sample_report() -> Report {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "swap_exact_in",
            "Program User111 invoke [1]\n\
             Program User111 consumed 96812 of 200000 compute units\n\
             Program User111 success",
            true,
        );
        let mut scenario = Scenario::new("swap_exact_in");
        scenario.budget = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"))
    }

    #[test]
    fn renders_headers_and_values() {
        let table = render(&sample_report());
        assert!(table.contains("Scenario"));
        assert!(table.contains("swap_exact_in"));
        assert!(table.contains("96,812"));
        assert!(table.contains("100,000"));
        assert!(table.contains("WARN"));
        assert!(table.contains("1 scenario(s): 0 passed, 1 warned, 0 failed"));
    }
}

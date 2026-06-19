//! Re-exports of the core report data types.
//!
//! Rendering operates on the *same* data model the core produces — this module
//! simply re-exports it so report consumers have a single import surface and the
//! raw-data/rendering boundary stays explicit.

pub use cu_profiler_core::baseline::BaselineComparison;
pub use cu_profiler_core::budget::{PolicyResult, PolicyStatus, Severity};
pub use cu_profiler_core::confidence::{Confidence, ConfidenceLevel};
pub use cu_profiler_core::diagnostics::Diagnostic;
pub use cu_profiler_core::model::{
    InstructionMeasurement, Measurement, Report, ScenarioReport, Status, Summary,
};

/// Format a `u64` with thousands separators, e.g. `96812` → `96,812`.
#[must_use]
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut grouped: Vec<char> = Vec::with_capacity(s.len() + s.len() / 3);
    for (count, ch) in s.chars().rev().enumerate() {
        if count != 0 && count % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.iter().rev().collect()
}

/// The absolute budget for a scenario, read back from its evaluated policy.
#[must_use]
pub fn scenario_budget(scenario: &ScenarioReport) -> Option<u64> {
    scenario
        .policy_results
        .iter()
        .find(|p| p.policy_id == "absolute_max_cu")
        .and_then(|p| p.expected)
        .map(|f| f as u64)
}

/// The baseline delta percentage for a scenario, if compared.
#[must_use]
pub fn scenario_delta_pct(scenario: &ScenarioReport) -> Option<f64> {
    scenario.baseline_comparison.as_ref().map(|c| c.delta_pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_formatting() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(96_812), "96,812");
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(thousands(999), "999");
    }
}

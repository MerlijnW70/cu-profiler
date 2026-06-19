//! Diagnostic engine: turns analysed data into actionable, Solana-specific
//! findings.

pub mod rules;

pub use rules::Context;

use serde::{Deserialize, Serialize};

use crate::budget::Severity;

/// A single finding about a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable identifier, e.g. `"cpi_explosion"`.
    pub id: String,
    /// Short human title.
    pub title: String,
    /// Severity.
    pub severity: Severity,
    /// Scenario the finding applies to.
    pub scenario: String,
    /// The evidence that triggered the finding.
    pub evidence: String,
    /// What to do about it.
    pub recommendation: String,
}

/// Run every rule against the context and collect the findings.
#[must_use]
pub fn evaluate(ctx: &Context) -> Vec<Diagnostic> {
    rules::RULES.iter().filter_map(|rule| rule(ctx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{self, BudgetPolicy};
    use crate::confidence::Confidence;
    use crate::model::Measurement;
    use crate::scenario::ExpectedResult;

    #[test]
    fn flags_absolute_budget_and_cpi_explosion() {
        let measurement = Measurement {
            total_cu: 120_000,
            cpi_count: 10,
            ..Measurement::empty()
        };
        let policy = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            ..Default::default()
        };
        let policy_results = budget::evaluate(&measurement, &policy, None);
        let confidence = Confidence::high();
        let ctx = Context {
            scenario: "swap",
            measurement: &measurement,
            policy_results: &policy_results,
            baseline: None,
            confidence: &confidence,
            expected: ExpectedResult::Success,
            scope_count: 0,
            log_line_count: 0,
            late_validation: false,
        };
        let diags = evaluate(&ctx);
        let ids: Vec<&str> = diags.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"absolute_budget_exceeded"));
        assert!(ids.contains(&"cpi_explosion"));
    }

    #[test]
    fn clean_run_has_no_diagnostics() {
        let measurement = Measurement {
            total_cu: 10_000,
            ..Measurement::empty()
        };
        let confidence = Confidence::high();
        let ctx = Context {
            scenario: "ok",
            measurement: &measurement,
            policy_results: &[],
            baseline: None,
            confidence: &confidence,
            expected: ExpectedResult::Success,
            scope_count: 0,
            log_line_count: 0,
            late_validation: false,
        };
        assert!(evaluate(&ctx).is_empty());
    }

    #[test]
    fn flags_cpi_share_log_bloat_and_late_validation() {
        let measurement = Measurement {
            total_cu: 50_000,
            cpi_count: 2,
            unattributed_pct: 10.0, // ⇒ 90% CPI share
            ..Measurement::empty()
        };
        let confidence = Confidence::high();
        let ctx = Context {
            scenario: "swap",
            measurement: &measurement,
            policy_results: &[],
            baseline: None,
            confidence: &confidence,
            expected: ExpectedResult::Success,
            scope_count: 4,
            log_line_count: 40,
            late_validation: true,
        };
        let ids: Vec<String> = evaluate(&ctx).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"high_cpi_share".to_string()));
        assert!(ids.contains(&"event_log_bloat".to_string()));
        assert!(ids.contains(&"late_validation".to_string()));
    }
}

//! Declarative budget policy. Every field is optional; an absent field means
//! "no opinion", so policies compose by merging defaults with per-scenario
//! overrides.

use serde::{Deserialize, Serialize};

/// A budget policy attached to a scenario (or used as a project default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BudgetPolicy {
    /// Absolute maximum compute units. Exceeding this is a hard `Fail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_max_cu: Option<u64>,
    /// Warn once consumption reaches this percentage of `absolute_max_cu`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_at_budget_pct: Option<f64>,
    /// Maximum tolerated regression versus baseline, as a percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_regression_pct: Option<f64>,
    /// Maximum tolerated regression versus baseline, in absolute units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_regression_units: Option<u64>,
    /// Minimum required margin below `absolute_max_cu`, as a percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_margin_pct: Option<f64>,
    /// Maximum number of CPI invocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpi_count: Option<u32>,
    /// Maximum CPI invoke depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cpi_depth: Option<u32>,
    /// Maximum percentage of CU left unattributed to a scope before warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_unattributed_pct: Option<f64>,
    /// Warn when instrumentation overhead exceeds this percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumentation_overhead_warn_pct: Option<f64>,
}

impl BudgetPolicy {
    /// Overlay `override_with` onto `self`: any field set in the override wins.
    #[must_use]
    pub fn merged_with(&self, override_with: &BudgetPolicy) -> BudgetPolicy {
        BudgetPolicy {
            absolute_max_cu: override_with.absolute_max_cu.or(self.absolute_max_cu),
            warn_at_budget_pct: override_with.warn_at_budget_pct.or(self.warn_at_budget_pct),
            max_regression_pct: override_with.max_regression_pct.or(self.max_regression_pct),
            max_regression_units: override_with
                .max_regression_units
                .or(self.max_regression_units),
            min_margin_pct: override_with.min_margin_pct.or(self.min_margin_pct),
            max_cpi_count: override_with.max_cpi_count.or(self.max_cpi_count),
            max_cpi_depth: override_with.max_cpi_depth.or(self.max_cpi_depth),
            max_unattributed_pct: override_with
                .max_unattributed_pct
                .or(self.max_unattributed_pct),
            instrumentation_overhead_warn_pct: override_with
                .instrumentation_overhead_warn_pct
                .or(self.instrumentation_overhead_warn_pct),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_but_keeps_base_fields() {
        let base = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            max_regression_pct: Some(5.0),
            ..Default::default()
        };
        let over = BudgetPolicy {
            max_regression_pct: Some(3.0),
            ..Default::default()
        };
        let merged = base.merged_with(&over);
        assert_eq!(merged.absolute_max_cu, Some(100_000));
        assert_eq!(merged.max_regression_pct, Some(3.0));
    }
}

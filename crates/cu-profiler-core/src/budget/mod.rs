//! Budget policy engine.
//!
//! Given a [`crate::model::Measurement`], a [`BudgetPolicy`], and an optional
//! baseline figure, [`evaluate`] produces a list of structured [`PolicyResult`]s
//! — one per clause that the policy actually constrains.

mod policy;
mod result;

pub use policy::BudgetPolicy;
pub use result::{PolicyResult, PolicyStatus, Severity};

use crate::model::Measurement;

/// Evaluate every active clause of `policy` against `measurement`.
///
/// `baseline_units`, when present, enables the regression clauses.
#[must_use]
pub fn evaluate(
    measurement: &Measurement,
    policy: &BudgetPolicy,
    baseline_units: Option<u64>,
) -> Vec<PolicyResult> {
    let mut out = Vec::new();
    let actual = measurement.total_cu;

    if let Some(max) = policy.absolute_max_cu {
        out.push(eval_absolute(actual, max));
        if let Some(warn_pct) = policy.warn_at_budget_pct {
            if let Some(r) = eval_warn_threshold(actual, max, warn_pct) {
                out.push(r);
            }
        }
        if let Some(min_margin) = policy.min_margin_pct {
            out.push(eval_min_margin(actual, max, min_margin));
        }
    }

    if let Some(base) = baseline_units {
        if let Some(pct) = policy.max_regression_pct {
            out.push(eval_regression_pct(actual, base, pct));
        }
        if let Some(units) = policy.max_regression_units {
            out.push(eval_regression_units(actual, base, units));
        }
    }

    if let Some(max_count) = policy.max_cpi_count {
        out.push(eval_cpi_count(measurement.cpi_count, max_count));
    }
    if let Some(max_depth) = policy.max_cpi_depth {
        out.push(eval_cpi_depth(measurement.cpi_depth, max_depth));
    }
    if let Some(max_unattr) = policy.max_unattributed_pct {
        out.push(eval_unattributed(measurement.unattributed_pct, max_unattr));
    }
    if let (Some(warn), Some(overhead)) = (
        policy.instrumentation_overhead_warn_pct,
        measurement.instrumentation_overhead_pct,
    ) {
        out.push(eval_overhead(overhead, warn));
    }

    out
}

/// Roll a set of results up into a single worst-case status.
#[must_use]
pub fn overall_status(results: &[PolicyResult]) -> PolicyStatus {
    results
        .iter()
        .fold(PolicyStatus::Pass, |acc, r| acc.max(r.status))
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64) * 100.0
    }
}

fn eval_absolute(actual: u64, max: u64) -> PolicyResult {
    let (status, severity) = if actual > max {
        (PolicyStatus::Fail, Severity::Error)
    } else {
        (PolicyStatus::Pass, Severity::Info)
    };
    PolicyResult {
        policy_id: "absolute_max_cu".into(),
        status,
        severity,
        actual: Some(actual as f64),
        expected: Some(max as f64),
        message: format!("{actual} CU against an absolute maximum of {max} CU"),
        remediation: (status == PolicyStatus::Fail).then(|| {
            "Reduce hot-path compute: move cheap validation before CPIs and cut event emission."
                .to_string()
        }),
    }
}

fn eval_warn_threshold(actual: u64, max: u64, warn_pct: f64) -> Option<PolicyResult> {
    if actual > max {
        return None; // already covered by the absolute clause as a Fail
    }
    let used = pct(actual, max);
    if used < warn_pct {
        return None;
    }
    Some(PolicyResult {
        policy_id: "warn_at_budget_pct".into(),
        status: PolicyStatus::Warn,
        severity: Severity::Warning,
        actual: Some(used),
        expected: Some(warn_pct),
        message: format!("{used:.1}% of budget used (warns at {warn_pct:.0}%)"),
        remediation: Some("Approaching the budget ceiling; profile the hottest scope.".into()),
    })
}

fn eval_min_margin(actual: u64, max: u64, min_margin: f64) -> PolicyResult {
    let margin = 100.0 - pct(actual, max);
    let ok = margin >= min_margin;
    PolicyResult {
        policy_id: "min_margin_pct".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Warn
        },
        severity: if ok {
            Severity::Info
        } else {
            Severity::Warning
        },
        actual: Some(margin),
        expected: Some(min_margin),
        message: format!("{margin:.1}% margin below budget (minimum {min_margin:.0}%)"),
        remediation: (!ok).then(|| "Thin margin; a small regression could breach budget.".into()),
    }
}

fn eval_regression_pct(actual: u64, base: u64, max_pct: f64) -> PolicyResult {
    let delta = actual as i64 - base as i64;
    let delta_pct = if base == 0 {
        0.0
    } else {
        (delta as f64 / base as f64) * 100.0
    };
    let ok = delta_pct <= max_pct;
    PolicyResult {
        policy_id: "max_regression_pct".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Fail
        },
        severity: if ok { Severity::Info } else { Severity::Error },
        actual: Some(delta_pct),
        expected: Some(max_pct),
        message: format!(
            "regression {delta_pct:+.2}% vs baseline {base} CU (allowed +{max_pct:.2}%)"
        ),
        remediation: (!ok)
            .then(|| "Inspect the CPI count and recently changed validation path.".into()),
    }
}

fn eval_regression_units(actual: u64, base: u64, max_units: u64) -> PolicyResult {
    let delta = actual as i64 - base as i64;
    let ok = delta <= max_units as i64;
    PolicyResult {
        policy_id: "max_regression_units".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Fail
        },
        severity: if ok { Severity::Info } else { Severity::Error },
        actual: Some(delta as f64),
        expected: Some(max_units as f64),
        message: format!("regression {delta:+} CU vs baseline (allowed +{max_units} CU)"),
        remediation: (!ok)
            .then(|| "Compute grew beyond the unit budget; bisect recent changes.".into()),
    }
}

fn eval_cpi_count(actual: u32, max: u32) -> PolicyResult {
    let ok = actual <= max;
    PolicyResult {
        policy_id: "max_cpi_count".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Fail
        },
        severity: if ok { Severity::Info } else { Severity::Error },
        actual: Some(actual as f64),
        expected: Some(max as f64),
        message: format!("{actual} CPIs (maximum {max})"),
        remediation: (!ok).then(|| "Check for duplicate ATA creation or redundant CPIs.".into()),
    }
}

fn eval_cpi_depth(actual: u32, max: u32) -> PolicyResult {
    let ok = actual <= max;
    PolicyResult {
        policy_id: "max_cpi_depth".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Fail
        },
        severity: if ok { Severity::Info } else { Severity::Error },
        actual: Some(actual as f64),
        expected: Some(max as f64),
        message: format!("CPI depth {actual} (maximum {max})"),
        remediation: (!ok).then(|| "Deep CPI nesting risks the runtime invoke-depth limit.".into()),
    }
}

fn eval_unattributed(actual_pct: f64, max_pct: f64) -> PolicyResult {
    let ok = actual_pct <= max_pct;
    PolicyResult {
        policy_id: "max_unattributed_pct".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Warn
        },
        severity: if ok {
            Severity::Info
        } else {
            Severity::Warning
        },
        actual: Some(actual_pct),
        expected: Some(max_pct),
        message: format!("{actual_pct:.1}% unattributed CU (maximum {max_pct:.0}%)"),
        remediation: (!ok).then(|| {
            "Add scope markers around account validation and math to attribute CU.".into()
        }),
    }
}

fn eval_overhead(actual_pct: f64, warn_pct: f64) -> PolicyResult {
    let ok = actual_pct <= warn_pct;
    PolicyResult {
        policy_id: "instrumentation_overhead_warn_pct".into(),
        status: if ok {
            PolicyStatus::Pass
        } else {
            PolicyStatus::Warn
        },
        severity: if ok {
            Severity::Info
        } else {
            Severity::Warning
        },
        actual: Some(actual_pct),
        expected: Some(warn_pct),
        message: format!("instrumentation overhead {actual_pct:.1}% (warns at {warn_pct:.0}%)"),
        remediation: (!ok).then(|| "Reduce the number of profiler markers in the hot path.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Measurement;

    fn measurement(total: u64) -> Measurement {
        Measurement {
            total_cu: total,
            ..Measurement::empty()
        }
    }

    #[test]
    fn absolute_budget_fails_when_exceeded() {
        let m = measurement(120_000);
        let policy = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            ..Default::default()
        };
        let results = evaluate(&m, &policy, None);
        assert_eq!(overall_status(&results), PolicyStatus::Fail);
        assert!(results[0].remediation.is_some());
    }

    #[test]
    fn warn_threshold_triggers_below_ceiling() {
        let m = measurement(96_000);
        let policy = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        let results = evaluate(&m, &policy, None);
        assert_eq!(overall_status(&results), PolicyStatus::Warn);
    }

    #[test]
    fn regression_pct_fails_over_allowance() {
        let m = measurement(96_812);
        let policy = BudgetPolicy {
            max_regression_pct: Some(5.0),
            ..Default::default()
        };
        let results = evaluate(&m, &policy, Some(91_204));
        assert_eq!(overall_status(&results), PolicyStatus::Fail);
    }

    #[test]
    fn within_all_limits_passes() {
        let m = measurement(50_000);
        let policy = BudgetPolicy {
            absolute_max_cu: Some(100_000),
            warn_at_budget_pct: Some(90.0),
            max_regression_pct: Some(5.0),
            ..Default::default()
        };
        let results = evaluate(&m, &policy, Some(49_500));
        assert_eq!(overall_status(&results), PolicyStatus::Pass);
    }
}

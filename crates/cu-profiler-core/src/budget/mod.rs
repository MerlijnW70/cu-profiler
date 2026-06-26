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

    // ---- Per-clause boundary & remediation coverage ----------------------------
    // The integration-style tests above exercise `evaluate` end-to-end but never
    // pin the exact-at-limit boundary or the "remediation only on failure" rule for
    // each clause. These tests assert those laws in Rust so a boundary flip
    // (`<=`→`>`, `>`→`>=`) or a misplaced remediation cannot pass the suite. Each
    // calls the clause function directly.

    #[test]
    fn absolute_exactly_at_budget_passes() {
        let r = eval_absolute(100, 100);
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn absolute_over_budget_fails_with_remediation() {
        let r = eval_absolute(101, 100);
        assert_eq!(r.status, PolicyStatus::Fail);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn warn_threshold_fires_at_full_budget() {
        // actual == max (100% used) must still warn, not be swallowed by the
        // `actual > max` early return.
        let r = eval_warn_threshold(100, 100, 90.0).expect("100% used warns");
        assert_eq!(r.status, PolicyStatus::Warn);
    }

    #[test]
    fn warn_threshold_fires_exactly_at_threshold() {
        // used == warn_pct must warn (the comparison is `<`, not `<=`). 1/2 = 50.0
        // is exactly representable, so this pins the boundary without float drift.
        let r = eval_warn_threshold(1, 2, 50.0).expect("exactly at threshold warns");
        assert_eq!(r.status, PolicyStatus::Warn);
    }

    #[test]
    fn warn_threshold_silent_below_threshold() {
        assert!(eval_warn_threshold(50, 100, 90.0).is_none());
    }

    #[test]
    fn warn_threshold_silent_over_budget() {
        assert!(eval_warn_threshold(120, 100, 90.0).is_none());
    }

    #[test]
    fn min_margin_reports_exact_margin() {
        // 1/2 used → 50% margin. Pins the subtraction so `+`/`/` mutants die.
        let r = eval_min_margin(1, 2, 40.0);
        assert_eq!(r.actual, Some(50.0));
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn min_margin_exactly_at_minimum_passes() {
        // 50% margin == 50% minimum → Pass (the comparison is `>=`).
        let r = eval_min_margin(1, 2, 50.0);
        assert_eq!(r.status, PolicyStatus::Pass);
    }

    #[test]
    fn min_margin_thin_warns_with_remediation() {
        // 3/4 used → 25% margin < 40% minimum.
        let r = eval_min_margin(3, 4, 40.0);
        assert_eq!(r.status, PolicyStatus::Warn);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn regression_pct_pass_has_no_remediation() {
        let r = eval_regression_pct(100, 100, 5.0); // 0% regression
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn regression_pct_fail_has_remediation() {
        let r = eval_regression_pct(110, 100, 5.0); // +10% > 5%
        assert_eq!(r.status, PolicyStatus::Fail);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn regression_units_reports_signed_delta() {
        // +20 CU delta; pins the subtraction so `+`/`/` mutants die.
        let r = eval_regression_units(120, 100, 30);
        assert_eq!(r.actual, Some(20.0));
    }

    #[test]
    fn regression_units_exactly_at_allowance_passes() {
        let r = eval_regression_units(130, 100, 30); // delta 30 == allowance
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn regression_units_over_allowance_fails_with_remediation() {
        let r = eval_regression_units(131, 100, 30); // delta 31 > 30
        assert_eq!(r.status, PolicyStatus::Fail);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn regression_units_improvement_never_fails() {
        let r = eval_regression_units(80, 100, 30); // delta -20
        assert_eq!(r.status, PolicyStatus::Pass);
        assert_eq!(r.actual, Some(-20.0));
    }

    #[test]
    fn cpi_count_exactly_at_limit_passes() {
        let r = eval_cpi_count(5, 5);
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn cpi_count_over_limit_fails_with_remediation() {
        let r = eval_cpi_count(6, 5);
        assert_eq!(r.status, PolicyStatus::Fail);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn cpi_depth_exactly_at_limit_passes() {
        let r = eval_cpi_depth(4, 4);
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn cpi_depth_over_limit_fails_with_remediation() {
        let r = eval_cpi_depth(5, 4);
        assert_eq!(r.status, PolicyStatus::Fail);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn unattributed_exactly_at_limit_passes() {
        let r = eval_unattributed(10.0, 10.0);
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn unattributed_over_limit_warns_with_remediation() {
        let r = eval_unattributed(11.0, 10.0);
        assert_eq!(r.status, PolicyStatus::Warn);
        assert!(r.remediation.is_some());
    }

    #[test]
    fn overhead_exactly_at_limit_passes() {
        let r = eval_overhead(5.0, 5.0);
        assert_eq!(r.status, PolicyStatus::Pass);
        assert!(r.remediation.is_none());
    }

    #[test]
    fn overhead_over_limit_warns_with_remediation() {
        let r = eval_overhead(6.0, 5.0);
        assert_eq!(r.status, PolicyStatus::Warn);
        assert!(r.remediation.is_some());
    }

    // ----------------------------------------------------------------------
    // Property / law tests for the five floating-point clauses.
    //
    // The integer/ordinal clauses are decidable exhaustively (∀ over the
    // domain), but the float math in these five is not. Instead we pin their
    // *safety laws* over a dense deterministic grid — no RNG, so the suite stays
    // reproducible — plus the exact-at-threshold case probed from both sides.
    // The laws: boundary correctness (`<=`/`>=`, never strict),
    // status/severity/remediation consistency, improvement-never-fails
    // (regression), and monotonicity (a worse input can never improve the
    // outcome).
    // ----------------------------------------------------------------------

    /// A representative grid of budget ceilings, from 1 CU to a full block.
    const MAXES: [u64; 5] = [1, 100, 1_000, 200_000, 1_400_000];

    /// `actual` at `i`% of `max` (i in 0..=100), via u128 so it never overflows.
    fn at_pct(max: u64, i: u64) -> u64 {
        (u128::from(max) * u128::from(i) / 100) as u64
    }

    /// Every clause that splits Pass vs not-Pass keeps status, severity and
    /// remediation in lockstep: Pass ⟺ Info ⟺ no remediation; any non-Pass
    /// carries a Warning/Error severity AND a remediation. Both `actual` and
    /// `expected` are always populated for the renderer.
    fn assert_consistent(r: &PolicyResult) {
        match r.status {
            PolicyStatus::Pass => {
                assert_eq!(r.severity, Severity::Info, "Pass must be Info: {r:?}");
                assert!(r.remediation.is_none(), "Pass carries remediation: {r:?}");
            }
            PolicyStatus::Warn | PolicyStatus::Fail => {
                assert!(
                    matches!(r.severity, Severity::Warning | Severity::Error),
                    "non-Pass must be Warning/Error: {r:?}"
                );
                assert!(
                    r.remediation.is_some(),
                    "non-Pass must carry remediation: {r:?}"
                );
            }
        }
        assert!(
            r.actual.is_some() && r.expected.is_some(),
            "actual/expected must be set: {r:?}"
        );
    }

    #[test]
    fn min_margin_is_monotonic_and_boundary_correct() {
        // As usage rises the margin falls, so the status may only move
        // Pass → Warn, never recover.
        for max in MAXES {
            for &min_margin in &[0.0_f64, 1.0, 5.0, 10.0, 50.0, 100.0] {
                let mut warned = false;
                for i in 0..=100u64 {
                    let r = eval_min_margin(at_pct(max, i), max, min_margin);
                    match r.status {
                        PolicyStatus::Warn => warned = true,
                        _ => assert!(
                            !warned,
                            "margin status recovered: max={max} min={min_margin} i={i}"
                        ),
                    }
                    assert_consistent(&r);
                }
            }
        }
        // Exact boundary: max=100, actual=90 → margin = 10.0 exactly.
        assert_eq!(eval_min_margin(90, 100, 10.0).status, PolicyStatus::Pass); // == → Pass
        assert_eq!(
            eval_min_margin(90, 100, 10.000_1).status,
            PolicyStatus::Warn
        ); // just over
        assert_eq!(eval_min_margin(90, 100, 9.999_9).status, PolicyStatus::Pass); // just under
    }

    #[test]
    fn warn_threshold_within_budget_is_monotonic_and_boundary_correct() {
        // Within budget, rising usage can only turn the warning on, never off.
        for max in MAXES {
            for &warn_pct in &[0.0_f64, 1.0, 50.0, 90.0, 100.0] {
                let mut warned = false;
                for i in 0..=100u64 {
                    match eval_warn_threshold(at_pct(max, i), max, warn_pct) {
                        Some(r) => {
                            warned = true;
                            assert_eq!(r.status, PolicyStatus::Warn);
                            assert_consistent(&r);
                        }
                        None => assert!(
                            !warned,
                            "warning cleared as usage rose: max={max} warn={warn_pct} i={i}"
                        ),
                    }
                }
            }
            // Over budget is the absolute clause's job, never this one's.
            assert!(eval_warn_threshold(max + 1, max, 0.0).is_none());
        }
        // Exact boundary: max=100, actual=90 → used = 90.0 exactly.
        assert!(eval_warn_threshold(90, 100, 90.0).is_some()); // == → warn
        assert!(eval_warn_threshold(90, 100, 90.000_1).is_none()); // just under → silent
        assert!(eval_warn_threshold(90, 100, 89.999_9).is_some()); // just over → warn
    }

    #[test]
    fn regression_pct_improvement_never_fails() {
        // The worst failure mode for a regression tool: a measurement at or
        // below baseline must never be reported as a regression, for any
        // non-negative allowance. (The float twin of the integer-clause law.)
        for base in [1u64, 100, 50_000, 200_000, 1_400_000] {
            for i in 0..=100u64 {
                let actual = at_pct(base, i); // 0..=base
                for &max_pct in &[0.0_f64, 1.0, 5.0, 25.0] {
                    let r = eval_regression_pct(actual, base, max_pct);
                    assert_eq!(
                        r.status,
                        PolicyStatus::Pass,
                        "improvement flagged as regression: actual={actual} base={base} allow={max_pct}"
                    );
                    assert_consistent(&r);
                }
            }
        }
    }

    #[test]
    fn regression_pct_boundary_and_zero_baseline() {
        // base=100, actual=110 → delta_pct = 10.0 exactly.
        assert_eq!(
            eval_regression_pct(110, 100, 10.0).status,
            PolicyStatus::Pass
        ); // == → Pass
        assert_eq!(
            eval_regression_pct(110, 100, 9.999_9).status,
            PolicyStatus::Fail
        ); // allowance just under
        assert_eq!(
            eval_regression_pct(110, 100, 10.000_1).status,
            PolicyStatus::Pass
        ); // allowance just over
        // A zero baseline short-circuits delta_pct to 0.0 → never a regression.
        for &max_pct in &[0.0_f64, 1.0, 100.0] {
            assert_eq!(
                eval_regression_pct(999_999, 0, max_pct).status,
                PolicyStatus::Pass
            );
        }
    }

    #[test]
    fn unattributed_and_overhead_obey_the_threshold_law() {
        // Both clauses share the shape `ok = actual_pct <= max_pct` (Pass/Warn).
        let clauses: [fn(f64, f64) -> PolicyResult; 2] = [eval_unattributed, eval_overhead];
        for eval in clauses {
            for &max_pct in &[0.0_f64, 1.0, 5.0, 50.0, 100.0] {
                // Exact boundary, both sides.
                assert_eq!(eval(max_pct, max_pct).status, PolicyStatus::Pass); // == → Pass
                assert_eq!(eval(max_pct + 0.000_1, max_pct).status, PolicyStatus::Warn); // over
                if max_pct > 0.0 {
                    assert_eq!(eval(max_pct - 0.000_1, max_pct).status, PolicyStatus::Pass);
                }
                // Monotonicity: once it warns, more usage never un-warns.
                let mut warned = false;
                let mut a = 0.0_f64;
                while a <= max_pct + 10.0 {
                    let r = eval(a, max_pct);
                    match r.status {
                        PolicyStatus::Warn => warned = true,
                        _ => assert!(
                            !warned,
                            "status recovered after warning: a={a} max={max_pct}"
                        ),
                    }
                    assert_consistent(&r);
                    a += 0.5;
                }
            }
        }
    }
}

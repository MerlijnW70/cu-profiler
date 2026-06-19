//! Individual diagnostic rules. Each inspects the analysed data and optionally
//! emits a [`Diagnostic`] with Solana-specific, actionable advice.

use crate::baseline::BaselineComparison;
use crate::budget::{PolicyResult, PolicyStatus, Severity};
use crate::confidence::{Confidence, ConfidenceLevel};
use crate::diagnostics::Diagnostic;
use crate::model::Measurement;
use crate::scenario::ExpectedResult;

/// CPI count beyond which we flag a likely explosion.
const CPI_EXPLOSION_THRESHOLD: u32 = 8;
/// CPI depth beyond which nesting is concerning.
const CPI_DEPTH_THRESHOLD: u32 = 4;
/// Fraction of requested compute left unused that we consider over-requesting.
const OVER_REQUEST_FRACTION: f64 = 0.5;
/// Unattributed CU share above which we suggest more markers.
const UNATTRIBUTED_THRESHOLD: f64 = 60.0;
/// CU above which a *failing* path is considered expensive.
const EXPENSIVE_FAILURE_CU: u64 = 5_000;
/// CPI share of total CU above which we surface it (informational).
const HIGH_CPI_SHARE_THRESHOLD: f64 = 70.0;
/// Log-line count above which we flag potential event/log bloat.
const LOG_BLOAT_THRESHOLD: usize = 25;

/// Context handed to every rule.
pub struct Context<'a> {
    /// Scenario name.
    pub scenario: &'a str,
    /// The measurement under inspection.
    pub measurement: &'a Measurement,
    /// Evaluated budget policy results.
    pub policy_results: &'a [PolicyResult],
    /// Baseline comparison, if any.
    pub baseline: Option<&'a BaselineComparison>,
    /// Confidence in the measurement.
    pub confidence: &'a Confidence,
    /// What the scenario expected to happen.
    pub expected: ExpectedResult,
    /// Number of scope markers detected (gates scope-attribution advice).
    pub scope_count: usize,
    /// Number of program log/data lines emitted (drives log-bloat detection).
    pub log_line_count: usize,
    /// Whether a validation scope opened after a CPI (marker-gated).
    pub late_validation: bool,
}

type Rule = fn(&Context) -> Option<Diagnostic>;

/// All rules, applied in order.
pub const RULES: &[Rule] = &[
    absolute_budget_exceeded,
    near_budget_limit,
    regression_exceeded,
    expensive_failure_path,
    cpi_explosion,
    high_cpi_depth,
    high_cpi_share,
    over_requested_compute,
    high_unattributed,
    event_log_bloat,
    late_validation,
    stale_baseline,
    low_confidence,
];

fn policy_status(ctx: &Context, id: &str) -> Option<PolicyStatus> {
    ctx.policy_results
        .iter()
        .find(|p| p.policy_id == id)
        .map(|p| p.status)
}

fn diag(
    ctx: &Context,
    id: &str,
    title: &str,
    severity: Severity,
    evidence: String,
    recommendation: &str,
) -> Diagnostic {
    Diagnostic {
        id: id.to_string(),
        title: title.to_string(),
        severity,
        scenario: ctx.scenario.to_string(),
        evidence,
        recommendation: recommendation.to_string(),
    }
}

fn absolute_budget_exceeded(ctx: &Context) -> Option<Diagnostic> {
    (policy_status(ctx, "absolute_max_cu") == Some(PolicyStatus::Fail)).then(|| {
        diag(
            ctx,
            "absolute_budget_exceeded",
            "Absolute compute budget exceeded",
            Severity::Error,
            format!("{} CU consumed", ctx.measurement.total_cu),
            "Reduce hot-path compute; profile the most expensive CPI and scope.",
        )
    })
}

fn near_budget_limit(ctx: &Context) -> Option<Diagnostic> {
    (policy_status(ctx, "warn_at_budget_pct") == Some(PolicyStatus::Warn)).then(|| {
        diag(
            ctx,
            "near_budget_limit",
            "Scenario is near its compute budget",
            Severity::Warning,
            format!("{} CU consumed", ctx.measurement.total_cu),
            "Leave headroom: a small regression could breach the budget.",
        )
    })
}

fn regression_exceeded(ctx: &Context) -> Option<Diagnostic> {
    let failed = policy_status(ctx, "max_regression_pct") == Some(PolicyStatus::Fail)
        || policy_status(ctx, "max_regression_units") == Some(PolicyStatus::Fail);
    failed.then(|| {
        let evidence = ctx
            .baseline
            .map(BaselineComparison::summary)
            .unwrap_or_else(|| "regression policy exceeded".to_string());
        diag(
            ctx,
            "regression_exceeded",
            "Compute regression exceeded policy",
            Severity::Error,
            evidence,
            "Inspect the CPI count and recently changed validation path.",
        )
    })
}

fn expensive_failure_path(ctx: &Context) -> Option<Diagnostic> {
    let failed = !ctx.measurement.simulation_success || ctx.expected == ExpectedResult::Failure;
    (failed && ctx.measurement.total_cu >= EXPENSIVE_FAILURE_CU).then(|| {
        diag(
            ctx,
            "expensive_failure_path",
            "Failure path consumes significant compute",
            Severity::Warning,
            format!("{} CU consumed before failing", ctx.measurement.total_cu),
            "Validate cheaply and early so rejected transactions fail fast.",
        )
    })
}

fn cpi_explosion(ctx: &Context) -> Option<Diagnostic> {
    (ctx.measurement.cpi_count >= CPI_EXPLOSION_THRESHOLD).then(|| {
        diag(
            ctx,
            "cpi_explosion",
            "High number of CPIs",
            Severity::Warning,
            format!("{} CPIs", ctx.measurement.cpi_count),
            "Check for duplicate ATA creation or batchable cross-program calls.",
        )
    })
}

fn high_cpi_depth(ctx: &Context) -> Option<Diagnostic> {
    (ctx.measurement.cpi_depth >= CPI_DEPTH_THRESHOLD).then(|| {
        diag(
            ctx,
            "high_cpi_depth",
            "Deep CPI nesting",
            Severity::Warning,
            format!("CPI depth {}", ctx.measurement.cpi_depth),
            "Deep nesting risks the runtime invoke-depth limit; flatten where possible.",
        )
    })
}

fn high_cpi_share(ctx: &Context) -> Option<Diagnostic> {
    // Share of total CU spent inside CPIs = the complement of the unattributed
    // (entrypoint-local) share. Only meaningful when CPIs were actually made.
    let cpi_share = 100.0 - ctx.measurement.unattributed_pct;
    (ctx.measurement.cpi_count > 0 && cpi_share >= HIGH_CPI_SHARE_THRESHOLD).then(|| {
        diag(
            ctx,
            "high_cpi_share",
            "Most compute is spent in CPIs",
            Severity::Info,
            format!("{cpi_share:.0}% of CU consumed inside CPIs"),
            "Review the most expensive cross-program call before optimising local code.",
        )
    })
}

fn event_log_bloat(ctx: &Context) -> Option<Diagnostic> {
    (ctx.log_line_count >= LOG_BLOAT_THRESHOLD).then(|| {
        diag(
            ctx,
            "event_log_bloat",
            "High log/event volume",
            Severity::Warning,
            format!("{} log line(s) emitted", ctx.log_line_count),
            "Reduce event emission in the hot path; logging itself costs compute.",
        )
    })
}

fn late_validation(ctx: &Context) -> Option<Diagnostic> {
    ctx.late_validation.then(|| {
        diag(
            ctx,
            "late_validation",
            "Validation runs after a CPI",
            Severity::Warning,
            "a validation scope opened after a cross-program invocation".to_string(),
            "Move cheap validation before CPIs so rejected transactions fail fast.",
        )
    })
}

fn over_requested_compute(ctx: &Context) -> Option<Diagnostic> {
    let limit = ctx.measurement.requested_limit?;
    let unused = ctx.measurement.over_requested?;
    (limit > 0 && (unused as f64 / limit as f64) >= OVER_REQUEST_FRACTION).then(|| {
        diag(
            ctx,
            "over_requested_compute",
            "Compute budget is over-requested",
            Severity::Info,
            format!("{unused} of {limit} requested CU unused"),
            "Lower the requested compute limit if it is consistently over-requested.",
        )
    })
}

fn high_unattributed(ctx: &Context) -> Option<Diagnostic> {
    // Only meaningful once the user has opted into scope attribution: a program
    // doing its own work without markers is not "unattributed" in any bad sense.
    (ctx.scope_count > 0 && ctx.measurement.unattributed_pct >= UNATTRIBUTED_THRESHOLD).then(|| {
        diag(
            ctx,
            "high_unattributed",
            "Large share of compute is unattributed",
            Severity::Info,
            format!("{:.0}% unattributed CU", ctx.measurement.unattributed_pct),
            "Add scope markers around account validation and math to attribute CU.",
        )
    })
}

fn stale_baseline(ctx: &Context) -> Option<Diagnostic> {
    let baseline = ctx.baseline?;
    (!baseline.matched).then(|| {
        diag(
            ctx,
            "stale_baseline",
            "Baseline is stale",
            Severity::Warning,
            baseline.stale_reasons.join("; "),
            "Re-record the baseline after confirming the change is intended.",
        )
    })
}

fn low_confidence(ctx: &Context) -> Option<Diagnostic> {
    matches!(
        ctx.confidence.level,
        ConfidenceLevel::Low | ConfidenceLevel::Unknown
    )
    .then(|| {
        diag(
            ctx,
            "low_confidence",
            "Measurement confidence is low",
            Severity::Warning,
            ctx.confidence.reasons.join("; "),
            "Treat the figure as indicative; resolve the listed reasons before gating on it.",
        )
    })
}

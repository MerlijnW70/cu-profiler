//! The report data model: the raw, serializable result of a profiling run.
//!
//! This module owns *data*. Rendering to table/JSON/Markdown/JUnit lives in the
//! `cu-profiler-report` crate, keeping raw data and presentation separate.

use serde::{Deserialize, Serialize};

use crate::baseline::BaselineComparison;
use crate::budget::PolicyResult;
use crate::confidence::Confidence;
use crate::diagnostics::Diagnostic;
use crate::metadata::RunMetadata;
use crate::parser::{CallNode, ScopeResult};

/// Headline pass/warn/fail/unknown status of a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// All policies satisfied.
    Pass,
    /// A soft threshold tripped.
    Warn,
    /// A hard limit breached, or an unexpected outcome.
    Fail,
    /// Not enough information to judge.
    Unknown,
}

impl Status {
    /// Uppercase label used in tables and JUnit.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Derive a status from a rolled-up budget [`crate::budget::PolicyStatus`].
    #[must_use]
    pub fn from_policy(p: crate::budget::PolicyStatus) -> Self {
        match p {
            crate::budget::PolicyStatus::Pass => Self::Pass,
            crate::budget::PolicyStatus::Warn => Self::Warn,
            crate::budget::PolicyStatus::Fail => Self::Fail,
        }
    }
}

/// Compute attributed to a single instruction within a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionMeasurement {
    /// Zero-based index in the transaction.
    pub index: usize,
    /// Program that owns the instruction.
    pub program_id: String,
    /// Resolved label, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// CU consumed, if the logs reported it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<u64>,
}

/// Distribution of `total_cu` across multiple measurement samples.
///
/// Only present when a scenario was run more than once on a *non-deterministic*
/// backend (`Scenario::samples > 1`); the recorded backend is deterministic, so it
/// never produces this — the tool never fabricates a spread it did not observe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SampleStats {
    /// Number of samples taken.
    pub count: u32,
    /// Smallest observed `total_cu`.
    pub min: u64,
    /// Median observed `total_cu`.
    pub median: u64,
    /// Largest observed `total_cu`.
    pub max: u64,
    /// Population variance of `total_cu`.
    pub variance: f64,
    /// Standard deviation of `total_cu` (`sqrt(variance)`).
    pub std_dev: f64,
    /// Coefficient of variation (`std_dev / mean`); `0.0` when the mean is `0`.
    pub cv: f64,
}

impl SampleStats {
    /// Compute statistics over the per-sample `total_cu` values, or `None` for
    /// fewer than two samples (a single run has no distribution).
    #[must_use]
    pub fn from_samples(totals: &[u64]) -> Option<Self> {
        if totals.len() < 2 {
            return None;
        }
        let count = totals.len() as u32;
        let min = *totals.iter().min()?;
        let max = *totals.iter().max()?;

        let mut sorted = totals.to_vec();
        sorted.sort_unstable();
        let mid = sorted.len() / 2;
        let median = if sorted.len() % 2 == 1 {
            sorted[mid]
        } else {
            (sorted[mid - 1] + sorted[mid]) / 2
        };

        let n = totals.len() as f64;
        let mean = totals.iter().map(|&x| x as f64).sum::<f64>() / n;
        let variance = totals
            .iter()
            .map(|&x| {
                let d = x as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / n;
        let std_dev = variance.sqrt();
        let cv = if mean == 0.0 { 0.0 } else { std_dev / mean };

        Some(Self {
            count,
            min,
            median,
            max,
            variance,
            std_dev,
            cv,
        })
    }
}

/// The quantitative core of a scenario result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// Total CU consumed across the transaction.
    pub total_cu: u64,
    /// CU actually consumed (equals `total_cu` for single-tx scenarios).
    pub consumed: u64,
    /// Compute-budget requested limit, if a `SetComputeUnitLimit` was present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_limit: Option<u64>,
    /// CU requested but unused (`requested_limit - consumed`), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_requested: Option<u64>,
    /// Number of CPI invocations observed.
    pub cpi_count: u32,
    /// Maximum CPI invoke depth observed.
    pub cpi_depth: u32,
    /// Percentage of total CU not attributed to any scope (0..=100).
    pub unattributed_pct: f64,
    /// Instrumentation overhead as a percentage of total CU, if estimable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumentation_overhead_pct: Option<f64>,
    /// Per-instruction breakdown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_instruction: Vec<InstructionMeasurement>,
    /// Distribution of `total_cu` across samples, when multi-sampled (>1 run on a
    /// non-deterministic backend). Absent for single-sample / recorded runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_stats: Option<SampleStats>,
    /// Whether the simulation completed successfully.
    pub simulation_success: bool,
}

impl Measurement {
    /// A zeroed measurement (successful, no compute) — a useful base for tests
    /// and for `..Measurement::empty()` struct-update syntax.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            total_cu: 0,
            consumed: 0,
            requested_limit: None,
            over_requested: None,
            cpi_count: 0,
            cpi_depth: 0,
            unattributed_pct: 0.0,
            instrumentation_overhead_pct: None,
            per_instruction: Vec::new(),
            sample_stats: None,
            simulation_success: true,
        }
    }
}

/// The full result for one scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioReport {
    /// Scenario name.
    pub name: String,
    /// Headline status.
    pub status: Status,
    /// Quantitative measurement.
    pub measurement: Measurement,
    /// Reconstructed CPI call tree, if logs allowed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_tree: Option<CallNode>,
    /// Scope attribution results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ScopeResult>,
    /// Budget policy results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_results: Vec<PolicyResult>,
    /// Diagnostics raised for this scenario.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Confidence in this measurement.
    pub confidence: Confidence,
    /// Baseline comparison, if a baseline was available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_comparison: Option<BaselineComparison>,
    /// Non-fatal parser warnings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parser_warnings: Vec<String>,
    /// Raw logs, included only when the caller opts in (they can be large).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_logs: Option<Vec<String>>,
}

/// Aggregate counts across all scenarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Number of scenarios profiled.
    pub total_scenarios: usize,
    /// How many passed.
    pub passed: usize,
    /// How many warned.
    pub warned: usize,
    /// How many failed.
    pub failed: usize,
    /// Sum of total CU across scenarios.
    pub total_cu: u64,
}

/// The complete profiling report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Aggregate summary.
    pub summary: Summary,
    /// Per-scenario results.
    pub scenarios: Vec<ScenarioReport>,
    /// Run metadata.
    pub metadata: RunMetadata,
}

impl Report {
    /// Assemble a report from scenario results, computing the summary.
    #[must_use]
    pub fn new(scenarios: Vec<ScenarioReport>, metadata: RunMetadata) -> Self {
        let mut summary = Summary {
            total_scenarios: scenarios.len(),
            passed: 0,
            warned: 0,
            failed: 0,
            total_cu: 0,
        };
        for s in &scenarios {
            summary.total_cu = summary.total_cu.saturating_add(s.measurement.total_cu);
            match s.status {
                Status::Pass => summary.passed += 1,
                Status::Warn => summary.warned += 1,
                Status::Fail | Status::Unknown => summary.failed += 1,
            }
        }
        Self {
            summary,
            scenarios,
            metadata,
        }
    }

    /// Did any scenario fail?
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.summary.failed > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confidence::Confidence;
    use crate::metadata::RunMetadata;

    #[test]
    fn sample_stats_needs_at_least_two_samples() {
        assert!(SampleStats::from_samples(&[]).is_none());
        assert!(SampleStats::from_samples(&[42]).is_none());
    }

    #[test]
    fn sample_stats_computes_spread() {
        let s = SampleStats::from_samples(&[100, 120, 110]).expect("two+ samples");
        assert_eq!(s.count, 3);
        assert_eq!(s.min, 100);
        assert_eq!(s.max, 120);
        assert_eq!(s.median, 110);
        // mean 110, population variance ((100)+(100)+0)/3 = 66.67, std ~8.16.
        assert!((s.variance - 66.6667).abs() < 0.01);
        assert!((s.std_dev - 8.165).abs() < 0.01);
        assert!((s.cv - 0.0742).abs() < 0.001);
    }

    #[test]
    fn sample_stats_identical_samples_have_zero_variance() {
        let s = SampleStats::from_samples(&[500, 500, 500, 500]).unwrap();
        assert_eq!(s.variance, 0.0);
        assert_eq!(s.cv, 0.0);
        assert_eq!(s.median, 500);
    }

    #[test]
    fn sample_stats_even_count_medians_the_middle_pair() {
        let s = SampleStats::from_samples(&[10, 20, 30, 40]).unwrap();
        assert_eq!(s.median, 25); // (20 + 30) / 2
    }

    fn scenario(name: &str, status: Status, cu: u64) -> ScenarioReport {
        ScenarioReport {
            name: name.into(),
            status,
            measurement: Measurement {
                total_cu: cu,
                ..Measurement::empty()
            },
            call_tree: None,
            scopes: Vec::new(),
            policy_results: Vec::new(),
            diagnostics: Vec::new(),
            confidence: Confidence::high(),
            baseline_comparison: None,
            parser_warnings: Vec::new(),
            raw_logs: None,
        }
    }

    #[test]
    fn summary_counts_and_totals() {
        let r = Report::new(
            vec![
                scenario("a", Status::Pass, 100),
                scenario("b", Status::Warn, 200),
                scenario("c", Status::Fail, 300),
            ],
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(r.summary.passed, 1);
        assert_eq!(r.summary.warned, 1);
        assert_eq!(r.summary.failed, 1);
        assert_eq!(r.summary.total_cu, 600);
        assert!(r.has_failures());
    }

    #[test]
    fn status_labels_are_stable() {
        assert_eq!(Status::Pass.label(), "PASS");
        assert_eq!(Status::Warn.label(), "WARN");
        assert_eq!(Status::Fail.label(), "FAIL");
        assert_eq!(Status::Unknown.label(), "UNKNOWN");
    }

    #[test]
    fn two_samples_compute_even_median() {
        // Exactly two samples must produce stats (the `len < 2` boundary), and an
        // even count medians the middle pair rather than an endpoint.
        let s = SampleStats::from_samples(&[10, 20]).expect("two samples compute");
        assert_eq!(s.count, 2);
        assert_eq!(s.median, 15); // (10 + 20) / 2
    }

    #[test]
    fn report_without_failures_is_clean() {
        let r = Report::new(
            vec![scenario("ok", Status::Pass, 100)],
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(r.summary.failed, 0);
        assert!(!r.has_failures());
    }
}

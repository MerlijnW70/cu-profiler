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
}

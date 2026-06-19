//! Structured outcomes of evaluating a [`super::BudgetPolicy`].

use serde::{Deserialize, Serialize};

/// Tri-state outcome of a single policy check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStatus {
    /// Within limits.
    Pass,
    /// Within hard limits but past a soft/warning threshold.
    Warn,
    /// A hard limit was exceeded.
    Fail,
}

impl PolicyStatus {
    /// Uppercase label (`PASS` / `WARN` / `FAIL`).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }

    /// The more severe of two statuses (`Fail` > `Warn` > `Pass`).
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        self.max_rank(other)
    }

    fn rank(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Warn => 1,
            Self::Fail => 2,
        }
    }

    fn max_rank(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }
}

/// Severity classification, independent of pass/fail (a `Warn` can still be
/// `Error`-severity for a critical scenario).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational.
    Info,
    /// Worth attention.
    Warning,
    /// Blocking.
    Error,
}

/// The result of evaluating one policy clause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyResult {
    /// Stable identifier of the clause, e.g. `"absolute_max_cu"`.
    pub policy_id: String,
    /// Pass/warn/fail outcome.
    pub status: PolicyStatus,
    /// Severity classification.
    pub severity: Severity,
    /// The measured value, if numeric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<f64>,
    /// The threshold compared against, if numeric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<f64>,
    /// Human-readable explanation.
    pub message: String,
    /// Optional Solana-specific remediation hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

impl PolicyResult {
    /// Convenience constructor for a passing result.
    #[must_use]
    pub fn pass(policy_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            policy_id: policy_id.into(),
            status: PolicyStatus::Pass,
            severity: Severity::Info,
            actual: None,
            expected: None,
            message: message.into(),
            remediation: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_max_picks_worst() {
        assert_eq!(
            PolicyStatus::Pass.max(PolicyStatus::Fail),
            PolicyStatus::Fail
        );
        assert_eq!(
            PolicyStatus::Warn.max(PolicyStatus::Pass),
            PolicyStatus::Warn
        );
    }
}

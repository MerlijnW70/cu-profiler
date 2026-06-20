//! The first-class [`Scenario`] type.
//!
//! A scenario is not merely a test — it is a reproducible compute benchmark with
//! an expected outcome, a budget policy, and metadata used for fingerprinting.

use serde::{Deserialize, Serialize};

use crate::budget::BudgetPolicy;

/// How important a scenario is. Drives diagnostic severity and `--strict` gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Criticality {
    /// Failure must block CI.
    Critical,
    /// Notable but non-blocking by default.
    #[default]
    Normal,
    /// Informational only.
    Low,
}

/// What a scenario is expected to do. Failure paths are first-class: a failing
/// instruction that burns CU is relevant for both performance and security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExpectedResult {
    /// The transaction is expected to succeed.
    #[default]
    Success,
    /// The transaction is expected to fail (a measured failure path).
    Failure,
}

/// A reproducible compute benchmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable, hierarchical name, e.g. `swap/referral_enabled`.
    pub name: String,
    /// Human description of what the scenario exercises.
    #[serde(default)]
    pub description: String,
    /// Free-form tags used for filtering (`--tag`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// How critical the scenario is.
    #[serde(default)]
    pub criticality: Criticality,
    /// Optional owner (team or person) for triage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Expected outcome.
    #[serde(default)]
    pub expected: ExpectedResult,
    /// The budget policy applied to this scenario.
    #[serde(default)]
    pub budget: BudgetPolicy,
    /// How many samples to take when measuring (>= 1). **Reserved**: the
    /// recorded backend is deterministic so it ignores this; it will apply to
    /// live backends that exhibit run-to-run variance (see the roadmap).
    #[serde(default = "default_samples")]
    pub samples: u32,
}

fn default_samples() -> u32 {
    1
}

impl Scenario {
    /// A minimal scenario with the given name and default policy.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            tags: Vec::new(),
            criticality: Criticality::Normal,
            owner: None,
            expected: ExpectedResult::Success,
            budget: BudgetPolicy::default(),
            samples: 1,
        }
    }

    /// Does this scenario carry the given tag?
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Scenario::new("swap/happy_path");
        assert_eq!(s.samples, 1);
        assert_eq!(s.expected, ExpectedResult::Success);
        assert_eq!(s.criticality, Criticality::Normal);
    }

    #[test]
    fn tag_filtering() {
        let mut s = Scenario::new("swap/large_pool");
        s.tags = vec!["swap".into(), "hot-path".into()];
        assert!(s.has_tag("hot-path"));
        assert!(!s.has_tag("admin"));
    }
}

//! Comparing a current measurement against a stored baseline record.

use serde::{Deserialize, Serialize};

use crate::baseline::fingerprint::Fingerprint;
use crate::model::Measurement;

/// The result of comparing a current run to a baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// Whether the fingerprints matched (a non-stale comparison).
    pub matched: bool,
    /// Reasons the baseline is stale (empty when `matched`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_reasons: Vec<String>,
    /// CU recorded in the baseline.
    pub baseline_units: u64,
    /// CU measured in the current run.
    pub current_units: u64,
    /// Signed delta (`current - baseline`).
    pub delta_units: i64,
    /// Percentage delta relative to the baseline.
    pub delta_pct: f64,
}

impl BaselineComparison {
    /// Compute a comparison from a baseline figure + fingerprint and the current
    /// measurement + fingerprint.
    #[must_use]
    pub fn compute(
        baseline_units: u64,
        baseline_fp: &Fingerprint,
        current: &Measurement,
        current_fp: &Fingerprint,
    ) -> Self {
        let stale_reasons = baseline_fp.staleness_reasons(current_fp);
        let current_units = current.total_cu;
        let delta_units = current_units as i64 - baseline_units as i64;
        let delta_pct = if baseline_units == 0 {
            0.0
        } else {
            (delta_units as f64 / baseline_units as f64) * 100.0
        };
        Self {
            matched: stale_reasons.is_empty(),
            stale_reasons,
            baseline_units,
            current_units,
            delta_units,
            delta_pct,
        }
    }

    /// A one-line human summary, e.g. `+6.15% (+5608 CU) vs baseline`.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{:+.2}% ({:+} CU) vs baseline {}",
            self.delta_pct, self.delta_units, self.baseline_units
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(fixture: &str) -> Fingerprint {
        Fingerprint::new("s", fixture, "cfg", None)
    }

    fn measurement(cu: u64) -> Measurement {
        Measurement {
            total_cu: cu,
            ..Measurement::empty()
        }
    }

    #[test]
    fn matched_comparison_reports_delta() {
        let c = BaselineComparison::compute(91_204, &fp("a"), &measurement(96_812), &fp("a"));
        assert!(c.matched);
        assert_eq!(c.delta_units, 5_608);
        assert!((c.delta_pct - 6.15).abs() < 0.01);
    }

    #[test]
    fn changed_fixture_is_stale() {
        let c = BaselineComparison::compute(91_204, &fp("a"), &measurement(96_812), &fp("b"));
        assert!(!c.matched);
        assert!(c.stale_reasons.iter().any(|r| r.contains("fixture hash")));
    }
}

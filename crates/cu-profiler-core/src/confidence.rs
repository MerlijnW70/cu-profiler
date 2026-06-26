//! Confidence scoring.
//!
//! Every measurement carries a [`Confidence`]. The tool never claims more
//! certainty than the evidence supports, and it always explains *why* a score
//! is not [`ConfidenceLevel::High`].

use serde::{Deserialize, Serialize};

/// Qualitative confidence in a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    /// Reasons are present but do not undermine the result.
    Unknown,
    /// Multiple weak signals; treat the number as indicative only.
    Low,
    /// Minor caveats; the number is broadly trustworthy.
    Medium,
    /// No material caveats detected.
    High,
}

impl ConfidenceLevel {
    /// Lowercase, human-facing label (`"High"`, `"Medium"`, ...).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::Unknown => "Unknown",
        }
    }
}

/// A confidence score plus the reasons that shaped it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confidence {
    /// The qualitative level.
    pub level: ConfidenceLevel,
    /// Why the level is what it is. Always populated when `level != High`.
    pub reasons: Vec<String>,
}

impl Confidence {
    /// A high-confidence score with no caveats.
    #[must_use]
    pub fn high() -> Self {
        Self {
            level: ConfidenceLevel::High,
            reasons: Vec::new(),
        }
    }

    /// An unknown score with a single explanatory reason.
    #[must_use]
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            level: ConfidenceLevel::Unknown,
            reasons: vec![reason.into()],
        }
    }
}

/// Inputs to confidence scoring. Caller fills in what it knows; absent signals
/// are conservative defaults.
#[derive(Debug, Clone)]
pub struct ConfidenceFactors {
    /// Did the simulation succeed (or fail as expected)?
    pub simulation_ok: bool,
    /// Were the logs parsed without leftover unrecognised lines?
    pub logs_complete: bool,
    /// Number of parser warnings collected.
    pub parser_warnings: usize,
    /// Did the baseline fingerprint match (None when no baseline was compared)?
    pub baseline_matched: Option<bool>,
    /// Percentage of total CU that could not be attributed to a scope (0..=100).
    pub unattributed_pct: f64,
    /// Number of scope markers detected.
    pub scope_markers: usize,
    /// Whether runtime/version metadata was available.
    pub metadata_available: bool,
    /// Coefficient of variation of `total_cu` across samples, when multi-sampled.
    /// `None` for a single sample / deterministic backend.
    pub sample_cv: Option<f64>,
}

impl Default for ConfidenceFactors {
    fn default() -> Self {
        Self {
            simulation_ok: true,
            logs_complete: true,
            parser_warnings: 0,
            baseline_matched: None,
            unattributed_pct: 0.0,
            scope_markers: 0,
            metadata_available: false,
            sample_cv: None,
        }
    }
}

/// Score a measurement from its [`ConfidenceFactors`].
///
/// The model is deliberately simple and monotone: each adverse signal can only
/// lower the level, never raise it, and each contributes a reason string.
#[must_use]
pub fn score(factors: &ConfidenceFactors) -> Confidence {
    // `level` only ever moves downward. Because the enum is ordered
    // `Unknown < Low < Medium < High`, the worse level is the smaller one, so
    // `level.min(target)` demotes correctly.
    let mut level = ConfidenceLevel::High;
    let mut reasons = Vec::new();

    if !factors.simulation_ok {
        level = level.min(ConfidenceLevel::Low);
        reasons.push("simulation did not complete as expected".to_string());
    }
    if !factors.logs_complete {
        level = level.min(ConfidenceLevel::Low);
        reasons.push("logs were incomplete or contained unrecognised lines".to_string());
    }
    if factors.parser_warnings > 0 {
        level = level.min(ConfidenceLevel::Medium);
        reasons.push(format!("{} parser warning(s)", factors.parser_warnings));
    }
    match factors.baseline_matched {
        Some(true) => reasons.push("baseline matched".to_string()),
        Some(false) => {
            level = level.min(ConfidenceLevel::Low);
            reasons.push("baseline fingerprint did not match".to_string());
        }
        None => {}
    }
    if factors.unattributed_pct >= 20.0 {
        level = level.min(ConfidenceLevel::Medium);
        reasons.push(format!("{:.0}% unattributed CU", factors.unattributed_pct));
    }
    if factors.scope_markers > 0 {
        reasons.push(format!("{} scope markers detected", factors.scope_markers));
    }
    // Run-to-run variance across samples: a wide spread means the headline number
    // is less trustworthy. Thresholds are on the coefficient of variation.
    if let Some(cv) = factors.sample_cv {
        if cv >= 0.10 {
            level = level.min(ConfidenceLevel::Low);
            reasons.push(format!("high run-to-run variance (CV {:.1}%)", cv * 100.0));
        } else if cv >= 0.02 {
            level = level.min(ConfidenceLevel::Medium);
            reasons.push(format!("run-to-run variance (CV {:.1}%)", cv * 100.0));
        }
    }
    if !factors.metadata_available {
        level = level.min(ConfidenceLevel::Medium);
        reasons.push("runtime/version metadata unavailable".to_string());
    }

    Confidence { level, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_run_with_metadata_is_high() {
        let f = ConfidenceFactors {
            metadata_available: true,
            ..Default::default()
        };
        assert_eq!(score(&f).level, ConfidenceLevel::High);
    }

    #[test]
    fn failed_simulation_is_low() {
        let f = ConfidenceFactors {
            simulation_ok: false,
            metadata_available: true,
            ..Default::default()
        };
        assert_eq!(score(&f).level, ConfidenceLevel::Low);
    }

    #[test]
    fn unattributed_cu_demotes_to_medium_with_reason() {
        let f = ConfidenceFactors {
            unattributed_pct: 22.0,
            metadata_available: true,
            ..Default::default()
        };
        let c = score(&f);
        assert_eq!(c.level, ConfidenceLevel::Medium);
        assert!(c.reasons.iter().any(|r| r.contains("22% unattributed")));
    }

    #[test]
    fn sample_variance_demotes_confidence() {
        // Low spread → Medium; high spread → Low.
        let medium = ConfidenceFactors {
            metadata_available: true,
            sample_cv: Some(0.05),
            ..Default::default()
        };
        let c = score(&medium);
        assert_eq!(c.level, ConfidenceLevel::Medium);
        assert!(c.reasons.iter().any(|r| r.contains("variance")));

        let low = ConfidenceFactors {
            metadata_available: true,
            sample_cv: Some(0.25),
            ..Default::default()
        };
        assert_eq!(score(&low).level, ConfidenceLevel::Low);

        // A tiny spread is within tolerance and stays High.
        let high = ConfidenceFactors {
            metadata_available: true,
            sample_cv: Some(0.005),
            ..Default::default()
        };
        assert_eq!(score(&high).level, ConfidenceLevel::High);
    }

    #[test]
    fn levels_order_high_above_low() {
        assert!(ConfidenceLevel::High > ConfidenceLevel::Low);
        assert!(ConfidenceLevel::Medium > ConfidenceLevel::Unknown);
    }

    #[test]
    fn level_labels_are_stable() {
        assert_eq!(ConfidenceLevel::High.label(), "High");
        assert_eq!(ConfidenceLevel::Medium.label(), "Medium");
        assert_eq!(ConfidenceLevel::Low.label(), "Low");
        assert_eq!(ConfidenceLevel::Unknown.label(), "Unknown");
    }

    #[test]
    fn parser_warnings_demote_to_medium_with_reason() {
        let f = ConfidenceFactors {
            parser_warnings: 1,
            metadata_available: true,
            ..Default::default()
        };
        let c = score(&f);
        assert_eq!(c.level, ConfidenceLevel::Medium);
        assert!(c.reasons.iter().any(|r| r.contains("1 parser warning")));
    }

    #[test]
    fn scope_markers_add_a_reason_when_present() {
        let f = ConfidenceFactors {
            scope_markers: 2,
            metadata_available: true,
            ..Default::default()
        };
        assert!(
            score(&f)
                .reasons
                .iter()
                .any(|r| r.contains("2 scope markers detected"))
        );
    }

    #[test]
    fn no_scope_markers_adds_no_marker_reason() {
        let f = ConfidenceFactors {
            scope_markers: 0,
            metadata_available: true,
            ..Default::default()
        };
        assert!(
            !score(&f)
                .reasons
                .iter()
                .any(|r| r.contains("scope markers detected"))
        );
    }
}

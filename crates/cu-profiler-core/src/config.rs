//! `cu-profiler.toml` parsing.
//!
//! Parsing is strict — unknown keys are rejected — but every failure is turned
//! into a clear [`crate::Error::Config`] message. Per-scenario settings overlay
//! the project defaults to form an effective [`BudgetPolicy`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::budget::BudgetPolicy;
use crate::error::Error;
use crate::scenario::{Criticality, Scenario};

/// Top-level configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Project identity.
    pub project: ProjectConfig,
    /// Default policy + CI behaviour.
    #[serde(default)]
    pub defaults: DefaultsConfig,
    /// Output destinations and default format.
    #[serde(default)]
    pub output: OutputConfig,
    /// Extra program-ID labels.
    #[serde(default)]
    pub program_labels: BTreeMap<String, String>,
    /// Per-scenario configuration, keyed by scenario name.
    #[serde(default)]
    pub scenario: BTreeMap<String, ScenarioConfig>,
}

/// `[project]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Human project name.
    pub name: String,
    /// Program ID under test, if fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    /// Execution mode (`program-test`, `banks-client`, `recorded`).
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "program-test".to_string()
}

/// `[defaults]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct DefaultsConfig {
    /// Warn once this percentage of budget is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_at_budget_pct: Option<f64>,
    /// Maximum tolerated regression percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_regression_pct: Option<f64>,
    /// Fail CI when an absolute budget is exceeded.
    pub fail_on_budget: bool,
    /// Fail CI on regression.
    pub fail_on_regression: bool,
    /// Fail CI when the baseline is stale.
    pub fail_on_stale_baseline: bool,
}

/// `[output]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OutputConfig {
    /// Default render format (`table`, `json`, `markdown`, `junit`).
    pub default_format: String,
    /// JSON report path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_path: Option<PathBuf>,
    /// Markdown report path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown_path: Option<PathBuf>,
    /// JUnit report path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub junit_path: Option<PathBuf>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            default_format: "table".to_string(),
            json_path: None,
            markdown_path: None,
            junit_path: None,
        }
    }
}

/// `[scenario.<name>]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ScenarioConfig {
    /// Absolute CU budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<u64>,
    /// Per-scenario warn threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_at_budget_pct: Option<f64>,
    /// Per-scenario regression allowance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_regression_pct: Option<f64>,
    /// Whether the scenario is critical.
    pub critical: bool,
    /// Tags.
    pub tags: Vec<String>,
    /// Description.
    pub description: String,
}

impl Config {
    /// Parse configuration from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(s).map_err(|e| Error::Config(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Load configuration from a file path.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("cannot read config `{}`: {e}", path.display())))?;
        Self::from_toml(&text)
    }

    fn validate(&self) -> Result<()> {
        const FORMATS: &[&str] = &["table", "json", "markdown", "junit"];
        if !FORMATS.contains(&self.output.default_format.as_str()) {
            return Err(Error::Config(format!(
                "output.default_format `{}` is not one of {FORMATS:?}",
                self.output.default_format
            )));
        }
        Ok(())
    }

    /// The default budget policy assembled from `[defaults]`.
    #[must_use]
    pub fn default_policy(&self) -> BudgetPolicy {
        BudgetPolicy {
            warn_at_budget_pct: self.defaults.warn_at_budget_pct,
            max_regression_pct: self.defaults.max_regression_pct,
            ..Default::default()
        }
    }

    /// The effective budget policy for a scenario (defaults overlaid by the
    /// per-scenario settings).
    #[must_use]
    pub fn effective_policy(&self, scenario: &str) -> BudgetPolicy {
        let base = self.default_policy();
        match self.scenario.get(scenario) {
            Some(sc) => base.merged_with(&BudgetPolicy {
                absolute_max_cu: sc.budget,
                warn_at_budget_pct: sc.warn_at_budget_pct,
                max_regression_pct: sc.max_regression_pct,
                ..Default::default()
            }),
            None => base,
        }
    }

    /// Build [`Scenario`] values from the configured scenarios.
    #[must_use]
    pub fn scenarios(&self) -> Vec<Scenario> {
        self.scenario
            .iter()
            .map(|(name, sc)| Scenario {
                name: name.clone(),
                description: sc.description.clone(),
                tags: sc.tags.clone(),
                criticality: if sc.critical {
                    Criticality::Critical
                } else {
                    Criticality::Normal
                },
                owner: None,
                expected: crate::scenario::ExpectedResult::Success,
                budget: self.effective_policy(name),
                samples: 1,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[project]
name = "my-solana-program"
mode = "program-test"

[defaults]
warn_at_budget_pct = 90
max_regression_pct = 5
fail_on_budget = true
fail_on_regression = true
fail_on_stale_baseline = false

[output]
default_format = "table"

[program_labels]
"11111111111111111111111111111111" = "System Program"

[scenario.swap_exact_in]
budget = 100000
warn_at_budget_pct = 90
max_regression_pct = 5
critical = true
tags = ["swap", "hot-path"]

[scenario.initialize_pool]
budget = 80000
max_regression_pct = 3
critical = true
"#;

    #[test]
    fn parses_sample_config() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        assert_eq!(cfg.project.name, "my-solana-program");
        assert_eq!(cfg.scenario.len(), 2);
        assert!(cfg.defaults.fail_on_budget);
    }

    #[test]
    fn effective_policy_overlays_defaults() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        let p = cfg.effective_policy("initialize_pool");
        assert_eq!(p.absolute_max_cu, Some(80_000));
        // default warn threshold flows through; regression overridden to 3.
        assert_eq!(p.warn_at_budget_pct, Some(90.0));
        assert_eq!(p.max_regression_pct, Some(3.0));
    }

    #[test]
    fn rejects_unknown_format() {
        let toml = "[project]\nname = \"x\"\n[output]\ndefault_format = \"yaml\"\n";
        let err = Config::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("default_format"));
    }

    #[test]
    fn rejects_unknown_key() {
        let toml = "[project]\nname = \"x\"\nbogus = 1\n";
        assert!(Config::from_toml(toml).is_err());
    }

    #[test]
    fn builds_scenarios() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        let scenarios = cfg.scenarios();
        assert_eq!(scenarios.len(), 2);
        assert!(scenarios.iter().any(|s| s.name == "swap_exact_in"));
    }
}

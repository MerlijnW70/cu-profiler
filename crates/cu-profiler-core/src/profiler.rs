//! The profiler engine: runs scenarios through a backend, parses the logs, and
//! assembles a [`Report`] — applying budget policies, baseline comparison,
//! confidence scoring and diagnostics along the way.

use crate::backend::ExecutionBackend;
use crate::baseline::{BaselineComparison, BaselineStore, Fingerprint};
use crate::budget::{self, PolicyResult};
use crate::confidence::{self, Confidence, ConfidenceFactors};
use crate::diagnostics::{self, Context};
use crate::metadata::RunMetadata;
use crate::model::{InstructionMeasurement, Measurement, Report, ScenarioReport, Status};
use crate::parser::{self, ParseAnalysis};
use crate::program_registry::ProgramRegistry;
use crate::scenario::{ExpectedResult, Scenario};

/// Configurable profiler engine.
#[derive(Debug, Clone)]
pub struct Profiler {
    registry: ProgramRegistry,
    config_repr: String,
    include_raw_logs: bool,
}

impl Default for Profiler {
    fn default() -> Self {
        Self {
            registry: ProgramRegistry::with_builtins(),
            config_repr: String::new(),
            include_raw_logs: false,
        }
    }
}

impl Profiler {
    /// A profiler with the built-in program registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the program registry (e.g. extended from config labels).
    #[must_use]
    pub fn with_registry(mut self, registry: ProgramRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Set a string representation of the effective config, hashed into
    /// fingerprints so baselines go stale when configuration changes.
    #[must_use]
    pub fn with_config_repr(mut self, repr: impl Into<String>) -> Self {
        self.config_repr = repr.into();
        self
    }

    /// Include raw logs in each scenario report (they can be large).
    #[must_use]
    pub fn include_raw_logs(mut self, yes: bool) -> Self {
        self.include_raw_logs = yes;
        self
    }

    /// The fingerprint of a scenario under the current config.
    #[must_use]
    pub fn fingerprint(&self, scenario: &Scenario) -> Fingerprint {
        Fingerprint::new(
            &format!("{scenario:?}"),
            &scenario.name,
            &self.config_repr,
            None,
        )
    }

    /// Run all `scenarios` through `backend`, comparing against `baseline` when
    /// provided, and assemble a [`Report`].
    #[must_use]
    pub fn run(
        &self,
        backend: &dyn ExecutionBackend,
        scenarios: &[Scenario],
        baseline: Option<&BaselineStore>,
        metadata: RunMetadata,
    ) -> Report {
        let reports = scenarios
            .iter()
            .map(|s| self.profile_one(backend, s, baseline))
            .collect();
        Report::new(reports, metadata)
    }

    fn profile_one(
        &self,
        backend: &dyn ExecutionBackend,
        scenario: &Scenario,
        baseline: Option<&BaselineStore>,
    ) -> ScenarioReport {
        // The first run drives the structural report (call tree, per-instruction,
        // scopes). On a non-deterministic backend, extra samples add only their
        // `total_cu` to a distribution; a deterministic backend (recorded replay)
        // is run once — multi-sampling it would fabricate a spread of zero.
        let output = match backend.run(scenario) {
            Ok(output) => output,
            Err(e) => return self.simulation_error_report(scenario, &e.to_string()),
        };
        let analysis = parser::analyze(&output.logs, &self.registry);

        let extra = if backend.is_deterministic() {
            0
        } else {
            u64::from(scenario.samples.saturating_sub(1))
        };
        let mut totals = vec![analysis.total_cu];
        for _ in 0..extra {
            // Best-effort: a sample that fails to execute or parse is skipped, not
            // fatal — the surviving samples still yield an honest distribution.
            if let Ok(o) = backend.run(scenario) {
                totals.push(parser::analyze(&o.logs, &self.registry).total_cu);
            }
        }
        let sample_stats = crate::model::SampleStats::from_samples(&totals);

        self.assemble(
            scenario,
            analysis,
            output.success,
            output.logs,
            baseline,
            sample_stats,
        )
    }

    fn assemble(
        &self,
        scenario: &Scenario,
        analysis: ParseAnalysis,
        sim_success: bool,
        logs: Vec<String>,
        baseline: Option<&BaselineStore>,
        sample_stats: Option<crate::model::SampleStats>,
    ) -> ScenarioReport {
        // Each top-level (depth-1) invocation is one transaction instruction.
        let per_instruction: Vec<InstructionMeasurement> = analysis
            .call_tree
            .children
            .iter()
            .enumerate()
            .map(|(index, node)| InstructionMeasurement {
                index,
                program_id: node.program_id.clone(),
                label: node.label.clone(),
                consumed: node.units_consumed,
            })
            .collect();

        let measurement = Measurement {
            total_cu: analysis.total_cu,
            consumed: analysis.total_cu,
            requested_limit: analysis.requested_limit,
            over_requested: analysis.over_requested,
            cpi_count: analysis.cpi_count,
            cpi_depth: analysis.cpi_depth,
            unattributed_pct: analysis.unattributed_pct,
            instrumentation_overhead_pct: None,
            per_instruction,
            sample_stats,
            simulation_success: sim_success && analysis.simulation_success,
        };

        // Baseline comparison.
        let current_fp = self.fingerprint(scenario);
        let comparison = baseline
            .and_then(|store| store.get(&scenario.name))
            .map(|record| {
                BaselineComparison::compute(
                    record.actual_units,
                    &record.fingerprint,
                    &measurement,
                    &current_fp,
                )
            });
        let baseline_units = comparison
            .as_ref()
            .filter(|c| c.matched)
            .map(|c| c.baseline_units);

        // Budget policy.
        let policy_results: Vec<PolicyResult> =
            budget::evaluate(&measurement, &scenario.budget, baseline_units);

        // Confidence.
        let confidence = self.score_confidence(
            &analysis,
            comparison.as_ref(),
            measurement.sample_stats.map(|s| s.cv),
        );

        // Status.
        let status = self.derive_status(&measurement, &policy_results, scenario.expected);

        // Diagnostics.
        let ctx = Context {
            scenario: &scenario.name,
            measurement: &measurement,
            policy_results: &policy_results,
            baseline: comparison.as_ref(),
            confidence: &confidence,
            expected: scenario.expected,
            scope_count: analysis.scope_marker_count,
            log_line_count: analysis.log_line_count,
            late_validation: analysis.validation_after_cpi,
        };
        let diags = diagnostics::evaluate(&ctx);

        ScenarioReport {
            name: scenario.name.clone(),
            status,
            measurement,
            call_tree: Some(analysis.call_tree),
            scopes: analysis.scopes,
            policy_results,
            diagnostics: diags,
            confidence,
            baseline_comparison: comparison,
            parser_warnings: analysis.warnings,
            raw_logs: self.include_raw_logs.then_some(logs),
        }
    }

    fn score_confidence(
        &self,
        analysis: &ParseAnalysis,
        comparison: Option<&BaselineComparison>,
        sample_cv: Option<f64>,
    ) -> Confidence {
        // Unattributed CU only counts against confidence when the user opted
        // into scope attribution; otherwise it is just normal program work.
        let unattributed_pct = if analysis.scope_marker_count > 0 {
            analysis.unattributed_pct
        } else {
            0.0
        };
        let factors = ConfidenceFactors {
            simulation_ok: analysis.simulation_success,
            logs_complete: analysis.logs_complete,
            parser_warnings: analysis.warnings.len(),
            baseline_matched: comparison.map(|c| c.matched),
            unattributed_pct,
            scope_markers: analysis.scope_marker_count,
            metadata_available: true,
            sample_cv,
        };
        confidence::score(&factors)
    }

    fn derive_status(
        &self,
        measurement: &Measurement,
        policy_results: &[PolicyResult],
        expected: ExpectedResult,
    ) -> Status {
        // An unexpected simulation outcome is a failure regardless of budgets.
        let outcome_ok = match expected {
            ExpectedResult::Success => measurement.simulation_success,
            ExpectedResult::Failure => !measurement.simulation_success,
        };
        if !outcome_ok {
            return Status::Fail;
        }
        Status::from_policy(budget::overall_status(policy_results))
    }

    fn simulation_error_report(&self, scenario: &Scenario, error: &str) -> ScenarioReport {
        ScenarioReport {
            name: scenario.name.clone(),
            status: Status::Unknown,
            measurement: Measurement {
                simulation_success: false,
                ..Measurement::empty()
            },
            call_tree: None,
            scopes: Vec::new(),
            policy_results: Vec::new(),
            diagnostics: Vec::new(),
            confidence: Confidence::unknown(format!("simulation error: {error}")),
            baseline_comparison: None,
            parser_warnings: vec![format!("simulation error: {error}")],
            raw_logs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::RecordedLogsBackend;
    use crate::budget::BudgetPolicy;

    fn backend() -> RecordedLogsBackend {
        let mut b = RecordedLogsBackend::new();
        b.insert_blob(
            "swap",
            "Program User111 invoke [1]\n\
             Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]\n\
             Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 3000 of 197000 compute units\n\
             Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success\n\
             Program User111 consumed 96000 of 200000 compute units\n\
             Program User111 success",
            true,
        );
        b
    }

    fn swap_scenario(max: u64) -> Scenario {
        let mut s = Scenario::new("swap");
        s.budget = BudgetPolicy {
            absolute_max_cu: Some(max),
            warn_at_budget_pct: Some(90.0),
            ..Default::default()
        };
        s
    }

    #[test]
    fn end_to_end_pass() {
        let report = Profiler::new().run(
            &backend(),
            &[swap_scenario(200_000)],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(report.scenarios[0].status, Status::Pass);
        assert_eq!(report.scenarios[0].measurement.total_cu, 96_000);
        assert_eq!(
            report.scenarios[0].confidence.level,
            confidence::ConfidenceLevel::High
        );

        // The single top-level program invocation is recorded as one instruction.
        let per = &report.scenarios[0].measurement.per_instruction;
        assert_eq!(per.len(), 1);
        assert_eq!(per[0].index, 0);
        assert_eq!(per[0].program_id, "User111");
        assert_eq!(per[0].consumed, Some(96_000));
    }

    #[test]
    fn end_to_end_warn_near_budget() {
        let report = Profiler::new().run(
            &backend(),
            &[swap_scenario(100_000)],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(report.scenarios[0].status, Status::Warn);
        assert!(
            report.scenarios[0]
                .diagnostics
                .iter()
                .any(|d| d.id == "near_budget_limit")
        );
    }

    /// A non-deterministic backend that returns a different CU figure per call,
    /// cycling through `cus` — used to exercise multi-sampling and variance.
    struct VaryingBackend {
        calls: std::cell::Cell<usize>,
        cus: Vec<u64>,
    }

    impl crate::backend::ExecutionBackend for VaryingBackend {
        fn kind(&self) -> crate::metadata::BackendKind {
            crate::metadata::BackendKind::Mollusk
        }
        fn run(&self, _scenario: &Scenario) -> crate::Result<crate::backend::SimulationOutput> {
            let i = self.calls.get();
            self.calls.set(i + 1);
            let cu = self.cus[i % self.cus.len()];
            Ok(crate::backend::SimulationOutput::success(vec![
                "Program P invoke [1]".to_string(),
                format!("Program P consumed {cu} of 200000 compute units"),
                "Program P success".to_string(),
            ]))
        }
    }

    #[test]
    fn multi_sample_records_variance_and_demotes_confidence() {
        let backend = VaryingBackend {
            calls: std::cell::Cell::new(0),
            cus: vec![100_000, 120_000, 110_000],
        };
        let mut s = swap_scenario(200_000);
        s.samples = 3;
        let report = Profiler::new().run(&backend, &[s], None, RunMetadata::recorded("0.1.0"));

        let stats = report.scenarios[0]
            .measurement
            .sample_stats
            .expect("multi-sample stats present");
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, 100_000);
        assert_eq!(stats.max, 120_000);
        assert!(stats.variance > 0.0);
        // ~7.4% CV demotes confidence below High with a variance reason.
        assert!(report.scenarios[0].confidence.level < confidence::ConfidenceLevel::High);
        assert!(
            report.scenarios[0]
                .confidence
                .reasons
                .iter()
                .any(|r| r.contains("variance"))
        );
    }

    #[test]
    fn deterministic_backend_ignores_samples() {
        // The recorded backend is deterministic: samples > 1 must not fabricate a
        // distribution — there is no real run-to-run spread to report.
        let mut s = swap_scenario(200_000);
        s.samples = 5;
        let report = Profiler::new().run(&backend(), &[s], None, RunMetadata::recorded("0.1.0"));
        assert!(report.scenarios[0].measurement.sample_stats.is_none());
    }

    #[test]
    fn missing_scenario_yields_unknown() {
        let report = Profiler::new().run(
            &RecordedLogsBackend::new(),
            &[Scenario::new("ghost")],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(report.scenarios[0].status, Status::Unknown);
        assert!(report.has_failures());
    }
}

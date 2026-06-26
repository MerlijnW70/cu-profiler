//! Stable, documented process exit codes (see `docs/ci.md`).

use cu_profiler_core::Error;
use cu_profiler_core::model::{Report, Status};

/// The documented exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    /// Everything passed.
    Success = 0,
    /// A budget or regression policy failed.
    BudgetOrRegression = 1,
    /// Configuration was invalid.
    Config = 2,
    /// A scenario could not be simulated.
    Simulation = 3,
    /// A baseline was stale or missing.
    Baseline = 4,
    /// A parser or report error occurred.
    ParserReport = 5,
    /// A measurement was low-confidence under strict mode.
    LowConfidence = 6,
}

impl ExitCode {
    /// The numeric code.
    #[must_use]
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Map a core [`Error`] to the exit code that best describes it.
#[must_use]
pub fn code_for_error(err: &Error) -> ExitCode {
    match err {
        Error::Config(_) | Error::Toml(_) => ExitCode::Config,
        Error::Simulation(_) | Error::BackendUnimplemented(_) => ExitCode::Simulation,
        Error::Baseline(_) => ExitCode::Baseline,
        Error::Parse { .. } => ExitCode::ParserReport,
        Error::Io(_) => ExitCode::ParserReport,
        // Any other (incl. feature-gated `Json`) maps to a parser/report error.
        _ => ExitCode::ParserReport,
    }
}

/// Decision flags governing which conditions fail the run.
#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionFlags {
    /// Fail on absolute budget breach.
    pub fail_on_budget: bool,
    /// Fail on regression.
    pub fail_on_regression: bool,
    /// Fail on stale baseline.
    pub fail_on_stale_baseline: bool,
    /// Fail on low confidence.
    pub fail_on_low_confidence: bool,
}

/// Determine the exit code for a completed [`Report`] under `flags`.
#[must_use]
pub fn code_for_report(report: &Report, flags: DecisionFlags) -> ExitCode {
    use cu_profiler_core::budget::PolicyStatus;
    use cu_profiler_core::confidence::ConfidenceLevel;

    // A scenario that could not be simulated outranks every soft signal.
    if report.scenarios.iter().any(|s| s.status == Status::Unknown) {
        return ExitCode::Simulation;
    }

    let mut stale = false;
    let mut low_conf = false;

    for s in &report.scenarios {
        for p in &s.policy_results {
            if p.status != PolicyStatus::Fail {
                continue;
            }
            if p.policy_id == "absolute_max_cu" && flags.fail_on_budget {
                return ExitCode::BudgetOrRegression;
            }
            if p.policy_id.starts_with("max_regression") && flags.fail_on_regression {
                return ExitCode::BudgetOrRegression;
            }
        }
        if let Some(c) = &s.baseline_comparison {
            if !c.matched {
                stale = true;
            }
        }
        if matches!(
            s.confidence.level,
            ConfidenceLevel::Low | ConfidenceLevel::Unknown
        ) {
            low_conf = true;
        }
    }

    if flags.fail_on_stale_baseline && stale {
        return ExitCode::Baseline;
    }
    if flags.fail_on_low_confidence && low_conf {
        return ExitCode::LowConfidence;
    }
    ExitCode::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::budget::BudgetPolicy;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    fn report_with(total: u64, max: u64) -> Report {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "s",
            &format!("Program P invoke [1]\nProgram P consumed {total} of 200000 compute units\nProgram P success"),
            true,
        );
        let mut scenario = Scenario::new("s");
        scenario.budget = BudgetPolicy {
            absolute_max_cu: Some(max),
            ..Default::default()
        };
        Profiler::new().run(&backend, &[scenario], None, RunMetadata::recorded("0.1.0"))
    }

    #[test]
    fn budget_failure_exits_one_when_enabled() {
        let report = report_with(120_000, 100_000);
        let flags = DecisionFlags {
            fail_on_budget: true,
            ..Default::default()
        };
        assert_eq!(
            code_for_report(&report, flags),
            ExitCode::BudgetOrRegression
        );
    }

    #[test]
    fn budget_failure_ignored_when_disabled() {
        let report = report_with(120_000, 100_000);
        assert_eq!(
            code_for_report(&report, DecisionFlags::default()),
            ExitCode::Success
        );
    }

    #[test]
    fn missing_simulation_exits_three() {
        let report = Profiler::new().run(
            &RecordedLogsBackend::new(),
            &[Scenario::new("ghost")],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(
            code_for_report(&report, DecisionFlags::default()),
            ExitCode::Simulation
        );
    }

    #[test]
    fn error_codes_map_each_category() {
        use cu_profiler_core::Error;
        assert_eq!(code_for_error(&Error::Config("x".into())), ExitCode::Config);
        assert_eq!(code_for_error(&Error::Toml("x".into())), ExitCode::Config);
        assert_eq!(
            code_for_error(&Error::Simulation("x".into())),
            ExitCode::Simulation
        );
        assert_eq!(
            code_for_error(&Error::BackendUnimplemented("x".into())),
            ExitCode::Simulation
        );
        assert_eq!(
            code_for_error(&Error::Baseline("x".into())),
            ExitCode::Baseline
        );
        // NOTE: `Error::Parse` and `Error::Io` both map to `ParserReport`, which is
        // also the catch-all `_` arm. Deleting either arm is an *equivalent mutant*
        // (no behavioural change), so it cannot — and need not — be killed.
        assert_eq!(
            code_for_error(&Error::Parse {
                what: "w".into(),
                index: 0,
                reason: "r".into()
            }),
            ExitCode::ParserReport
        );
    }

    /// A scenario run against a baseline whose fingerprint does NOT match: the
    /// comparison is present but stale (`matched == false`), which also lowers
    /// confidence to Low. One report exercising both soft signals at once.
    fn report_with_stale_baseline() -> Report {
        use cu_profiler_core::baseline::{BaselineRecord, BaselineStore, Fingerprint};
        use cu_profiler_core::confidence::ConfidenceLevel;
        use cu_profiler_core::metadata::InstrumentationMode;

        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "s",
            "Program P invoke [1]\nProgram P consumed 50000 of 200000 compute units\nProgram P success",
            true,
        );
        let mut store = BaselineStore::new();
        store.insert(BaselineRecord {
            scenario: "s".into(),
            actual_units: 50_000,
            budget: None,
            timestamp: None,
            git_commit: None,
            fingerprint: Fingerprint::new("OTHER", "OTHER", "OTHER", None),
            solana_versions: Vec::new(),
            profiler_version: "0.1.0".into(),
            instrumentation: InstrumentationMode::Off,
            confidence: ConfidenceLevel::High,
            approved: false,
        });
        Profiler::new().run(
            &backend,
            &[Scenario::new("s")],
            Some(&store),
            RunMetadata::recorded("0.1.0"),
        )
    }

    #[test]
    fn soft_signals_do_not_fail_when_their_flags_are_off() {
        // stale baseline + low confidence are both present, but with no flags set
        // neither may change the exit code. Guards the `&&` in both decisions.
        let report = report_with_stale_baseline();
        assert_eq!(
            code_for_report(&report, DecisionFlags::default()),
            ExitCode::Success
        );
    }

    #[test]
    fn stale_baseline_exits_four_when_enabled() {
        let report = report_with_stale_baseline();
        let flags = DecisionFlags {
            fail_on_stale_baseline: true,
            ..Default::default()
        };
        assert_eq!(code_for_report(&report, flags), ExitCode::Baseline);
    }

    #[test]
    fn low_confidence_exits_six_when_enabled() {
        let report = report_with_stale_baseline();
        let flags = DecisionFlags {
            fail_on_low_confidence: true,
            ..Default::default()
        };
        assert_eq!(code_for_report(&report, flags), ExitCode::LowConfidence);
    }

    #[test]
    fn regression_ignored_when_flag_off() {
        // A matched baseline with a real regression, but `fail_on_regression` is off:
        // the regression branch must not fire. Guards its `&&`.
        use cu_profiler_core::baseline::{BaselineRecord, BaselineStore};
        use cu_profiler_core::confidence::ConfidenceLevel;
        use cu_profiler_core::metadata::InstrumentationMode;

        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "s",
            "Program P invoke [1]\nProgram P consumed 110000 of 200000 compute units\nProgram P success",
            true,
        );
        let mut scenario = Scenario::new("s");
        scenario.budget = BudgetPolicy {
            max_regression_pct: Some(5.0),
            ..Default::default()
        };
        let profiler = Profiler::new();
        let fingerprint = profiler.fingerprint(&scenario);
        let mut store = BaselineStore::new();
        store.insert(BaselineRecord {
            scenario: "s".into(),
            actual_units: 90_000, // +22% vs 110k → regression FAIL
            budget: None,
            timestamp: None,
            git_commit: None,
            fingerprint, // matches current → comparison is non-stale, regression evaluated
            solana_versions: Vec::new(),
            profiler_version: "0.1.0".into(),
            instrumentation: InstrumentationMode::Off,
            confidence: ConfidenceLevel::High,
            approved: false,
        });
        let report = profiler.run(
            &backend,
            &[scenario],
            Some(&store),
            RunMetadata::recorded("0.1.0"),
        );
        assert_eq!(
            code_for_report(&report, DecisionFlags::default()),
            ExitCode::Success
        );
        // Sanity: with the flag on, the same regression *does* fail.
        let strict = DecisionFlags {
            fail_on_regression: true,
            ..Default::default()
        };
        assert_eq!(
            code_for_report(&report, strict),
            ExitCode::BudgetOrRegression
        );
    }
}

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
}

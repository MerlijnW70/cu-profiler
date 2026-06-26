//! `cu-profiler run` — run scenarios and render a report.

use cu_profiler_core::Result;

use crate::args::RunArgs;
use crate::commands::{
    emit, load_config, profile, resolve_format, warn_if_demo, warn_if_live_mode,
};
use crate::exit::{DecisionFlags, ExitCode, code_for_report};

/// Execute the `run` command.
pub fn run(args: &RunArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, scenarios, _baseline) = profile(&loaded, &args.common, args.baseline.as_deref())?;

    warn_if_live_mode(&loaded.config, quiet);
    warn_if_demo(&scenarios, &args.common.logs_dir, quiet);

    let format = resolve_format(&loaded.config, args.format.as_deref())?;
    let rendered = cu_profiler_report::render(&report, format)?;
    emit(&rendered, args.output.as_deref(), quiet)?;

    let flags = DecisionFlags {
        fail_on_budget: args.fail_on_budget || loaded.config.defaults.fail_on_budget,
        fail_on_regression: args.fail_on_regression || loaded.config.defaults.fail_on_regression,
        fail_on_stale_baseline: loaded.config.defaults.fail_on_stale_baseline,
        fail_on_low_confidence: args.fail_on_low_confidence || args.strict,
    };
    Ok(code_for_report(&report, flags))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CommonRun;
    use crate::exit::ExitCode;
    use std::path::PathBuf;

    /// Scaffold a temp project (config + one scenario log) and a `RunArgs` pointing
    /// at it. Returns the base dir (for cleanup) and the args.
    fn temp_project(config_toml: &str, scenario: &str, log: &str) -> (PathBuf, RunArgs) {
        let base = std::env::temp_dir().join(format!("cu-run-{}-{scenario}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let logs = base.join(".cu").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let config = base.join("cu-profiler.toml");
        std::fs::write(&config, config_toml).unwrap();
        std::fs::write(logs.join(format!("{scenario}.log")), log).unwrap();
        let args = RunArgs {
            common: CommonRun {
                config,
                logs_dir: logs,
                scenarios: vec![],
                tags: vec![],
                samples: None,
            },
            format: None,
            output: None,
            baseline: None,
            strict: false,
            fail_on_budget: false,
            fail_on_regression: false,
            fail_on_low_confidence: false,
        };
        (base, args)
    }

    #[test]
    fn config_default_enforces_budget_without_a_cli_flag() {
        // `fail_on_budget` comes only from the config default; the over-budget run
        // must still fail. Guards the `||` between the CLI flag and the config.
        let config = "[project]\nname=\"t\"\n[defaults]\nfail_on_budget=true\n\
                      [scenario.over]\nbudget=100000\n";
        let log = "Program P invoke [1]\n\
                   Program P consumed 120000 of 200000 compute units\nProgram P success";
        let (base, args) = temp_project(config, "over", log);
        let code = run(&args, true);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(code.unwrap(), ExitCode::BudgetOrRegression);
    }

    #[test]
    fn strict_flag_fails_a_low_confidence_run() {
        // An unbalanced scope marker yields a parser warning → low confidence. With
        // `--strict` that must fail. Guards the `||` between the flag and `strict`.
        let config = "[project]\nname=\"t\"\n[scenario.shaky]\n";
        let log = "Program P invoke [1]\nProgram log: CU_PROFILER_BEGIN name=x\n\
                   Program P consumed 1000 of 200000 compute units\nProgram P success";
        let (base, mut args) = temp_project(config, "shaky", log);
        args.strict = true;
        let code = run(&args, true);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(code.unwrap(), ExitCode::LowConfidence);
    }

    #[test]
    fn config_default_enforces_regression_without_a_cli_flag() {
        // Save a baseline, then re-run against a higher-consuming log. The config
        // default (not a CLI flag) must enforce the regression. Guards the `||`
        // between the CLI flag and the config for the regression decision.
        use crate::args::BaselineSaveArgs;

        let config = "[project]\nname=\"t\"\n[defaults]\n\
                      fail_on_regression=true\nmax_regression_pct=5\n[scenario.s]\n";
        let (base, mut args) = temp_project(
            config,
            "s",
            "Program P invoke [1]\nProgram P consumed 90000 of 200000 compute units\nProgram P success",
        );
        let baseline = base.join("baseline.json");

        // 1) Record the baseline at 90k CU.
        let save_args = BaselineSaveArgs {
            common: args.common.clone(),
            baseline: baseline.clone(),
        };
        crate::commands::baseline_save(&save_args, true).expect("baseline save");

        // 2) Bump the measured CU to 110k (+22% > 5% allowance).
        std::fs::write(
            args.common.logs_dir.join("s.log"),
            "Program P invoke [1]\nProgram P consumed 110000 of 200000 compute units\nProgram P success",
        )
        .unwrap();

        // 3) Re-run with the baseline; fail_on_regression comes only from the config.
        args.baseline = Some(baseline);
        let code = run(&args, true);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(code.unwrap(), ExitCode::BudgetOrRegression);
    }
}

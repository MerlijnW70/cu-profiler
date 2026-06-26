//! `cu-profiler ci` — deterministic CI mode that writes artifacts and returns a
//! stable exit code.

use cu_profiler_core::Result;
use cu_profiler_report::Format;

use crate::args::RunArgs;
use crate::commands::{load_config, profile, warn_if_demo, warn_if_live_mode};
use crate::exit::{DecisionFlags, ExitCode, code_for_report};

/// Execute the `ci` command.
pub fn run(args: &RunArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, scenarios, _baseline) = profile(&loaded, &args.common, args.baseline.as_deref())?;

    warn_if_live_mode(&loaded.config, quiet);
    warn_if_demo(&scenarios, &args.common.logs_dir, quiet);

    // Write every artifact the config asks for; CI consumers pick what they want.
    let out = &loaded.config.output;
    if let Some(path) = &out.json_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Json)?)?;
    }
    if let Some(path) = &out.markdown_path {
        write_artifact(
            path,
            &cu_profiler_report::render(&report, Format::Markdown)?,
        )?;
    }
    if let Some(path) = &out.junit_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Junit)?)?;
    }
    if let Some(path) = &out.html_path {
        write_artifact(path, &cu_profiler_report::render(&report, Format::Html)?)?;
    }

    // Deterministic table summary to stdout.
    if !quiet {
        print!("{}", cu_profiler_report::render(&report, Format::Table)?);
    }

    // In CI, enforce by default; explicit flags can still tighten.
    let flags = DecisionFlags {
        fail_on_budget: args.fail_on_budget || loaded.config.defaults.fail_on_budget,
        fail_on_regression: args.fail_on_regression || loaded.config.defaults.fail_on_regression,
        fail_on_stale_baseline: loaded.config.defaults.fail_on_stale_baseline,
        fail_on_low_confidence: args.fail_on_low_confidence || args.strict,
    };
    Ok(code_for_report(&report, flags))
}

fn write_artifact(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::CommonRun;
    use std::path::PathBuf;

    fn temp_project(config_toml: &str, scenario: &str, log: &str) -> (PathBuf, RunArgs) {
        let base = std::env::temp_dir().join(format!("cu-ci-{}-{scenario}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let logs = base.join(".cu").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(base.join("cu-profiler.toml"), config_toml).unwrap();
        std::fs::write(logs.join(format!("{scenario}.log")), log).unwrap();
        let args = RunArgs {
            common: CommonRun {
                config: base.join("cu-profiler.toml"),
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
    fn ci_config_default_enforces_budget() {
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
    fn ci_strict_fails_low_confidence() {
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
    fn ci_config_default_enforces_regression() {
        // Record a baseline, then re-run a higher-consuming log. `fail_on_regression`
        // comes only from the config default (no CLI flag), so the `||` between the
        // flag and the config must hold — an `||`→`&&` flip would mask the regression.
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

        // 3) Re-run via `ci`; the regression must fail from the config default alone.
        args.baseline = Some(baseline);
        let code = run(&args, true);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(code.unwrap(), ExitCode::BudgetOrRegression);
    }

    #[test]
    fn ci_writes_configured_artifacts_into_nested_dirs() {
        // A configured json_path with a not-yet-existing parent must be written.
        // Guards both `write_artifact` and its parent-creation branch. Uses an
        // absolute path (forward slashes are valid TOML everywhere) so the test
        // never touches the process CWD — safe under parallel test execution.
        let base = std::env::temp_dir().join(format!("cu-ci-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let logs = base.join(".cu").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let json = base.join("artifacts").join("report.json"); // nested, absent
        let json_toml = json.to_string_lossy().replace('\\', "/");
        let config = format!(
            "[project]\nname=\"t\"\n\
             [output]\ndefault_format=\"table\"\njson_path=\"{json_toml}\"\n\
             [scenario.s]\n"
        );
        std::fs::write(base.join("cu-profiler.toml"), &config).unwrap();
        std::fs::write(
            logs.join("s.log"),
            "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
        )
        .unwrap();
        let args = RunArgs {
            common: CommonRun {
                config: base.join("cu-profiler.toml"),
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
        let code = run(&args, true);
        let exists = json.exists();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(code.unwrap(), ExitCode::Success);
        assert!(exists, "ci did not write the configured json artifact");
    }
}

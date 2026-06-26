//! `cu-profiler baseline save` / `approve`.

use cu_profiler_core::Result;
use cu_profiler_core::baseline::{BaselineRecord, BaselineStore};
use cu_profiler_core::error::Error;
use cu_profiler_core::metadata::InstrumentationMode;
use cu_profiler_core::model::Status;
use cu_profiler_core::profiler::Profiler;

use crate::args::{BaselineApproveArgs, BaselineSaveArgs};
use crate::commands::load_config;
use crate::exit::ExitCode;

/// Execute `baseline save`.
pub fn save(args: &BaselineSaveArgs, quiet: bool) -> Result<ExitCode> {
    let loaded = load_config(&args.common.config)?;
    let (report, scenarios, _) = super::profile(&loaded, &args.common, None)?;

    let fingerprinter = Profiler::new().with_config_repr(loaded.config_text.clone());
    let mut store = BaselineStore::load(&args.baseline)?;

    let mut saved = 0;
    for (scenario, sr) in scenarios.iter().zip(&report.scenarios) {
        if sr.status == Status::Unknown {
            continue; // never record a scenario we couldn't simulate
        }
        store.insert(BaselineRecord {
            scenario: scenario.name.clone(),
            actual_units: sr.measurement.total_cu,
            budget: scenario.budget.absolute_max_cu,
            timestamp: None,
            git_commit: None,
            fingerprint: fingerprinter.fingerprint(scenario),
            solana_versions: Vec::new(),
            profiler_version: cu_profiler_core::VERSION.to_string(),
            instrumentation: InstrumentationMode::Off,
            confidence: sr.confidence.level,
            approved: false,
        });
        saved += 1;
    }

    store.save(&args.baseline)?;
    if !quiet {
        println!(
            "saved {saved} baseline record(s) to {}",
            args.baseline.display()
        );
    }
    Ok(ExitCode::Success)
}

/// Execute `baseline approve`.
pub fn approve(args: &BaselineApproveArgs, quiet: bool) -> Result<ExitCode> {
    let mut store = BaselineStore::load(&args.baseline)?;
    if store.approve(&args.scenario) {
        store.save(&args.baseline)?;
        if !quiet {
            println!("approved baseline for `{}`", args.scenario);
        }
        Ok(ExitCode::Success)
    } else {
        Err(Error::Baseline(format!(
            "no baseline record for scenario `{}` in {}",
            args.scenario,
            args.baseline.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{BaselineSaveArgs, CommonRun};

    #[test]
    fn save_records_simulated_scenarios_and_skips_unsimulated() {
        let base = std::env::temp_dir().join(format!("cu-bl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let logs = base.join(".cu").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        // Two scenarios; only `good` has a log, so `missing` simulates to Unknown
        // and must NOT be written to the baseline.
        std::fs::write(
            base.join("cu-profiler.toml"),
            "[project]\nname=\"t\"\n[scenario.good]\n[scenario.missing]\n",
        )
        .unwrap();
        std::fs::write(
            logs.join("good.log"),
            "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
        )
        .unwrap();
        let baseline = base.join("baseline.json");
        let args = BaselineSaveArgs {
            common: CommonRun {
                config: base.join("cu-profiler.toml"),
                logs_dir: logs,
                scenarios: vec![],
                tags: vec![],
                samples: None,
            },
            baseline: baseline.clone(),
        };
        save(&args, true).expect("baseline save");
        let store = BaselineStore::load(&baseline).unwrap();
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            store.get("good").is_some(),
            "simulated scenario must be recorded"
        );
        assert!(
            store.get("missing").is_none(),
            "unsimulated (Unknown) scenario must be skipped"
        );
    }
}

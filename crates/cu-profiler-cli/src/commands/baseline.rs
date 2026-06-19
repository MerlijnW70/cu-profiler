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

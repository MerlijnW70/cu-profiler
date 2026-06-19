//! Live `mollusk-svm` execution backend for cu-profiler — **real** compute-unit
//! metering.
//!
//! Unlike recorded logs or the in-process `program-test` native path, Mollusk
//! runs a compiled **SBF** program through the SVM and reports the actual
//! `compute_units_consumed`. Mollusk does not surface program log messages, so
//! this backend translates that real CU number into the canonical Solana log
//! lines the `cu-profiler-core` parser already understands
//! (`Program <id> consumed <cu> of <budget> compute units`). The rest of the
//! pipeline — budgets, baselines, diagnostics, reports — is then identical to a
//! recorded run, but the number is genuinely metered.
//!
//! This is a **workspace-detached** crate: the Mollusk/Solana stack is heavy and
//! `openssl-sys` does not build on Windows, so the core crates and the local
//! quality gate stay Solana-free. CI builds an SBF program with `cargo build-sbf`
//! and runs this crate's tests on Linux.

use std::collections::HashMap;

use cu_profiler_core::Result;
use cu_profiler_core::backend::{ExecutionBackend, SimulationOutput};
use cu_profiler_core::error::Error;
use cu_profiler_core::metadata::BackendKind;
use cu_profiler_core::scenario::Scenario;

use mollusk_svm::Mollusk;
use solana_account::Account;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

/// Compute-unit budget used in the synthesized `consumed … of <budget>` line.
/// The headline figure (`total_cu`) is Mollusk's real measurement; the budget
/// only seeds the parser's requested-limit estimate.
const DEFAULT_BUDGET: u64 = 200_000;

/// Everything needed to execute one scenario through Mollusk: a configured
/// [`Mollusk`] harness (its SBF program already loaded) and the instruction to
/// run, with any accounts it touches.
pub struct ScenarioSetup {
    /// The Mollusk harness with the program under test loaded.
    pub mollusk: Mollusk,
    /// The instruction to execute.
    pub instruction: Instruction,
    /// Accounts the instruction reads/writes (empty for a no-account program).
    pub accounts: Vec<(Pubkey, Account)>,
}

impl ScenarioSetup {
    /// Convenience constructor for the no-account case.
    #[must_use]
    pub fn new(mollusk: Mollusk, instruction: Instruction) -> Self {
        Self {
            mollusk,
            instruction,
            accounts: Vec::new(),
        }
    }
}

/// A factory that builds a fresh [`ScenarioSetup`] per run.
pub type SetupFn = Box<dyn Fn() -> ScenarioSetup + Send + Sync>;

/// Execution backend that meters real compute units via `mollusk-svm`.
#[derive(Default)]
pub struct MolluskBackend {
    setups: HashMap<String, SetupFn>,
}

impl MolluskBackend {
    /// An empty backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the setup factory for a scenario name.
    pub fn register<F>(&mut self, scenario: impl Into<String>, setup: F)
    where
        F: Fn() -> ScenarioSetup + Send + Sync + 'static,
    {
        self.setups.insert(scenario.into(), Box::new(setup));
    }
}

impl ExecutionBackend for MolluskBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Mollusk
    }

    fn run(&self, scenario: &Scenario) -> Result<SimulationOutput> {
        let setup = self.setups.get(&scenario.name).ok_or_else(|| {
            Error::Simulation(format!(
                "no mollusk setup registered for scenario `{}`",
                scenario.name
            ))
        })?;
        let setup = setup();

        let result = setup
            .mollusk
            .process_instruction(&setup.instruction, &setup.accounts);

        let success = result.raw_result.is_ok();
        let cu = result.compute_units_consumed;
        let pid = setup.instruction.program_id;

        // Mollusk returns a structured CU figure but no log vector, so we emit
        // the canonical lines the parser reads, carrying the real measurement.
        let status = if success { "success" } else { "failed" };
        let logs = vec![
            format!("Program {pid} invoke [1]"),
            format!("Program {pid} consumed {cu} of {DEFAULT_BUDGET} compute units"),
            format!("Program {pid} {status}"),
        ];

        Ok(SimulationOutput { logs, success })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::parser::analyze;
    use cu_profiler_core::program_registry::ProgramRegistry;

    fn demo_backend() -> (MolluskBackend, Pubkey) {
        let program_id = Pubkey::new_unique();
        let mut backend = MolluskBackend::new();
        backend.register("demo", move || {
            // Loads `cu_profiler_demo_program.so` from SBF_OUT_DIR / tests/fixtures.
            let mollusk = Mollusk::new(&program_id, "cu_profiler_demo_program");
            let ix = Instruction::new_with_bytes(program_id, &[], Vec::new());
            ScenarioSetup::new(mollusk, ix)
        });
        (backend, program_id)
    }

    #[test]
    fn mollusk_meters_real_compute_units() {
        let (backend, _id) = demo_backend();
        let out = backend.run(&Scenario::new("demo")).expect("scenario runs");
        assert!(out.success, "transaction should succeed: {:?}", out.logs);

        let analysis = analyze(&out.logs, &ProgramRegistry::with_builtins());
        // The headline number is genuinely metered by Mollusk (not zero, not faked).
        assert!(
            analysis.total_cu > 0,
            "expected real metered CU, got: {:?}",
            out.logs
        );
        assert!(analysis.simulation_success);
    }

    #[test]
    fn unregistered_scenario_errors() {
        let backend = MolluskBackend::new();
        let err = backend.run(&Scenario::new("missing")).unwrap_err();
        assert!(err.to_string().contains("no mollusk setup"));
    }
}

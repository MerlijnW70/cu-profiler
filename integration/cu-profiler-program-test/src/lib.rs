//! Live `solana-program-test` execution backend for cu-profiler.
//!
//! This crate turns a scenario into **real** Solana logs by executing a
//! transaction in `solana-program-test`'s in-process runtime and capturing the
//! transaction metadata's `log_messages`. Those logs feed the same
//! `cu-profiler-core` parser the recorded backend uses, so a live run and a
//! replayed run produce the same shape of [`cu_profiler_core::model::Report`].
//!
//! It is deliberately a **separate, workspace-detached crate**: the Solana stack
//! is heavy and `openssl-sys` does not build on Windows, so the core crates and
//! the local quality gate stay Solana-free. CI builds this crate in a dedicated
//! Linux job.
//!
//! # Compute metering: SBF vs native
//! The Solana runtime only **meters compute units for SBF programs** (compiled
//! `.so` artifacts run in the SBF VM). An in-process *native* program registered
//! via `processor!` executes, but the runtime emits no
//! `consumed … compute units` line and does not route the program's `msg!`
//! output into the transaction metadata — only the `invoke`/`success` wrapper
//! lines are captured. This backend faithfully captures whatever the runtime
//! emits: against an SBF `.so` that includes the consumed-CU lines the parser
//! reads; against a native `processor!` it does not. Register an SBF program
//! (e.g. `ProgramTest::new(name, id, None)` loading `tests/fixtures/<name>.so`)
//! for real CU measurement.
//!
//! # How a scenario is executed
//! Because [`cu_profiler_core::scenario::Scenario`] carries no executable, the
//! backend holds a per-scenario [`SetupFn`] that returns a [`ScenarioSetup`]
//! (the configured [`ProgramTest`] plus the instructions to run) — the same
//! "register by name" shape as the recorded backend.

use std::collections::HashMap;

use cu_profiler_core::backend::{ExecutionBackend, SimulationOutput};
use cu_profiler_core::error::Error;
use cu_profiler_core::metadata::BackendKind;
use cu_profiler_core::scenario::Scenario;
use cu_profiler_core::Result;

use solana_program_test::ProgramTest;
use solana_sdk::instruction::Instruction;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;

/// Everything needed to execute one scenario: a configured `ProgramTest` (with
/// its programs registered) and the instructions to submit. Extra signers beyond
/// the funded payer are listed in `signers`.
pub struct ScenarioSetup {
    /// The configured program-test harness (programs already registered).
    pub program_test: ProgramTest,
    /// Instructions to submit as a single transaction.
    pub instructions: Vec<Instruction>,
    /// Additional signers required by the instructions (besides the payer).
    pub signers: Vec<Keypair>,
}

impl ScenarioSetup {
    /// Convenience constructor for the common "one program, some instructions,
    /// payer-only signing" case.
    #[must_use]
    pub fn new(program_test: ProgramTest, instructions: Vec<Instruction>) -> Self {
        Self {
            program_test,
            instructions,
            signers: Vec::new(),
        }
    }
}

/// A factory that builds a fresh [`ScenarioSetup`] for each run.
pub type SetupFn = Box<dyn Fn() -> ScenarioSetup + Send + Sync>;

/// Execution backend that runs scenarios in `solana-program-test`.
#[derive(Default)]
pub struct ProgramTestBackend {
    setups: HashMap<String, SetupFn>,
}

impl ProgramTestBackend {
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

impl ExecutionBackend for ProgramTestBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::ProgramTest
    }

    fn run(&self, scenario: &Scenario) -> Result<SimulationOutput> {
        let setup = self.setups.get(&scenario.name).ok_or_else(|| {
            Error::Simulation(format!(
                "no program-test setup registered for scenario `{}`",
                scenario.name
            ))
        })?;
        let setup = setup();

        // BanksClient is async; drive it on a private multi-thread runtime so
        // the synchronous `ExecutionBackend::run` contract is preserved.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Simulation(format!("failed to start tokio runtime: {e}")))?;
        runtime.block_on(execute(setup))
    }
}

async fn execute(setup: ScenarioSetup) -> Result<SimulationOutput> {
    let ScenarioSetup {
        program_test,
        instructions,
        signers,
    } = setup;

    let (banks, payer, blockhash) = program_test.start().await;

    let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
    let mut all_signers: Vec<&Keypair> = Vec::with_capacity(signers.len() + 1);
    all_signers.push(&payer);
    all_signers.extend(signers.iter());
    tx.sign(&all_signers, blockhash);

    let result = banks
        .process_transaction_with_metadata(tx)
        .await
        .map_err(|e| Error::Simulation(format!("banks transaction failed: {e}")))?;

    let success = result.result.is_ok();
    let logs = result
        .metadata
        .map(|m| m.log_messages)
        .unwrap_or_default();

    Ok(SimulationOutput { logs, success })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::parser::analyze;
    use cu_profiler_core::program_registry::ProgramRegistry;

    use solana_program_test::processor;
    use solana_sdk::account_info::AccountInfo;
    use solana_sdk::entrypoint::ProgramResult;
    use solana_sdk::msg;
    use solana_sdk::pubkey::Pubkey;

    /// A tiny in-process program: marks a scope, does some work, logs.
    fn process_instruction(
        _program_id: &Pubkey,
        _accounts: &[AccountInfo],
        _data: &[u8],
    ) -> ProgramResult {
        msg!("CU_PROFILER_BEGIN name=demo::work");
        let mut acc: u64 = 0;
        for i in 0..2000u64 {
            acc = acc.wrapping_add(i.wrapping_mul(7));
        }
        msg!("work result {}", acc);
        msg!("CU_PROFILER_END name=demo::work");
        Ok(())
    }

    fn backend_with_demo() -> (ProgramTestBackend, Pubkey) {
        let program_id = Pubkey::new_unique();
        let mut backend = ProgramTestBackend::new();
        backend.register("demo", move || {
            let pt = ProgramTest::new(
                "cu_profiler_demo",
                program_id,
                processor!(process_instruction),
            );
            let ix = Instruction::new_with_bytes(program_id, &[], Vec::new());
            ScenarioSetup::new(pt, vec![ix])
        });
        (backend, program_id)
    }

    #[test]
    fn runs_in_process_program_and_captures_runtime_logs() {
        let (backend, id) = backend_with_demo();
        let out = backend.run(&Scenario::new("demo")).expect("scenario runs");
        // A native `processor!` program is metered differently (see module docs),
        // so we assert the reliable truths: the tx succeeded and the runtime's
        // invoke/success logs for our program were captured.
        assert!(out.success, "transaction should succeed: {:?}", out.logs);
        assert!(!out.logs.is_empty(), "expected captured logs");
        let id = id.to_string();
        assert!(
            out.logs.iter().any(|l| l.contains(&id)),
            "expected our program id in the logs, got: {:?}",
            out.logs
        );
    }

    #[test]
    fn parser_reconstructs_call_tree_from_real_run() {
        let (backend, id) = backend_with_demo();
        let out = backend.run(&Scenario::new("demo")).unwrap();
        // The parser consumes *real* runtime logs end-to-end.
        let analysis = analyze(&out.logs, &ProgramRegistry::with_builtins());
        assert!(analysis.simulation_success);
        let id = id.to_string();
        assert!(
            analysis
                .call_tree
                .children
                .iter()
                .any(|n| n.program_id == id),
            "call tree should contain our program"
        );
    }

    #[test]
    fn unregistered_scenario_errors() {
        let backend = ProgramTestBackend::new();
        let err = backend.run(&Scenario::new("missing")).unwrap_err();
        assert!(err.to_string().contains("no program-test setup"));
    }
}

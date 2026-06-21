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

use cu_profiler_core::backend::{ExecutionBackend, SimulationOutput};
use cu_profiler_core::bench::{BenchPlan, InstructionFixture};
use cu_profiler_core::error::Error;
use cu_profiler_core::metadata::{BackendKind, InstrumentationMode, RunMetadata};
use cu_profiler_core::model::Report;
use cu_profiler_core::scenario::Scenario;
use cu_profiler_core::{Profiler, Result};

use mollusk_svm::Mollusk;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
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

    /// Build a backend from a validated [`BenchPlan`], loading the SBF program
    /// `program_name` (the `.so` stem, located from `SBF_OUT_DIR` / `target/deploy`)
    /// for every instruction. Each [`InstructionFixture`] becomes one scenario setup:
    /// its program id, hex data, and accounts are parsed once here (so malformed
    /// fixtures fail fast), then a fresh `Mollusk` harness + `Instruction` is built
    /// per run.
    ///
    /// This is the turnkey real-CU path: a declarative `bench.toml` in, real metered
    /// compute units out, with no hand-written harness.
    ///
    /// # Errors
    /// Returns [`Error::Config`] for a non-base58 address or non-hex data in the plan.
    pub fn from_plan(plan: &BenchPlan, program_name: &str) -> Result<Self> {
        let mut backend = Self::new();
        for fixture in &plan.instructions {
            let prepared = PreparedInstruction::from_fixture(fixture)?;
            let name = program_name.to_string();
            backend.register(fixture.scenario.clone(), move || prepared.setup(&name));
        }
        Ok(backend)
    }
}

/// A [`InstructionFixture`] parsed into ready-to-run Solana types, so parsing
/// happens once (with error handling) rather than per run inside the setup closure.
#[derive(Clone)]
struct PreparedInstruction {
    program_id: Pubkey,
    data: Vec<u8>,
    metas: Vec<AccountMeta>,
    accounts: Vec<(Pubkey, Account)>,
}

impl PreparedInstruction {
    fn from_fixture(fixture: &InstructionFixture) -> Result<Self> {
        let program_id = parse_pubkey(&fixture.program_id, "program_id")?;
        let data = decode_hex(&fixture.data, "instruction data")?;

        let mut metas = Vec::with_capacity(fixture.accounts.len());
        let mut accounts = Vec::with_capacity(fixture.accounts.len());
        for acc in &fixture.accounts {
            let pubkey = parse_pubkey(&acc.pubkey, "account pubkey")?;
            metas.push(AccountMeta {
                pubkey,
                is_signer: acc.signer,
                is_writable: acc.writable,
            });
            let owner = match &acc.owner {
                Some(o) => parse_pubkey(o, "account owner")?,
                None => Pubkey::default(),
            };
            let account_data = match &acc.data {
                Some(d) => decode_hex(d, "account data")?,
                None => Vec::new(),
            };
            accounts.push((
                pubkey,
                Account {
                    lamports: acc.lamports,
                    data: account_data,
                    owner,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }
        Ok(Self {
            program_id,
            data,
            metas,
            accounts,
        })
    }

    fn setup(&self, program_name: &str) -> ScenarioSetup {
        let mollusk = Mollusk::new(&self.program_id, program_name);
        let instruction =
            Instruction::new_with_bytes(self.program_id, &self.data, self.metas.clone());
        ScenarioSetup {
            mollusk,
            instruction,
            accounts: self.accounts.clone(),
        }
    }
}

/// Run a whole [`BenchPlan`] end-to-end and assemble a [`Report`]: build a
/// [`MolluskBackend`] from the plan (loading `program_name`), profile one scenario
/// per instruction, and meter real compute units. This is the one-call turnkey API
/// behind the `cu-profiler-bench` binary.
///
/// # Errors
/// Returns [`Error::Config`] if the plan is malformed (bad address or hex).
pub fn run_plan(plan: &BenchPlan, program_name: &str) -> Result<Report> {
    let backend = MolluskBackend::from_plan(plan, program_name)?;
    let scenarios: Vec<Scenario> = plan
        .instructions
        .iter()
        .map(|ix| Scenario::new(&ix.scenario))
        .collect();
    let metadata = RunMetadata {
        profiler_version: cu_profiler_core::VERSION.to_string(),
        backend: BackendKind::Mollusk,
        instrumentation: InstrumentationMode::Off,
        git_commit: None,
        solana_versions: Vec::new(),
        generated_at: None,
    };
    Ok(Profiler::new().run(&backend, &scenarios, None, metadata))
}

/// Parse a base58 Solana address, mapping failure to a clear config error.
fn parse_pubkey(s: &str, what: &str) -> Result<Pubkey> {
    s.parse::<Pubkey>()
        .map_err(|e| Error::Config(format!("{what} `{s}` is not a valid address: {e}")))
}

/// Decode a hex string into bytes (empty string → empty vec).
fn decode_hex(s: &str, what: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(Error::Config(format!("{what}: hex has odd length")));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| Error::Config(format!("{what}: invalid hex: {e}")))
        })
        .collect()
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

    #[test]
    fn decode_hex_roundtrips_and_rejects_bad_input() {
        assert_eq!(decode_hex("", "x").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_hex("01ab", "x").unwrap(), vec![0x01, 0xab]);
        assert!(decode_hex("abc", "x").is_err()); // odd length
        assert!(decode_hex("zz", "x").is_err()); // non-hex
    }

    #[test]
    fn from_plan_parses_a_fixture_into_a_registered_setup() {
        // Build a plan whose program id is a real (valid base58) pubkey, but do not
        // run it — this exercises the parse/convert path without needing the .so.
        let program_id = Pubkey::new_unique();
        let toml = format!(
            "[[instruction]]\nscenario=\"swap\"\nprogram_id=\"{program_id}\"\ndata=\"01ff\"\n"
        );
        let plan = BenchPlan::from_toml(&toml).expect("valid plan");
        let backend =
            MolluskBackend::from_plan(&plan, "cu_profiler_demo_program").expect("plan converts");
        assert!(backend.setups.contains_key("swap"));
    }

    #[test]
    fn run_plan_meters_the_demo_into_a_report() {
        let program_id = Pubkey::new_unique();
        let toml = format!("[[instruction]]\nscenario=\"demo\"\nprogram_id=\"{program_id}\"\n");
        let plan = BenchPlan::from_toml(&toml).expect("valid plan");
        let report = run_plan(&plan, "cu_profiler_demo_program").expect("plan runs");
        assert_eq!(report.scenarios.len(), 1);
        assert_eq!(report.metadata.backend, BackendKind::Mollusk);
        assert!(report.scenarios[0].measurement.total_cu > 0);
    }

    #[test]
    fn from_plan_runs_the_demo_and_meters_real_cu() {
        // End-to-end: a declarative plan, loaded against the demo .so, yields real CU.
        let program_id = Pubkey::new_unique();
        let toml = format!("[[instruction]]\nscenario=\"demo\"\nprogram_id=\"{program_id}\"\n");
        let plan = BenchPlan::from_toml(&toml).expect("valid plan");
        let backend =
            MolluskBackend::from_plan(&plan, "cu_profiler_demo_program").expect("plan converts");

        let out = backend.run(&Scenario::new("demo")).expect("scenario runs");
        let analysis = analyze(&out.logs, &ProgramRegistry::with_builtins());
        assert!(out.success, "demo should succeed: {:?}", out.logs);
        assert!(
            analysis.total_cu > 0,
            "expected real metered CU: {:?}",
            out.logs
        );
    }
}

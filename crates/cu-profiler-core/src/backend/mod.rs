//! Execution backends.
//!
//! A backend turns a [`Scenario`] into raw logs. v1 ships a fully-working
//! [`RecordedLogsBackend`] (deterministic, used for tests, fixtures and the
//! `inspect` command) and *skeleton* live backends whose interfaces are defined
//! but which return [`crate::Error::BackendUnimplemented`] until wired to
//! `solana-program-test` / `BanksClient`.

mod banks_client;
mod program_test;
mod recorded;

pub use banks_client::BanksClientBackend;
pub use program_test::ProgramTestBackend;
pub use recorded::RecordedLogsBackend;

use crate::Result;
use crate::metadata::BackendKind;
use crate::scenario::Scenario;

/// The raw result of executing a scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationOutput {
    /// Program logs, one entry per line.
    pub logs: Vec<String>,
    /// Whether the transaction succeeded.
    pub success: bool,
}

impl SimulationOutput {
    /// A successful output from the given logs.
    #[must_use]
    pub fn success(logs: Vec<String>) -> Self {
        Self {
            logs,
            success: true,
        }
    }
}

/// Anything that can execute a scenario and return logs.
pub trait ExecutionBackend {
    /// Which kind of backend this is (recorded in report metadata).
    fn kind(&self) -> BackendKind;

    /// Execute `scenario`, returning its raw logs.
    ///
    /// # Errors
    /// Returns [`crate::Error::Simulation`] on execution failure, or
    /// [`crate::Error::BackendUnimplemented`] for skeleton backends.
    fn run(&self, scenario: &Scenario) -> Result<SimulationOutput>;
}

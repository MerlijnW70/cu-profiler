//! `solana-program-test` backend — interface defined, implementation pending.
//!
//! The type and its construction surface exist so callers can target it today;
//! [`ExecutionBackend::run`] returns [`crate::Error::BackendUnimplemented`]
//! until the `program-test` integration is wired up. Keeping the Solana
//! dependency out of the default build keeps the core pure Rust and fast to
//! compile.

use std::path::PathBuf;

use crate::Result;
use crate::backend::{ExecutionBackend, SimulationOutput};
use crate::error::Error;
use crate::metadata::BackendKind;
use crate::scenario::Scenario;

/// Backend that runs scenarios in-process via `solana-program-test`.
#[derive(Debug, Clone)]
pub struct ProgramTestBackend {
    /// Path to the compiled `.so` program under test.
    pub program_so: PathBuf,
    /// The program ID to deploy under.
    pub program_id: String,
}

impl ProgramTestBackend {
    /// Construct a backend targeting a compiled program.
    #[must_use]
    pub fn new(program_so: impl Into<PathBuf>, program_id: impl Into<String>) -> Self {
        Self {
            program_so: program_so.into(),
            program_id: program_id.into(),
        }
    }
}

impl ExecutionBackend for ProgramTestBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::ProgramTest
    }

    fn run(&self, _scenario: &Scenario) -> Result<SimulationOutput> {
        Err(Error::BackendUnimplemented(
            "program-test (planned for a future release)".to_string(),
        ))
    }
}

//! `solana-program-test` backend — **interface stub**.
//!
//! This type defines the shape of a program-test backend but does not execute:
//! [`ExecutionBackend::run`] returns [`crate::Error::BackendUnimplemented`],
//! because keeping the Solana stack out of the core keeps it pure Rust and
//! buildable on Windows. The **working** implementation lives in the detached
//! `cu-profiler-program-test` integration crate; for real compute-unit metering
//! use `cu-profiler-mollusk`. This stub remains only to document the trait shape.

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
            "program-test: this core type is an interface stub — use the \
             `cu-profiler-program-test` integration crate for a working backend, \
             or `cu-profiler-mollusk` for real compute-unit metering"
                .to_string(),
        ))
    }
}

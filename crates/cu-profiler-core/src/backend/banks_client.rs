//! `BanksClient` backend — interface defined, implementation pending.
//!
//! Wraps a `BanksClient` against a test validator. Like [`super::ProgramTestBackend`]
//! it is a v1 skeleton: the interface is stable but [`ExecutionBackend::run`]
//! returns [`crate::Error::BackendUnimplemented`] for now.

use crate::Result;
use crate::backend::{ExecutionBackend, SimulationOutput};
use crate::error::Error;
use crate::metadata::BackendKind;
use crate::scenario::Scenario;

/// Backend that drives a `BanksClient`.
#[derive(Debug, Clone, Default)]
pub struct BanksClientBackend {
    /// Optional RPC/validator endpoint the client should connect to.
    pub endpoint: Option<String>,
}

impl BanksClientBackend {
    /// Construct a backend, optionally targeting an endpoint.
    #[must_use]
    pub fn new(endpoint: Option<String>) -> Self {
        Self { endpoint }
    }
}

impl ExecutionBackend for BanksClientBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::BanksClient
    }

    fn run(&self, _scenario: &Scenario) -> Result<SimulationOutput> {
        Err(Error::BackendUnimplemented(
            "banks-client (planned for a future release)".to_string(),
        ))
    }
}

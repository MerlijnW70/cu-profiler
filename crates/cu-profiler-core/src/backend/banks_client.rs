//! `BanksClient` backend — **interface stub** (not yet implemented).
//!
//! Wraps a `BanksClient` against a test validator. Like [`super::ProgramTestBackend`]
//! it is a stub: [`ExecutionBackend::run`] returns
//! [`crate::Error::BackendUnimplemented`]. For real compute-unit metering today,
//! use the `cu-profiler-mollusk` integration crate; this stub documents the
//! intended shape and is on the [roadmap](https://github.com/MerlijnW70/cu-profiler/blob/main/ROADMAP.md).

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
            "banks-client: this core type is an interface stub and is not yet \
             implemented — use the `cu-profiler-mollusk` integration crate for \
             real compute-unit metering today"
                .to_string(),
        ))
    }
}

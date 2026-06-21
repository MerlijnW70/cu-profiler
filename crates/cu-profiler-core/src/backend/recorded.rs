//! Deterministic backend that replays recorded logs.
//!
//! This is the backbone of testing and the `inspect` command: it depends on no
//! Solana runtime, so parser, budget, and CI logic can be developed and tested
//! in isolation.

use std::collections::HashMap;

use crate::Result;
use crate::backend::{ExecutionBackend, SimulationOutput};
use crate::error::Error;
use crate::metadata::BackendKind;
use crate::scenario::Scenario;

/// A backend that returns pre-recorded logs keyed by scenario name.
#[derive(Debug, Default, Clone)]
pub struct RecordedLogsBackend {
    by_scenario: HashMap<String, SimulationOutput>,
}

impl RecordedLogsBackend {
    /// An empty backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register logs for a scenario. `success` reflects the simulated outcome.
    pub fn insert(&mut self, scenario: impl Into<String>, logs: Vec<String>, success: bool) {
        self.by_scenario
            .insert(scenario.into(), SimulationOutput { logs, success });
    }

    /// Register logs from a raw multi-line blob (splitting on newlines).
    pub fn insert_blob(&mut self, scenario: impl Into<String>, blob: &str, success: bool) {
        let logs = blob.lines().map(str::to_string).collect();
        self.insert(scenario, logs, success);
    }
}

impl ExecutionBackend for RecordedLogsBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Recorded
    }

    /// Replaying a fixed log is deterministic — multi-sampling is a no-op here.
    fn is_deterministic(&self) -> bool {
        true
    }

    fn run(&self, scenario: &Scenario) -> Result<SimulationOutput> {
        self.by_scenario
            .get(&scenario.name)
            .cloned()
            .ok_or_else(|| {
                Error::Simulation(format!(
                    "no recorded logs registered for scenario `{}`",
                    scenario.name
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_registered_logs() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob("swap", "Program X invoke [1]\nProgram X success", true);
        let out = backend.run(&Scenario::new("swap")).unwrap();
        assert_eq!(out.logs.len(), 2);
        assert!(out.success);
    }

    #[test]
    fn missing_scenario_errors_clearly() {
        let backend = RecordedLogsBackend::new();
        let err = backend.run(&Scenario::new("nope")).unwrap_err();
        assert!(err.to_string().contains("no recorded logs"));
    }
}

//! Run-level metadata attached to every report and baseline record.

use serde::{Deserialize, Serialize};

/// Which execution backend produced a measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    /// Replayed from recorded logs (deterministic; used for tests and `inspect`).
    Recorded,
    /// `solana-program-test` in-process runtime.
    ProgramTest,
    /// `BanksClient` against a test validator.
    BanksClient,
    /// `mollusk-svm` harness (real compute-unit metering of an SBF program).
    Mollusk,
    /// RPC `simulateTransaction` (designed, not implemented in v1).
    RpcSimulation,
}

impl BackendKind {
    /// Stable lowercase identifier.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::ProgramTest => "program-test",
            Self::BanksClient => "banks-client",
            Self::Mollusk => "mollusk",
            Self::RpcSimulation => "rpc-simulation",
        }
    }
}

/// Whether instrumentation markers were active during a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstrumentationMode {
    /// No profiler markers expected.
    Off,
    /// Profiler markers expected and parsed.
    On,
}

/// Metadata describing the environment a report was generated in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMetadata {
    /// Version of `cu-profiler` that produced the report.
    pub profiler_version: String,
    /// Backend used.
    pub backend: BackendKind,
    /// Instrumentation mode.
    pub instrumentation: InstrumentationMode,
    /// Git commit, if discoverable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Solana/Agave crate versions, if known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solana_versions: Vec<String>,
    /// RFC3339 timestamp, if the caller chose to stamp one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
}

impl RunMetadata {
    /// Metadata for a recorded-logs run with the given profiler version.
    #[must_use]
    pub fn recorded(profiler_version: impl Into<String>) -> Self {
        Self {
            profiler_version: profiler_version.into(),
            backend: BackendKind::Recorded,
            instrumentation: InstrumentationMode::Off,
            git_commit: None,
            solana_versions: Vec::new(),
            generated_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_kind_as_str_is_stable() {
        assert_eq!(BackendKind::Recorded.as_str(), "recorded");
        assert_eq!(BackendKind::ProgramTest.as_str(), "program-test");
        assert_eq!(BackendKind::BanksClient.as_str(), "banks-client");
        assert_eq!(BackendKind::Mollusk.as_str(), "mollusk");
        assert_eq!(BackendKind::RpcSimulation.as_str(), "rpc-simulation");
    }

    #[test]
    fn recorded_metadata_uses_the_recorded_backend() {
        let m = RunMetadata::recorded("1.2.3");
        assert_eq!(m.backend, BackendKind::Recorded);
        assert_eq!(m.instrumentation, InstrumentationMode::Off);
        assert_eq!(m.profiler_version, "1.2.3");
    }
}

//! Typed error model for `cu-profiler-core`.
//!
//! Errors are actionable: parser failures carry the offending log line index and
//! the concrete reason, so a caller can render a message like
//! `Failed to parse compute-unit line at log index 42: expected integer after
//! "consumed"` rather than a bare `parse failed`.

use std::fmt;

/// The result type used throughout the core crate.
pub type Result<T> = std::result::Result<T, Error>;

/// All error conditions the core can surface.
///
/// Variants map onto the documented CLI exit codes (see the `cu-profiler-cli`
/// crate) so the boundary layer can translate an [`Error`] into a stable code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration file was missing, malformed, or semantically invalid.
    #[error("configuration error: {0}")]
    Config(String),

    /// A Solana log line could not be parsed. Carries enough context to locate
    /// and explain the failure.
    #[error("failed to parse {what} at log index {index}: {reason}")]
    Parse {
        /// What the parser was trying to read (e.g. `"compute-unit line"`).
        what: String,
        /// Zero-based index of the offending line within the log stream.
        index: usize,
        /// Why parsing failed, in plain language.
        reason: String,
    },

    /// A simulation backend failed to execute a scenario.
    #[error("simulation failure: {0}")]
    Simulation(String),

    /// A baseline could not be read, written, or compared.
    #[error("baseline error: {0}")]
    Baseline(String),

    /// A backend exists as an interface but is not implemented in this build.
    #[error("execution backend `{0}` is not implemented in this build")]
    BackendUnimplemented(String),

    /// An underlying I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A TOML (de)serialization failure, kept as a string so the error type
    /// stays independent of the `toml` error representation.
    #[error("toml error: {0}")]
    Toml(String),

    /// A JSON (de)serialization failure.
    #[cfg(feature = "json")]
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Build a [`Error::Parse`] without repeating the field names at call sites.
    pub fn parse(what: impl fmt::Display, index: usize, reason: impl fmt::Display) -> Self {
        Self::Parse {
            what: what.to_string(),
            index,
            reason: reason.to_string(),
        }
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e.to_string())
    }
}

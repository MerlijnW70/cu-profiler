//! `cu-profiler-core` — the Solana compute-intelligence engine.
//!
//! This crate owns the domain model, the Solana log parser, the CPI call-tree
//! and scope-marker analysis, the budget policy engine, baselines, confidence
//! scoring, and diagnostics. It depends on no CLI code and no live Solana
//! runtime by default: the [`backend::RecordedLogsBackend`] drives the entire
//! pipeline from logs, which is how the parser, reports and CI logic are
//! developed and tested.
//!
//! # Honesty about limitations
//! - Scope/function attribution requires explicit markers; there is no automatic
//!   source-line profiling.
//! - `program-test` results may differ from mainnet runtime conditions.
//! - Baselines are only valid when their [`baseline::Fingerprint`] matches.
//!
//! # Example
//! ```
//! use cu_profiler_core::backend::RecordedLogsBackend;
//! use cu_profiler_core::metadata::RunMetadata;
//! use cu_profiler_core::scenario::Scenario;
//! use cu_profiler_core::Profiler;
//!
//! let mut backend = RecordedLogsBackend::new();
//! backend.insert_blob(
//!     "swap",
//!     "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
//!     true,
//! );
//! let report = Profiler::new().run(
//!     &backend,
//!     &[Scenario::new("swap")],
//!     None,
//!     RunMetadata::recorded(cu_profiler_core::VERSION),
//! );
//! assert_eq!(report.scenarios[0].measurement.total_cu, 1000);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "anchor")]
pub mod anchor;
pub mod backend;
pub mod baseline;
pub mod budget;
pub mod confidence;
pub mod config;
pub mod diagnostics;
pub mod error;
pub mod metadata;
pub mod model;
pub mod parser;
pub mod profiler;
pub mod program_registry;
pub mod scenario;

pub use error::{Error, Result};
pub use profiler::Profiler;

/// The version of `cu-profiler-core`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

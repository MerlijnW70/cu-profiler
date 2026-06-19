//! `cu-profiler-instrumentation` — lightweight, opt-in scope markers for Solana
//! programs.
//!
//! The crate emits the same marker lines that `cu-profiler-core` parses. It has
//! no Solana dependency: you supply an `emit` closure (usually wrapping `msg!`),
//! which keeps the crate portable and testable.
//!
//! Instrumentation is **off by default** and gated behind the `instrumentation`
//! feature, because markers add real compute overhead — overhead the profiler
//! can detect and report.
//!
//! # Example (no feature required)
//! ```
//! use cu_profiler_instrumentation::markers;
//! // In a program you would do: `msg!("{}", markers::begin_line("swap::validate"));`
//! assert_eq!(markers::begin_line("x"), "CU_PROFILER_BEGIN name=x");
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod macros;
pub mod markers;

#[cfg(feature = "instrumentation")]
pub use macros::ScopeGuard;

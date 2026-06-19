//! `cu-profiler-report` — rendering for the core report model.
//!
//! Every renderer takes a [`cu_profiler_core::model::Report`] and produces a
//! string. The crate holds no analysis logic; it only formats already-computed
//! data, keeping the raw-data/presentation boundary clean.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod json;
pub mod junit;
pub mod markdown;
pub mod model;
pub mod table;

use std::str::FromStr;

use cu_profiler_core::model::Report;
use cu_profiler_core::{Error, Result};

/// A selectable output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Aligned text table.
    Table,
    /// Stable JSON.
    Json,
    /// Markdown (PR comments).
    Markdown,
    /// JUnit XML.
    Junit,
}

impl Format {
    /// All formats, for help text and iteration.
    pub const ALL: [Format; 4] = [Format::Table, Format::Json, Format::Markdown, Format::Junit];

    /// Lowercase identifier.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Table => "table",
            Format::Json => "json",
            Format::Markdown => "markdown",
            Format::Junit => "junit",
        }
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "table" => Ok(Format::Table),
            "json" => Ok(Format::Json),
            "markdown" | "md" => Ok(Format::Markdown),
            "junit" | "xml" => Ok(Format::Junit),
            other => Err(Error::Config(format!(
                "unknown output format `{other}` (expected table|json|markdown|junit)"
            ))),
        }
    }
}

/// Render `report` in the requested `format`.
///
/// # Errors
/// Propagates serialization failures from the JSON renderer.
pub fn render(report: &Report, format: Format) -> Result<String> {
    Ok(match format {
        Format::Table => table::render(report),
        Format::Json => json::render(report)?,
        Format::Markdown => markdown::render(report),
        Format::Junit => junit::render(report),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_parsing_round_trips() {
        for f in Format::ALL {
            assert_eq!(Format::from_str(f.as_str()).unwrap(), f);
        }
        assert!(Format::from_str("yaml").is_err());
        assert_eq!(Format::from_str("md").unwrap(), Format::Markdown);
    }
}

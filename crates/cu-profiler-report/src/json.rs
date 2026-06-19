//! Stable JSON output for machines and CI.
//!
//! The schema is the serialized [`Report`]; it is intentionally the same shape
//! the `inspect` command reads back.

use cu_profiler_core::Result;

use crate::model::Report;

/// Render `report` as pretty-printed JSON.
///
/// # Errors
/// Propagates any serialization failure as [`cu_profiler_core::Error`].
pub fn render(report: &Report) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// Parse a [`Report`] back from JSON (used by `cu-profiler inspect`).
///
/// # Errors
/// Propagates any deserialization failure.
pub fn parse(json: &str) -> Result<Report> {
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cu_profiler_core::Profiler;
    use cu_profiler_core::backend::RecordedLogsBackend;
    use cu_profiler_core::metadata::RunMetadata;
    use cu_profiler_core::scenario::Scenario;

    #[test]
    fn json_round_trips() {
        let mut backend = RecordedLogsBackend::new();
        backend.insert_blob(
            "s",
            "Program P invoke [1]\nProgram P consumed 1000 of 200000 compute units\nProgram P success",
            true,
        );
        let report = Profiler::new().run(
            &backend,
            &[Scenario::new("s")],
            None,
            RunMetadata::recorded("0.1.0"),
        );
        let json = render(&report).unwrap();
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"total_cu\""));
        let back = parse(&json).unwrap();
        assert_eq!(report, back);
    }
}

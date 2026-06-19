//! Tolerant lexer for Solana program logs.
//!
//! The lexer turns raw log lines into a flat stream of [`LogEvent`]s. It never
//! panics on malformed input: a line it cannot classify is preserved as
//! [`LogEvent::Raw`], and a line that looks structured but parses badly yields a
//! [`LogEvent::Raw`] plus an actionable warning, lowering confidence downstream.

use crate::parser::scope_markers::{self, MarkerKind};

/// One recognised (or preserved) log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent {
    /// `Program <id> invoke [<depth>]`
    Invoke {
        /// Program being invoked.
        program_id: String,
        /// Invoke depth (1 for the top level).
        depth: u32,
    },
    /// `Program <id> consumed <used> of <budget> compute units`
    Consumed {
        /// Program reporting consumption.
        program_id: String,
        /// CU consumed by this invocation.
        used: u64,
        /// CU budget available to this invocation.
        budget: u64,
    },
    /// `Program <id> success`
    Success {
        /// Program that succeeded.
        program_id: String,
    },
    /// `Program <id> failed: <reason>` (or `Program failed to complete`).
    Failed {
        /// Program that failed (may be empty if the runtime didn't name it).
        program_id: String,
        /// Failure reason.
        reason: String,
    },
    /// A `Program log:` message that is not a profiler marker.
    Log {
        /// The message text.
        message: String,
    },
    /// A balanced-scope begin marker.
    ScopeBegin {
        /// Scope name.
        name: String,
        /// Remaining compute units at the marker, if logged.
        cu: Option<u64>,
    },
    /// A scope end marker.
    ScopeEnd {
        /// Scope name.
        name: String,
        /// Remaining compute units at the marker, if logged.
        cu: Option<u64>,
    },
    /// A point-in-time marker.
    ScopePoint {
        /// Marker name.
        name: String,
        /// Remaining compute units at the marker, if logged.
        cu: Option<u64>,
    },
    /// Any line that could not be classified, preserved verbatim.
    Raw(String),
}

/// The outcome of lexing a log stream.
#[derive(Debug, Default, Clone)]
pub struct LexResult {
    /// Recognised events, each tagged with its source line index.
    pub events: Vec<(usize, LogEvent)>,
    /// Non-fatal warnings (malformed-but-recoverable lines).
    pub warnings: Vec<String>,
}

impl LexResult {
    /// Iterate over just the events, dropping line indices.
    pub fn events(&self) -> impl Iterator<Item = &LogEvent> {
        self.events.iter().map(|(_, e)| e)
    }
}

/// Lex a slice of log lines into a [`LexResult`].
#[must_use]
pub fn lex(lines: &[String]) -> LexResult {
    let mut result = LexResult::default();
    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        match classify(line, index) {
            Ok(event) => result.events.push((index, event)),
            Err(warning) => {
                result.warnings.push(warning);
                result.events.push((index, LogEvent::Raw(raw.clone())));
            }
        }
    }
    result
}

/// Classify a single trimmed line. `Err` carries an actionable warning string;
/// the caller preserves the original line as [`LogEvent::Raw`].
fn classify(line: &str, index: usize) -> Result<LogEvent, String> {
    if let Some(rest) = line.strip_prefix("Program log: ") {
        return Ok(classify_log_message(rest));
    }
    if let Some(rest) = line.strip_prefix("Program data: ") {
        return Ok(LogEvent::Log {
            message: format!("data: {rest}"),
        });
    }
    if line == "Program failed to complete" {
        return Ok(LogEvent::Failed {
            program_id: String::new(),
            reason: "failed to complete".to_string(),
        });
    }
    if let Some(rest) = line.strip_prefix("Program ") {
        return classify_program_line(rest, index);
    }
    Ok(LogEvent::Raw(line.to_string()))
}

/// Handle a `Program log:` payload, detecting profiler markers.
fn classify_log_message(message: &str) -> LogEvent {
    match scope_markers::parse_marker(message) {
        Some(m) => match m.kind {
            MarkerKind::Begin => LogEvent::ScopeBegin {
                name: m.name,
                cu: m.cu,
            },
            MarkerKind::End => LogEvent::ScopeEnd {
                name: m.name,
                cu: m.cu,
            },
            MarkerKind::Point => LogEvent::ScopePoint {
                name: m.name,
                cu: m.cu,
            },
        },
        None => LogEvent::Log {
            message: message.to_string(),
        },
    }
}

/// Handle the portion of a line after the leading `Program `.
fn classify_program_line(rest: &str, index: usize) -> Result<LogEvent, String> {
    // `<id> invoke [<depth>]`
    if let Some((id, tail)) = rest.split_once(" invoke ") {
        let depth = tail
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<u32>()
            .map_err(|_| {
                format!(
                    "failed to parse invoke depth at log index {index}: expected `[<n>]`, got `{tail}`"
                )
            })?;
        return Ok(LogEvent::Invoke {
            program_id: id.to_string(),
            depth,
        });
    }

    // `<id> consumed <used> of <budget> compute units`
    if let Some((id, tail)) = rest.split_once(" consumed ") {
        return parse_consumed(id, tail, index).map_err(|e| e.to_string());
    }

    // `<id> success`
    if let Some(id) = rest.strip_suffix(" success") {
        return Ok(LogEvent::Success {
            program_id: id.to_string(),
        });
    }

    // `<id> failed: <reason>`
    if let Some((id, reason)) = rest.split_once(" failed: ") {
        return Ok(LogEvent::Failed {
            program_id: id.to_string(),
            reason: reason.to_string(),
        });
    }
    if let Some(id) = rest.strip_suffix(" failed") {
        return Ok(LogEvent::Failed {
            program_id: id.to_string(),
            reason: "failed".to_string(),
        });
    }

    Ok(LogEvent::Raw(format!("Program {rest}")))
}

/// Parse the `<used> of <budget> compute units` tail. Returns a typed
/// [`crate::Error`] with the precise reason on malformed numerics.
fn parse_consumed(id: &str, tail: &str, index: usize) -> crate::Result<LogEvent> {
    let body = tail.strip_suffix(" compute units").unwrap_or(tail).trim();
    let (used_s, budget_s) = body.split_once(" of ").ok_or_else(|| {
        crate::Error::parse(
            "compute-unit line",
            index,
            format!("expected `<used> of <budget> compute units`, got `{tail}`"),
        )
    })?;
    let used = used_s.trim().replace(',', "").parse::<u64>().map_err(|_| {
        crate::Error::parse(
            "compute-unit line",
            index,
            format!("expected integer for consumed units, got `{used_s}`"),
        )
    })?;
    let budget = budget_s
        .trim()
        .replace(',', "")
        .parse::<u64>()
        .map_err(|_| {
            crate::Error::parse(
                "compute-unit line",
                index,
                format!("expected integer for unit budget, got `{budget_s}`"),
            )
        })?;
    Ok(LogEvent::Consumed {
        program_id: id.to_string(),
        used,
        budget,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_invoke_consumed_success() {
        let lines = vec![
            "Program Vote111 invoke [1]".to_string(),
            "Program Vote111 consumed 1200 of 200000 compute units".to_string(),
            "Program Vote111 success".to_string(),
        ];
        let r = lex(&lines);
        assert!(r.warnings.is_empty());
        assert_eq!(
            r.events[0].1,
            LogEvent::Invoke {
                program_id: "Vote111".into(),
                depth: 1
            }
        );
        assert_eq!(
            r.events[1].1,
            LogEvent::Consumed {
                program_id: "Vote111".into(),
                used: 1200,
                budget: 200000
            }
        );
        assert_eq!(
            r.events[2].1,
            LogEvent::Success {
                program_id: "Vote111".into()
            }
        );
    }

    #[test]
    fn unknown_lines_are_preserved_not_panicked() {
        let lines = vec!["totally unexpected line".to_string()];
        let r = lex(&lines);
        assert_eq!(
            r.events[0].1,
            LogEvent::Raw("totally unexpected line".into())
        );
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn malformed_consumed_warns_and_keeps_raw() {
        let lines = vec!["Program X consumed abc of 200000 compute units".to_string()];
        let r = lex(&lines);
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("expected integer for consumed units"));
        assert!(matches!(r.events[0].1, LogEvent::Raw(_)));
    }

    #[test]
    fn detects_failure_with_reason() {
        let lines = vec!["Program X failed: custom program error: 0x1".to_string()];
        let r = lex(&lines);
        assert_eq!(
            r.events[0].1,
            LogEvent::Failed {
                program_id: "X".into(),
                reason: "custom program error: 0x1".into()
            }
        );
    }

    #[test]
    fn detects_scope_markers_in_log_messages() {
        let lines = vec![
            "Program log: CU_PROFILER_BEGIN name=swap::validate cu=200000".to_string(),
            "Program log: hello".to_string(),
            "Program log: CU_PROFILER_END name=swap::validate cu=195000".to_string(),
        ];
        let r = lex(&lines);
        assert_eq!(
            r.events[0].1,
            LogEvent::ScopeBegin {
                name: "swap::validate".into(),
                cu: Some(200_000),
            }
        );
        assert!(matches!(r.events[1].1, LogEvent::Log { .. }));
        assert_eq!(
            r.events[2].1,
            LogEvent::ScopeEnd {
                name: "swap::validate".into(),
                cu: Some(195_000),
            }
        );
    }
}

//! Profiler scope markers.
//!
//! Scopes are attributed *only* from explicit markers — the tool makes no claim
//! of automatic source-line profiling. Balanced begin/end pairs raise
//! confidence; unbalanced markers produce warnings and lower it.

use serde::{Deserialize, Serialize};

/// Which kind of marker a log message carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// `CU_PROFILER_BEGIN name=...`
    Begin,
    /// `CU_PROFILER_END name=...`
    End,
    /// `CU_PROFILER_POINT name=...`
    Point,
}

/// How a scope's CU figure was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributionMethod {
    /// Derived directly from a reliable consumed-CU delta between markers.
    LogDelta,
    /// Estimated; treat as indicative only.
    Estimated,
    /// Could not be attributed.
    Unknown,
}

/// The attribution result for a single scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeResult {
    /// Scope name from the marker.
    pub name: String,
    /// Parent scope, if nested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Estimated CU for the scope, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units_estimated: Option<u64>,
    /// Share of total CU, if estimable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage_of_total: Option<f64>,
    /// How the figure was derived.
    pub attribution_method: AttributionMethod,
    /// Per-scope warnings (e.g. unbalanced markers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Parse a `Program log:` message into a marker, if it is one.
///
/// Recognised forms (case-sensitive):
/// `CU_PROFILER_BEGIN name=<n>`, `CU_PROFILER_END name=<n>`,
/// `CU_PROFILER_POINT name=<n>`.
#[must_use]
pub fn parse_marker(message: &str) -> Option<(MarkerKind, String)> {
    let msg = message.trim();
    let (kind, rest) = if let Some(r) = msg.strip_prefix("CU_PROFILER_BEGIN") {
        (MarkerKind::Begin, r)
    } else if let Some(r) = msg.strip_prefix("CU_PROFILER_END") {
        (MarkerKind::End, r)
    } else if let Some(r) = msg.strip_prefix("CU_PROFILER_POINT") {
        (MarkerKind::Point, r)
    } else {
        return None;
    };
    let name = extract_name(rest)?;
    Some((kind, name))
}

fn extract_name(rest: &str) -> Option<String> {
    let rest = rest.trim();
    let after = rest.strip_prefix("name=")?;
    let name = after.split_whitespace().next()?.to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Check whether a sequence of begin/end markers is balanced.
///
/// Returns the list of warnings; an empty list means perfectly balanced.
#[must_use]
pub fn balance_warnings(markers: &[(MarkerKind, String)]) -> Vec<String> {
    let mut stack: Vec<&str> = Vec::new();
    let mut warnings = Vec::new();
    for (kind, name) in markers {
        match kind {
            MarkerKind::Begin => stack.push(name),
            MarkerKind::End => match stack.pop() {
                Some(open) if open == name => {}
                Some(open) => warnings.push(format!(
                    "scope marker mismatch: END `{name}` does not close BEGIN `{open}`"
                )),
                None => warnings.push(format!("scope END `{name}` has no matching BEGIN")),
            },
            MarkerKind::Point => {}
        }
    }
    for unclosed in stack {
        warnings.push(format!("scope BEGIN `{unclosed}` was never closed"));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_begin_marker() {
        assert_eq!(
            parse_marker("CU_PROFILER_BEGIN name=swap::validate"),
            Some((MarkerKind::Begin, "swap::validate".to_string()))
        );
    }

    #[test]
    fn non_marker_is_none() {
        assert_eq!(parse_marker("just a log line"), None);
    }

    #[test]
    fn balanced_markers_have_no_warnings() {
        let markers = vec![
            (MarkerKind::Begin, "a".to_string()),
            (MarkerKind::End, "a".to_string()),
        ];
        assert!(balance_warnings(&markers).is_empty());
    }

    #[test]
    fn unbalanced_markers_warn() {
        let markers = vec![(MarkerKind::Begin, "a".to_string())];
        let w = balance_warnings(&markers);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("never closed"));
    }
}

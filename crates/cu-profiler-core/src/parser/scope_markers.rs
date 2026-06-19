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

/// A parsed profiler marker, optionally carrying a compute snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Marker kind.
    pub kind: MarkerKind,
    /// Scope/point name.
    pub name: String,
    /// Remaining compute units at the marker, if the program logged one
    /// (`cu=<n>`). Enables reliable [`AttributionMethod::LogDelta`] estimation.
    pub cu: Option<u64>,
}

/// Parse a `Program log:` message into a [`Marker`], if it is one.
///
/// Recognised forms (case-sensitive), with an optional trailing `cu=<n>` giving
/// the *remaining* compute units at the marker:
/// `CU_PROFILER_BEGIN name=<n> [cu=<n>]`, `CU_PROFILER_END name=<n> [cu=<n>]`,
/// `CU_PROFILER_POINT name=<n> [cu=<n>]`.
#[must_use]
pub fn parse_marker(message: &str) -> Option<Marker> {
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
    let cu = extract_cu(rest);
    Some(Marker { kind, name, cu })
}

fn extract_name(rest: &str) -> Option<String> {
    let after = rest.trim().strip_prefix("name=")?;
    let name = after.split_whitespace().next()?.to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn extract_cu(rest: &str) -> Option<u64> {
    rest.split_whitespace()
        .find_map(|tok| tok.strip_prefix("cu="))
        .and_then(|v| v.parse::<u64>().ok())
}

/// Check whether a sequence of begin/end markers is balanced.
///
/// Returns the list of warnings; an empty list means perfectly balanced.
#[must_use]
pub fn balance_warnings(markers: &[Marker]) -> Vec<String> {
    let mut stack: Vec<&str> = Vec::new();
    let mut warnings = Vec::new();
    for marker in markers {
        match marker.kind {
            MarkerKind::Begin => stack.push(&marker.name),
            MarkerKind::End => match stack.pop() {
                Some(open) if open == marker.name => {}
                Some(open) => warnings.push(format!(
                    "scope marker mismatch: END `{}` does not close BEGIN `{open}`",
                    marker.name
                )),
                None => warnings.push(format!("scope END `{}` has no matching BEGIN", marker.name)),
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

    fn marker(kind: MarkerKind, name: &str) -> Marker {
        Marker {
            kind,
            name: name.to_string(),
            cu: None,
        }
    }

    #[test]
    fn parses_begin_marker() {
        assert_eq!(
            parse_marker("CU_PROFILER_BEGIN name=swap::validate"),
            Some(marker(MarkerKind::Begin, "swap::validate"))
        );
    }

    #[test]
    fn parses_marker_with_compute_snapshot() {
        let m = parse_marker("CU_PROFILER_END name=swap::math cu=187654").unwrap();
        assert_eq!(m.kind, MarkerKind::End);
        assert_eq!(m.name, "swap::math");
        assert_eq!(m.cu, Some(187_654));
    }

    #[test]
    fn non_marker_is_none() {
        assert_eq!(parse_marker("just a log line"), None);
    }

    #[test]
    fn balanced_markers_have_no_warnings() {
        let markers = vec![marker(MarkerKind::Begin, "a"), marker(MarkerKind::End, "a")];
        assert!(balance_warnings(&markers).is_empty());
    }

    #[test]
    fn unbalanced_markers_warn() {
        let markers = vec![marker(MarkerKind::Begin, "a")];
        let w = balance_warnings(&markers);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("never closed"));
    }
}

//! Solana log parsing: lexer, compute-budget heuristics, CPI tree, and scope
//! markers, plus a [`analyze`] facade that turns raw logs into a structured
//! [`ParseAnalysis`] the profiler can consume.

pub mod compute_budget;
pub mod cpi_tree;
pub mod scope_markers;
pub mod solana_logs;

pub use cpi_tree::{CallNode, NodeStatus};
pub use scope_markers::{AttributionMethod, MarkerKind, ScopeResult};
pub use solana_logs::{LexResult, LogEvent};

use crate::program_registry::ProgramRegistry;

/// Structured result of analysing a single transaction's logs.
#[derive(Debug, Clone)]
pub struct ParseAnalysis {
    /// Reconstructed call tree (root included).
    pub call_tree: CallNode,
    /// Total CU consumed across top-level (depth-1) invocations.
    pub total_cu: u64,
    /// Number of CPIs (depth >= 2).
    pub cpi_count: u32,
    /// CPI nesting depth (max depth beyond the entrypoint).
    pub cpi_depth: u32,
    /// Estimated requested compute limit, if derivable.
    pub requested_limit: Option<u64>,
    /// Requested but unused CU, if derivable.
    pub over_requested: Option<u64>,
    /// Percentage of total CU not attributed to a CPI child (0..=100).
    pub unattributed_pct: f64,
    /// Scope attribution results.
    pub scopes: Vec<ScopeResult>,
    /// Number of scope markers detected.
    pub scope_marker_count: usize,
    /// Whether the transaction simulated without a `failed` line.
    pub simulation_success: bool,
    /// Number of `Program log:` / `Program data:` lines emitted (log volume).
    pub log_line_count: usize,
    /// Whether a validation scope began *after* a CPI was issued — a hint that
    /// cheap checks run too late. Only set when scope markers are present.
    pub validation_after_cpi: bool,
    /// Parser warnings collected along the way.
    pub warnings: Vec<String>,
    /// Whether the logs parsed cleanly (no warnings).
    pub logs_complete: bool,
}

/// Analyse a log stream into a [`ParseAnalysis`].
#[must_use]
pub fn analyze(logs: &[String], registry: &ProgramRegistry) -> ParseAnalysis {
    let lexed = solana_logs::lex(logs);
    let events: Vec<LogEvent> = lexed.events().cloned().collect();
    let mut warnings = lexed.warnings.clone();

    let call_tree = cpi_tree::build(&events, registry);
    let total_cu = sum_units_at(&call_tree, 1);
    let cpi_attributed = sum_units_below(&call_tree, 2);
    let cpi_count = call_tree.cpi_count();
    let cpi_depth = call_tree.max_depth().saturating_sub(1);

    let requested_limit = compute_budget::estimated_requested_limit(&events);
    let over_requested = requested_limit.and_then(|lim| lim.checked_sub(total_cu));

    let unattributed = total_cu.saturating_sub(cpi_attributed);
    let unattributed_pct = if total_cu == 0 {
        0.0
    } else {
        (unattributed as f64 / total_cu as f64) * 100.0
    };

    let (scopes, scope_marker_count, scope_warnings) = collect_scopes(&events);
    warnings.extend(scope_warnings);

    let simulation_success = !events.iter().any(|e| matches!(e, LogEvent::Failed { .. }));

    let log_line_count = events
        .iter()
        .filter(|e| matches!(e, LogEvent::Log { .. }))
        .count();
    let validation_after_cpi = detect_validation_after_cpi(&events);

    let logs_complete = warnings.is_empty();

    ParseAnalysis {
        call_tree,
        total_cu,
        cpi_count,
        cpi_depth,
        requested_limit,
        over_requested,
        unattributed_pct,
        scopes,
        scope_marker_count,
        simulation_success,
        log_line_count,
        validation_after_cpi,
        warnings,
        logs_complete,
    }
}

/// True if a scope whose name mentions validation opens after the first CPI
/// invoke (depth >= 2). Evidence-based and marker-gated: with no scope markers
/// it can never fire, so it makes no unsupported claim.
fn detect_validation_after_cpi(events: &[LogEvent]) -> bool {
    let mut cpi_seen = false;
    for e in events {
        match e {
            LogEvent::Invoke { depth, .. } if *depth >= 2 => cpi_seen = true,
            LogEvent::ScopeBegin { name } if cpi_seen => {
                let lower = name.to_ascii_lowercase();
                if lower.contains("valid") || lower.contains("check") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn sum_units_at(node: &CallNode, depth: u32) -> u64 {
    let here = if node.depth == depth {
        node.units_consumed.unwrap_or(0)
    } else {
        0
    };
    here + node
        .children
        .iter()
        .map(|c| sum_units_at(c, depth))
        .sum::<u64>()
}

fn sum_units_below(node: &CallNode, min_depth: u32) -> u64 {
    let here = if node.depth >= min_depth {
        node.units_consumed.unwrap_or(0)
    } else {
        0
    };
    here + node
        .children
        .iter()
        .map(|c| sum_units_below(c, min_depth))
        .sum::<u64>()
}

fn collect_scopes(events: &[LogEvent]) -> (Vec<ScopeResult>, usize, Vec<String>) {
    use scope_markers::{AttributionMethod, MarkerKind};

    let mut markers: Vec<(MarkerKind, String)> = Vec::new();
    for e in events {
        match e {
            LogEvent::ScopeBegin { name } => markers.push((MarkerKind::Begin, name.clone())),
            LogEvent::ScopeEnd { name } => markers.push((MarkerKind::End, name.clone())),
            LogEvent::ScopePoint { name } => markers.push((MarkerKind::Point, name.clone())),
            _ => {}
        }
    }
    let warnings = scope_markers::balance_warnings(&markers);

    // One ScopeResult per BEGIN, with parent inferred from nesting.
    let mut stack: Vec<String> = Vec::new();
    let mut scopes: Vec<ScopeResult> = Vec::new();
    for (kind, name) in &markers {
        match kind {
            MarkerKind::Begin => {
                let parent = stack.last().cloned();
                scopes.push(ScopeResult {
                    name: name.clone(),
                    parent,
                    units_estimated: None,
                    percentage_of_total: None,
                    attribution_method: AttributionMethod::Estimated,
                    warnings: Vec::new(),
                });
                stack.push(name.clone());
            }
            MarkerKind::End => {
                if stack.last() == Some(name) {
                    stack.pop();
                }
            }
            MarkerKind::Point => {}
        }
    }

    (scopes, markers.len(), warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn computes_totals_and_unattributed() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 3000 of 197000 compute units",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
            "Program User111 consumed 12000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert_eq!(a.total_cu, 12_000);
        assert_eq!(a.cpi_count, 1);
        assert_eq!(a.cpi_depth, 1);
        assert_eq!(a.requested_limit, Some(200_000));
        assert_eq!(a.over_requested, Some(188_000));
        // 12000 total - 3000 in CPI = 9000 unattributed = 75%.
        assert!((a.unattributed_pct - 75.0).abs() < 0.01);
        assert!(a.simulation_success);
    }

    #[test]
    fn scopes_inferred_with_nesting() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program log: CU_PROFILER_BEGIN name=outer",
            "Program log: CU_PROFILER_BEGIN name=inner",
            "Program log: CU_PROFILER_END name=inner",
            "Program log: CU_PROFILER_END name=outer",
            "Program User111 consumed 5000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert_eq!(a.scope_marker_count, 4);
        assert_eq!(a.scopes.len(), 2);
        assert_eq!(a.scopes[1].parent.as_deref(), Some("outer"));
        assert!(a.warnings.is_empty());
    }

    #[test]
    fn detects_log_volume_and_late_validation() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
            // Validation marker opens *after* the CPI above.
            "Program log: CU_PROFILER_BEGIN name=swap::validate_accounts",
            "Program log: CU_PROFILER_END name=swap::validate_accounts",
            "Program log: emitting event one",
            "Program log: emitting event two",
            "Program User111 consumed 9000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert!(a.validation_after_cpi);
        assert_eq!(a.log_line_count, 2); // the two non-marker `Program log:` lines
    }

    #[test]
    fn validation_before_cpi_is_not_flagged() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program log: CU_PROFILER_BEGIN name=swap::validate_accounts",
            "Program log: CU_PROFILER_END name=swap::validate_accounts",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
            "Program User111 consumed 9000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert!(!a.validation_after_cpi);
    }

    #[test]
    fn failure_path_flips_simulation_success() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program User111 consumed 8000 of 200000 compute units",
            "Program User111 failed: custom program error: 0x1",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert!(!a.simulation_success);
        assert_eq!(a.total_cu, 8000);
    }
}

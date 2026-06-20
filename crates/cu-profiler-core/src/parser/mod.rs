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
    if structural_depth(&call_tree) > cpi_tree::MAX_DEPTH {
        warnings.push(format!(
            "CPI nesting exceeded {} levels; the call tree was flattened (logs are likely malformed)",
            cpi_tree::MAX_DEPTH
        ));
    }
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

    let (scopes, scope_marker_count, scope_warnings) = collect_scopes(&events, total_cu);
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
            LogEvent::ScopeBegin { name, .. } if cpi_seen => {
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

/// Structural nesting depth of the tree (bounded by [`cpi_tree::MAX_DEPTH`], so
/// this recursion is safe). Used only to detect that the cap was hit.
fn structural_depth(node: &CallNode) -> usize {
    1 + node
        .children
        .iter()
        .map(structural_depth)
        .max()
        .unwrap_or(0)
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

fn collect_scopes(events: &[LogEvent], total_cu: u64) -> (Vec<ScopeResult>, usize, Vec<String>) {
    use scope_markers::{AttributionMethod, Marker, MarkerKind};

    let mut markers: Vec<Marker> = Vec::new();
    for e in events {
        match e {
            LogEvent::ScopeBegin { name, cu } => markers.push(Marker {
                kind: MarkerKind::Begin,
                name: name.clone(),
                cu: *cu,
            }),
            LogEvent::ScopeEnd { name, cu } => markers.push(Marker {
                kind: MarkerKind::End,
                name: name.clone(),
                cu: *cu,
            }),
            LogEvent::ScopePoint { name, cu } => markers.push(Marker {
                kind: MarkerKind::Point,
                name: name.clone(),
                cu: *cu,
            }),
            _ => {}
        }
    }
    let mut warnings = scope_markers::balance_warnings(&markers);

    // One ScopeResult per BEGIN, with parent inferred from nesting. When a BEGIN
    // and its matching END both carry a `cu=` snapshot, the scope's CU is the
    // (inclusive) delta — a reliable `LogDelta` estimate; otherwise it stays an
    // unquantified `Estimated` scope (no fake precision).
    let mut stack: Vec<(usize, Option<u64>)> = Vec::new();
    let mut scopes: Vec<ScopeResult> = Vec::new();
    for m in &markers {
        match m.kind {
            MarkerKind::Begin => {
                let parent = stack.last().map(|&(i, _)| scopes[i].name.clone());
                stack.push((scopes.len(), m.cu));
                scopes.push(ScopeResult {
                    name: m.name.clone(),
                    parent,
                    units_estimated: None,
                    percentage_of_total: None,
                    attribution_method: AttributionMethod::Estimated,
                    warnings: Vec::new(),
                });
            }
            MarkerKind::End => {
                let matches_top = stack.last().is_some_and(|&(i, _)| scopes[i].name == m.name);
                if matches_top {
                    if let Some((idx, begin_cu)) = stack.pop() {
                        if let (Some(begin), Some(end)) = (begin_cu, m.cu) {
                            let units = begin.saturating_sub(end);
                            scopes[idx].units_estimated = Some(units);
                            scopes[idx].attribution_method = AttributionMethod::LogDelta;
                            // A snapshot delta larger than the measured program
                            // total is contradictory (only possible with broken
                            // logs): withhold the percentage rather than emit a
                            // nonsensical >100%, and flag the inconsistency.
                            if total_cu > 0 && units <= total_cu {
                                scopes[idx].percentage_of_total =
                                    Some((units as f64 / total_cu as f64) * 100.0);
                            } else if units > total_cu {
                                let warning = format!(
                                    "scope `{}` CU delta ({units}) exceeds the measured total ({total_cu}); percentage withheld",
                                    scopes[idx].name
                                );
                                scopes[idx].warnings.push(warning.clone());
                                warnings.push(warning);
                            }
                        }
                    }
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
    fn estimates_scope_cu_from_compute_snapshots() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program log: CU_PROFILER_BEGIN name=swap::validate cu=200000",
            "Program log: CU_PROFILER_END name=swap::validate cu=188000",
            "Program User111 consumed 24000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        let scope = &a.scopes[0];
        assert_eq!(scope.name, "swap::validate");
        // 200000 remaining at begin − 188000 at end = 12000 CU in the scope.
        assert_eq!(scope.units_estimated, Some(12_000));
        assert_eq!(scope.attribution_method, AttributionMethod::LogDelta);
        // 12000 / 24000 total = 50%.
        assert!((scope.percentage_of_total.unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn contradictory_snapshot_withholds_percentage_and_warns() {
        // Snapshot delta (12000) exceeds the program's measured total (5000):
        // inconsistent logs. The percentage must be withheld, not >100%.
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program log: CU_PROFILER_BEGIN name=swap::math cu=200000",
            "Program log: CU_PROFILER_END name=swap::math cu=188000",
            "Program User111 consumed 5000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert_eq!(a.scopes[0].units_estimated, Some(12_000));
        assert_eq!(a.scopes[0].percentage_of_total, None);
        assert!(
            a.warnings
                .iter()
                .any(|w| w.contains("exceeds the measured total"))
        );
    }

    #[test]
    fn scope_without_snapshots_stays_unquantified() {
        let logs = lines(&[
            "Program User111 invoke [1]",
            "Program log: CU_PROFILER_BEGIN name=swap::validate",
            "Program log: CU_PROFILER_END name=swap::validate",
            "Program User111 consumed 24000 of 200000 compute units",
            "Program User111 success",
        ]);
        let a = analyze(&logs, &ProgramRegistry::with_builtins());
        assert_eq!(a.scopes[0].units_estimated, None);
        assert_eq!(a.scopes[0].attribution_method, AttributionMethod::Estimated);
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

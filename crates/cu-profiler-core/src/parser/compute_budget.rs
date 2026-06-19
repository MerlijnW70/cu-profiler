//! Compute-budget heuristics derived from logs.
//!
//! The Solana runtime does not log the requested compute-unit limit directly.
//! What it *does* expose is the budget figure `Y` in
//! `consumed X of Y compute units`; for the outermost invocation that equals the
//! transaction's available budget. We use the maximum observed budget as the
//! best log-only estimate of the requested limit, and flag it as an estimate.

use crate::parser::solana_logs::LogEvent;

/// The Compute Budget program ID.
pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";

/// Best log-derived estimate of the requested compute-unit limit, if any
/// `consumed … of Y …` line was seen.
#[must_use]
pub fn estimated_requested_limit(events: &[LogEvent]) -> Option<u64> {
    events
        .iter()
        .filter_map(|e| match e {
            LogEvent::Consumed { budget, .. } => Some(*budget),
            _ => None,
        })
        .max()
}

/// Whether the Compute Budget program was invoked in this transaction.
#[must_use]
pub fn used_compute_budget_program(events: &[LogEvent]) -> bool {
    events.iter().any(|e| match e {
        LogEvent::Invoke { program_id, .. } => program_id == COMPUTE_BUDGET_PROGRAM_ID,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::solana_logs::lex;

    #[test]
    fn estimates_limit_from_max_budget() {
        let lines = vec![
            "Program A invoke [1]".to_string(),
            "Program B invoke [2]".to_string(),
            "Program B consumed 3000 of 197000 compute units".to_string(),
            "Program A consumed 12000 of 200000 compute units".to_string(),
        ];
        let events: Vec<LogEvent> = lex(&lines).events().cloned().collect();
        assert_eq!(estimated_requested_limit(&events), Some(200_000));
    }
}

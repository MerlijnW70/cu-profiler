//! Reconstructs the CPI call tree from a lexed event stream.
//!
//! The builder is tolerant: mismatched or missing success/failure lines never
//! panic; nodes simply close on a best-effort basis and the resulting tree is
//! still well-formed.

use serde::{Deserialize, Serialize};

use crate::parser::solana_logs::LogEvent;
use crate::program_registry::ProgramRegistry;

/// Outcome of a single program invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    /// Reported `success`.
    Success,
    /// Reported `failed`.
    Failed,
    /// No terminal line was seen.
    Unknown,
}

/// A node in the CPI call tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallNode {
    /// Program ID (or `"transaction"` for the synthetic root).
    pub program_id: String,
    /// Resolved display label, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Invoke depth (0 for the root, 1 for the entrypoint program).
    pub depth: u32,
    /// CU consumed by this invocation, if reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units_consumed: Option<u64>,
    /// Terminal status.
    pub status: NodeStatus,
    /// Log messages emitted directly under this invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
    /// Nested invocations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CallNode>,
}

impl CallNode {
    fn new(program_id: String, depth: u32, registry: &ProgramRegistry) -> Self {
        let label = registry.label(&program_id).map(str::to_string);
        Self {
            program_id,
            label,
            depth,
            units_consumed: None,
            status: NodeStatus::Unknown,
            logs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The synthetic transaction root.
    fn root() -> Self {
        Self {
            program_id: "transaction".to_string(),
            label: Some("root transaction".to_string()),
            depth: 0,
            units_consumed: None,
            status: NodeStatus::Unknown,
            logs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Total number of program invocations in the subtree (excluding the root).
    #[must_use]
    pub fn invocation_count(&self) -> u32 {
        self.children.iter().map(|c| 1 + c.invocation_count()).sum()
    }

    /// Maximum invoke depth in the subtree.
    #[must_use]
    pub fn max_depth(&self) -> u32 {
        self.children
            .iter()
            .map(CallNode::max_depth)
            .max()
            .map_or(self.depth, |d| d.max(self.depth))
    }

    /// Number of cross-program invocations (depth >= 2).
    #[must_use]
    pub fn cpi_count(&self) -> u32 {
        let here = u32::from(self.depth >= 2);
        here + self.children.iter().map(CallNode::cpi_count).sum::<u32>()
    }
}

/// Maximum CPI nesting the tree will represent. Real Solana caps CPI depth at a
/// handful of levels; this generous bound prevents adversarial logs (tens of
/// thousands of unclosed `invoke` lines) from building a tree so deep that
/// serializing or traversing it overflows the stack. Invocations beyond it are
/// flattened (attached at the cap, not nested deeper).
pub const MAX_DEPTH: usize = 64;

/// Build a call tree from a lexed event stream.
#[must_use]
pub fn build(events: &[LogEvent], registry: &ProgramRegistry) -> CallNode {
    let mut stack: Vec<CallNode> = vec![CallNode::root()];

    for event in events {
        match event {
            LogEvent::Invoke { program_id, depth } => {
                let node = CallNode::new(program_id.clone(), *depth, registry);
                // Cap nesting: beyond MAX_DEPTH, attach as a leaf of the deepest
                // open frame instead of opening a new one. Closes still balance
                // against the real open frames; the tree depth stays bounded.
                if stack.len() <= MAX_DEPTH {
                    stack.push(node);
                } else if let Some(top) = stack.last_mut() {
                    top.children.push(node);
                }
            }
            LogEvent::Consumed {
                program_id, used, ..
            } => {
                // Assign only on an exact program-ID match. In real Solana logs
                // a program's `consumed` line is emitted while that program is
                // the open frame, so this always matches; refusing to fall back
                // to "any open frame" prevents silent CU misattribution when
                // logs are malformed or out of order.
                if let Some(top) = stack.last_mut() {
                    if &top.program_id == program_id {
                        top.units_consumed = Some(*used);
                    }
                }
            }
            LogEvent::Success { .. } => close_top(&mut stack, NodeStatus::Success),
            LogEvent::Failed { .. } => close_top(&mut stack, NodeStatus::Failed),
            LogEvent::Log { message } => push_log(&mut stack, message.clone()),
            LogEvent::ScopeBegin { name, .. } => {
                push_log(&mut stack, format!("scope-begin: {name}"));
            }
            LogEvent::ScopeEnd { name, .. } => push_log(&mut stack, format!("scope-end: {name}")),
            LogEvent::ScopePoint { name, .. } => {
                push_log(&mut stack, format!("scope-point: {name}"));
            }
            LogEvent::Raw(line) => push_log(&mut stack, line.clone()),
        }
    }

    // Anything left open closes into the root, preserving partial trees.
    while stack.len() > 1 {
        close_top(&mut stack, NodeStatus::Unknown);
    }
    stack.pop().unwrap_or_else(CallNode::root)
}

fn close_top(stack: &mut Vec<CallNode>, status: NodeStatus) {
    if stack.len() <= 1 {
        return; // never pop the root
    }
    let mut node = stack.pop().expect("len > 1 checked");
    if node.status == NodeStatus::Unknown {
        node.status = status;
    }
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

fn push_log(stack: &mut [CallNode], message: String) {
    if let Some(top) = stack.last_mut() {
        top.logs.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::solana_logs::lex;

    fn tree_from(lines: &[&str]) -> CallNode {
        let owned: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
        let lexed = lex(&owned);
        let events: Vec<LogEvent> = lexed.events().cloned().collect();
        build(&events, &ProgramRegistry::with_builtins())
    }

    #[test]
    fn builds_nested_cpi_tree() {
        let tree = tree_from(&[
            "Program User111 invoke [1]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 3000 of 197000 compute units",
            "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
            "Program User111 consumed 12000 of 200000 compute units",
            "Program User111 success",
        ]);
        assert_eq!(tree.children.len(), 1);
        let user = &tree.children[0];
        assert_eq!(user.program_id, "User111");
        assert_eq!(user.units_consumed, Some(12000));
        assert_eq!(user.children.len(), 1);
        assert_eq!(user.children[0].label.as_deref(), Some("SPL Token"));
        assert_eq!(tree.cpi_count(), 1);
        assert_eq!(tree.max_depth(), 2);
        assert_eq!(tree.invocation_count(), 2);
    }

    #[test]
    fn deep_invoke_chain_is_capped_not_overflowing() {
        // Tens of thousands of unclosed invokes would build a tree deep enough
        // to overflow the stack on serialize/traverse; the cap flattens it.
        let lines: Vec<String> = (0..50_000)
            .map(|i| format!("Program P{i} invoke [{}]", i + 1))
            .collect();
        let lexed = lex(&lines);
        let events: Vec<LogEvent> = lexed.events().cloned().collect();
        let tree = build(&events, &ProgramRegistry::with_builtins());
        // These recursive traversals must not overflow (bounded by MAX_DEPTH).
        assert!(tree.invocation_count() >= 50_000);
        assert!(tree.max_depth() >= 1);
        // And the structure is genuinely shallow now.
        fn nesting(n: &CallNode) -> usize {
            1 + n.children.iter().map(nesting).max().unwrap_or(0)
        }
        // root + MAX_DEPTH open frames + one flattened-leaf level.
        assert!(nesting(&tree) <= MAX_DEPTH + 2);
    }

    #[test]
    fn unterminated_invoke_does_not_panic() {
        let tree = tree_from(&["Program User111 invoke [1]"]);
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].status, NodeStatus::Unknown);
    }

    #[test]
    fn consumed_attributes_only_to_matching_program() {
        // A `consumed` line for a program that is NOT the open frame must not be
        // misattributed to whatever frame happens to be on top of the stack.
        let tree = tree_from(&[
            "Program A111 invoke [1]",
            "Program B222 invoke [2]",
            "Program A111 consumed 5000 of 200000 compute units", // B is open, not A
            "Program B222 consumed 3000 of 197000 compute units",
            "Program B222 success",
            "Program A111 success",
        ]);
        let a = &tree.children[0];
        let b = &a.children[0];
        assert_eq!(b.program_id, "B222");
        // B keeps its own figure; the stray "A consumed" did not land on B.
        assert_eq!(b.units_consumed, Some(3000));
    }
}

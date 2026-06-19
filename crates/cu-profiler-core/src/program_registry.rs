//! Registry mapping program IDs (base-58 pubkeys) to human labels.
//!
//! Ships with the well-known native and SPL program IDs and is extensible from
//! configuration. Unknown programs render as `Unknown Program <pubkey>`.

use std::collections::HashMap;

/// Bidirectional-ish lookup from program ID to a display label.
#[derive(Debug, Clone, Default)]
pub struct ProgramRegistry {
    labels: HashMap<String, String>,
}

/// The built-in, well-known program labels.
const BUILTINS: &[(&str, &str)] = &[
    ("11111111111111111111111111111111", "System Program"),
    (
        "ComputeBudget111111111111111111111111111111",
        "Compute Budget Program",
    ),
    ("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", "SPL Token"),
    ("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb", "Token-2022"),
    (
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "Associated Token Account Program",
    ),
    (
        "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr",
        "Memo Program",
    ),
];

impl ProgramRegistry {
    /// A registry pre-loaded with the well-known program IDs.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut labels = HashMap::with_capacity(BUILTINS.len());
        for (id, name) in BUILTINS {
            labels.insert((*id).to_string(), (*name).to_string());
        }
        Self { labels }
    }

    /// Register or override a label for a program ID.
    pub fn insert(&mut self, program_id: impl Into<String>, label: impl Into<String>) {
        self.labels.insert(program_id.into(), label.into());
    }

    /// Merge labels from configuration, overriding any existing entries.
    pub fn extend_from<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in entries {
            self.insert(k, v);
        }
    }

    /// The known label for a program ID, if any.
    #[must_use]
    pub fn label(&self, program_id: &str) -> Option<&str> {
        self.labels.get(program_id).map(String::as_str)
    }

    /// A label suitable for display: the known name, or `Unknown Program <id>`.
    #[must_use]
    pub fn display_label(&self, program_id: &str) -> String {
        match self.label(program_id) {
            Some(name) => name.to_string(),
            None => format!("Unknown Program {program_id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_program_resolves() {
        let r = ProgramRegistry::with_builtins();
        assert_eq!(
            r.label("11111111111111111111111111111111"),
            Some("System Program")
        );
    }

    #[test]
    fn unknown_program_gets_placeholder() {
        let r = ProgramRegistry::with_builtins();
        assert_eq!(r.display_label("Zz99"), "Unknown Program Zz99");
    }

    #[test]
    fn config_can_override() {
        let mut r = ProgramRegistry::with_builtins();
        r.extend_from([("MyProg111", "My Program")]);
        assert_eq!(r.label("MyProg111"), Some("My Program"));
    }
}

//! Deterministic fingerprints used to decide whether a baseline still applies.
//!
//! Hashing uses FNV-1a: small, dependency-free, and stable across Rust versions
//! and platforms, which matters because fingerprints are persisted in baseline
//! files and compared later.

use serde::{Deserialize, Serialize};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a hash of `bytes`, rendered as lowercase hex.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    format!("{h:016x}")
}

/// FNV-1a hash of a string.
#[must_use]
pub fn hash_str(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

/// The fingerprint of the inputs that produced a measurement. If any component
/// changes, a stored baseline is considered stale for the affected reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// Hash of the scenario definition.
    pub scenario_hash: String,
    /// Hash of the account/fixture inputs.
    pub fixture_hash: String,
    /// Hash of the effective configuration.
    pub config_hash: String,
    /// Hash of the program binary, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_binary_hash: Option<String>,
}

impl Fingerprint {
    /// Build a fingerprint from the raw input strings.
    #[must_use]
    pub fn new(scenario: &str, fixture: &str, config: &str, program_binary: Option<&[u8]>) -> Self {
        Self {
            scenario_hash: hash_str(scenario),
            fixture_hash: hash_str(fixture),
            config_hash: hash_str(config),
            program_binary_hash: program_binary.map(hash_bytes),
        }
    }

    /// List the reasons `self` differs from `other` (empty means they match).
    #[must_use]
    pub fn staleness_reasons(&self, other: &Fingerprint) -> Vec<String> {
        let mut reasons = Vec::new();
        if self.scenario_hash != other.scenario_hash {
            reasons.push("scenario definition changed".to_string());
        }
        if self.fixture_hash != other.fixture_hash {
            reasons.push("fixture hash changed".to_string());
        }
        if self.config_hash != other.config_hash {
            reasons.push("config hash changed".to_string());
        }
        if self.program_binary_hash != other.program_binary_hash {
            reasons.push("program binary hash changed".to_string());
        }
        reasons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(hash_str("abc"), hash_str("abc"));
        assert_ne!(hash_str("abc"), hash_str("abd"));
    }

    #[test]
    fn detects_changed_fixture() {
        let a = Fingerprint::new("s", "fix1", "cfg", None);
        let b = Fingerprint::new("s", "fix2", "cfg", None);
        let reasons = a.staleness_reasons(&b);
        assert_eq!(reasons, vec!["fixture hash changed".to_string()]);
    }
}

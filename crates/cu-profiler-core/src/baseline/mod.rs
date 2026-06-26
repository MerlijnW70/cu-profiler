//! Baseline records and their on-disk store.
//!
//! A baseline stores not just a CU figure but the fingerprint metadata needed to
//! decide whether a later comparison is still valid.

mod compare;
mod fingerprint;

pub use compare::BaselineComparison;
pub use fingerprint::{Fingerprint, hash_bytes, hash_str};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::confidence::ConfidenceLevel;
use crate::metadata::InstrumentationMode;

/// A single scenario's recorded baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineRecord {
    /// Scenario name.
    pub scenario: String,
    /// Recorded CU.
    pub actual_units: u64,
    /// Budget at record time, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<u64>,
    /// RFC3339 timestamp, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Git commit, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Fingerprint of the inputs.
    pub fingerprint: Fingerprint,
    /// Solana/Agave crate versions, if known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solana_versions: Vec<String>,
    /// Profiler version that produced the baseline.
    pub profiler_version: String,
    /// Instrumentation mode at record time.
    pub instrumentation: InstrumentationMode,
    /// Confidence at record time.
    pub confidence: ConfidenceLevel,
    /// Whether the record has been explicitly approved.
    #[serde(default)]
    pub approved: bool,
}

/// A collection of baseline records keyed by scenario name. Serialized as a
/// stable, sorted JSON object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BaselineStore {
    /// Schema version for forward compatibility.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Records by scenario name (BTreeMap keeps output deterministic).
    #[serde(default)]
    pub records: BTreeMap<String, BaselineRecord>,
}

fn default_version() -> u32 {
    1
}

impl BaselineStore {
    /// An empty store at the current schema version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: default_version(),
            records: BTreeMap::new(),
        }
    }

    /// Insert or replace a record.
    pub fn insert(&mut self, record: BaselineRecord) {
        self.records.insert(record.scenario.clone(), record);
    }

    /// Look up a record by scenario name.
    #[must_use]
    pub fn get(&self, scenario: &str) -> Option<&BaselineRecord> {
        self.records.get(scenario)
    }

    /// Mark a scenario's record as approved. Returns `false` if absent.
    pub fn approve(&mut self, scenario: &str) -> bool {
        match self.records.get_mut(scenario) {
            Some(r) => {
                r.approved = true;
                true
            }
            None => false,
        }
    }

    /// Serialize to pretty JSON.
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parse from JSON.
    #[cfg(feature = "json")]
    pub fn from_json(s: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(s)?)
    }

    /// Load a store from a file, returning an empty store if it does not exist.
    #[cfg(feature = "json")]
    pub fn load(path: &std::path::Path) -> crate::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_json(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist the store to a file, creating parent directories.
    #[cfg(feature = "json")]
    pub fn save(&self, path: &std::path::Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, cu: u64) -> BaselineRecord {
        BaselineRecord {
            scenario: name.into(),
            actual_units: cu,
            budget: Some(100_000),
            timestamp: None,
            git_commit: None,
            fingerprint: Fingerprint::new(name, "fix", "cfg", None),
            solana_versions: Vec::new(),
            profiler_version: "0.1.0".into(),
            instrumentation: InstrumentationMode::Off,
            confidence: ConfidenceLevel::High,
            approved: false,
        }
    }

    #[test]
    fn insert_get_approve() {
        let mut store = BaselineStore::new();
        store.insert(record("swap", 95_000));
        assert_eq!(store.get("swap").map(|r| r.actual_units), Some(95_000));
        assert!(store.approve("swap"));
        assert!(store.get("swap").unwrap().approved);
        assert!(!store.approve("missing"));
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_round_trip_is_stable() {
        let mut store = BaselineStore::new();
        store.insert(record("b", 2));
        store.insert(record("a", 1));
        let json = store.to_json().unwrap();
        // BTreeMap ⇒ "a" serializes before "b".
        assert!(json.find("\"a\"").unwrap() < json.find("\"b\"").unwrap());
        let back = BaselineStore::from_json(&json).unwrap();
        assert_eq!(store, back);
    }

    #[test]
    fn new_store_is_at_schema_version_one() {
        assert_eq!(BaselineStore::new().version, 1);
    }

    #[cfg(feature = "json")]
    #[test]
    fn save_then_load_round_trips_through_disk() {
        let mut store = BaselineStore::new();
        store.insert(record("swap", 4242));
        let path = std::env::temp_dir().join("cu_profiler_baseline_roundtrip_test.json");
        let _ = std::fs::remove_file(&path);
        store.save(&path).expect("save");
        let loaded = BaselineStore::load(&path).expect("load");
        assert_eq!(loaded.get("swap").map(|r| r.actual_units), Some(4242));
        assert_eq!(loaded, store);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(feature = "json")]
    #[test]
    fn load_missing_file_returns_an_empty_store() {
        let path = std::env::temp_dir().join("cu_profiler_baseline_definitely_absent_xyzzy.json");
        let _ = std::fs::remove_file(&path);
        let store = BaselineStore::load(&path).expect("a missing baseline is not an error");
        assert!(store.records.is_empty());
    }

    #[cfg(feature = "json")]
    #[test]
    fn load_propagates_non_notfound_errors() {
        // A file that exists but is not valid UTF-8 yields an InvalidData (non-
        // NotFound) read error, which must propagate rather than be swallowed as an
        // empty store.
        let path = std::env::temp_dir().join("cu_profiler_baseline_invalid_utf8.json");
        std::fs::write(&path, [0xff, 0xfe, 0xff]).expect("write bytes");
        let result = BaselineStore::load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }
}

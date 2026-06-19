//! Optional Anchor IDL support (feature `anchor`).
//!
//! Anchor programs ship a JSON IDL. This module parses it leniently — across the
//! pre-0.30 (`name`/`metadata.address`) and 0.30+ (`address`/`metadata.name`)
//! layouts — and exposes the parts useful to a compute profiler:
//!
//! - the program's address + human name, to label it in a [`ProgramRegistry`];
//! - instruction and account names, for future instruction-level mapping;
//! - the error table, to decode `custom program error: 0x…` failure logs.
//!
//! Native Solana stays first-class: this is feature-gated and never a hard
//! dependency of the default build.

use serde::Deserialize;

use crate::Result;
use crate::error::Error;
use crate::program_registry::ProgramRegistry;

/// A parsed Anchor IDL (only the fields this tool uses; unknown fields ignored).
#[derive(Debug, Clone, Deserialize)]
pub struct AnchorIdl {
    /// Program address (Anchor 0.30+ top-level field).
    #[serde(default)]
    address: Option<String>,
    /// Program name (pre-0.30 top-level field).
    #[serde(default)]
    name: Option<String>,
    /// Metadata block (carries name/address depending on Anchor version).
    #[serde(default)]
    metadata: Option<IdlMetadata>,
    /// Instructions.
    #[serde(default)]
    instructions: Vec<IdlInstruction>,
    /// Account types.
    #[serde(default)]
    accounts: Vec<IdlNamed>,
    /// Error table.
    #[serde(default)]
    errors: Vec<IdlError>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdlMetadata {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdlInstruction {
    name: String,
    #[serde(default)]
    accounts: Vec<IdlNamed>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdlNamed {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IdlError {
    code: i64,
    name: String,
    #[serde(default)]
    msg: Option<String>,
}

/// A decoded Anchor error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorError {
    /// Numeric error code (e.g. 6000).
    pub code: i64,
    /// Error variant name.
    pub name: String,
    /// Human message, if the IDL provided one.
    pub msg: Option<String>,
}

impl AnchorIdl {
    /// Parse an IDL from JSON text.
    ///
    /// # Errors
    /// Returns [`Error::Json`] if the text is not valid IDL JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Error::from)
    }

    /// The program's on-chain address, across IDL layouts.
    #[must_use]
    pub fn program_id(&self) -> Option<&str> {
        self.address
            .as_deref()
            .or_else(|| self.metadata.as_ref().and_then(|m| m.address.as_deref()))
    }

    /// The program's human name, across IDL layouts.
    #[must_use]
    pub fn program_name(&self) -> Option<&str> {
        self.name
            .as_deref()
            .or_else(|| self.metadata.as_ref().and_then(|m| m.name.as_deref()))
    }

    /// Instruction names declared by the program.
    #[must_use]
    pub fn instruction_names(&self) -> Vec<&str> {
        self.instructions.iter().map(|i| i.name.as_str()).collect()
    }

    /// Account names for a given instruction (by instruction name).
    #[must_use]
    pub fn instruction_accounts(&self, instruction: &str) -> Vec<&str> {
        self.instructions
            .iter()
            .find(|i| i.name == instruction)
            .map(|i| i.accounts.iter().map(|a| a.name.as_str()).collect())
            .unwrap_or_default()
    }

    /// Account (state) type names.
    #[must_use]
    pub fn account_names(&self) -> Vec<&str> {
        self.accounts.iter().map(|a| a.name.as_str()).collect()
    }

    /// Look up an error by its numeric code.
    #[must_use]
    pub fn error(&self, code: i64) -> Option<AnchorError> {
        self.errors
            .iter()
            .find(|e| e.code == code)
            .map(|e| AnchorError {
                code: e.code,
                name: e.name.clone(),
                msg: e.msg.clone(),
            })
    }

    /// Decode an Anchor error from a failure reason such as
    /// `"custom program error: 0x1770"` (0x1770 = 6000).
    #[must_use]
    pub fn decode_error_reason(&self, reason: &str) -> Option<AnchorError> {
        let code = parse_custom_error_code(reason)?;
        self.error(code)
    }

    /// Add this program's `address → name` label to a registry, if both are
    /// present. Returns `true` if a label was added.
    pub fn apply_labels(&self, registry: &mut ProgramRegistry) -> bool {
        match (self.program_id(), self.program_name()) {
            (Some(id), Some(name)) => {
                registry.insert(id, name);
                true
            }
            _ => false,
        }
    }
}

/// Parse the numeric code out of a `custom program error: 0x<hex>` (or decimal)
/// reason string.
fn parse_custom_error_code(reason: &str) -> Option<i64> {
    let tail = reason.rsplit("custom program error:").next()?.trim();
    let token = tail.split_whitespace().next()?;
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()
    } else {
        token.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Anchor 0.30+ style IDL.
    const IDL_NEW: &str = r#"{
        "address": "Fg6PaFpoGXkYsidMpWxTWqSWY7zh5k8GcQ8t9oR3o9h",
        "metadata": { "name": "amm", "version": "0.1.0" },
        "instructions": [
            { "name": "swap", "accounts": [ { "name": "pool" }, { "name": "user" } ] },
            { "name": "initialize_pool", "accounts": [] }
        ],
        "accounts": [ { "name": "Pool" } ],
        "errors": [ { "code": 6000, "name": "InvalidOwner", "msg": "owner mismatch" } ]
    }"#;

    // Pre-0.30 style IDL.
    const IDL_OLD: &str = r#"{
        "version": "0.1.0",
        "name": "legacy_amm",
        "instructions": [ { "name": "swap" } ],
        "errors": [ { "code": 6001, "name": "StaleOracle" } ],
        "metadata": { "address": "Leg1111111111111111111111111111111111111111" }
    }"#;

    #[test]
    fn parses_new_layout() {
        let idl = AnchorIdl::from_json(IDL_NEW).unwrap();
        assert_eq!(
            idl.program_id(),
            Some("Fg6PaFpoGXkYsidMpWxTWqSWY7zh5k8GcQ8t9oR3o9h")
        );
        assert_eq!(idl.program_name(), Some("amm"));
        assert_eq!(idl.instruction_names(), vec!["swap", "initialize_pool"]);
        assert_eq!(idl.instruction_accounts("swap"), vec!["pool", "user"]);
        assert_eq!(idl.account_names(), vec!["Pool"]);
    }

    #[test]
    fn parses_old_layout() {
        let idl = AnchorIdl::from_json(IDL_OLD).unwrap();
        assert_eq!(
            idl.program_id(),
            Some("Leg1111111111111111111111111111111111111111")
        );
        assert_eq!(idl.program_name(), Some("legacy_amm"));
    }

    #[test]
    fn applies_program_label_to_registry() {
        let idl = AnchorIdl::from_json(IDL_NEW).unwrap();
        let mut registry = ProgramRegistry::with_builtins();
        assert!(idl.apply_labels(&mut registry));
        assert_eq!(
            registry.label("Fg6PaFpoGXkYsidMpWxTWqSWY7zh5k8GcQ8t9oR3o9h"),
            Some("amm")
        );
    }

    #[test]
    fn decodes_hex_custom_error() {
        let idl = AnchorIdl::from_json(IDL_NEW).unwrap();
        let err = idl
            .decode_error_reason("custom program error: 0x1770")
            .unwrap();
        assert_eq!(err.code, 6000);
        assert_eq!(err.name, "InvalidOwner");
        assert_eq!(err.msg.as_deref(), Some("owner mismatch"));
    }

    #[test]
    fn decodes_decimal_custom_error() {
        let idl = AnchorIdl::from_json(IDL_OLD).unwrap();
        let err = idl
            .decode_error_reason("custom program error: 6001")
            .unwrap();
        assert_eq!(err.name, "StaleOracle");
    }

    #[test]
    fn unknown_error_code_is_none() {
        let idl = AnchorIdl::from_json(IDL_NEW).unwrap();
        assert!(
            idl.decode_error_reason("custom program error: 0x9999")
                .is_none()
        );
    }

    #[test]
    fn malformed_idl_errors() {
        assert!(AnchorIdl::from_json("{ not json").is_err());
    }
}

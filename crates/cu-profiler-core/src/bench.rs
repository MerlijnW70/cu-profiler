//! Declarative fixtures for the turnkey real-CU `bench` path.
//!
//! A [`BenchPlan`] describes, as plain data, the instruction(s) to execute against
//! a compiled Solana program so a live backend (Mollusk) can measure real compute
//! units — no hand-written Rust harness required. This module owns only the
//! **schema, parsing and validation**; it pulls in no Solana crates and runs
//! everywhere the core does (including Windows). Converting a validated plan into
//! `solana-instruction`/`solana-account` types and executing it lives in the
//! Linux-only `cu-profiler-mollusk` integration crate.
//!
//! ```toml
//! # bench.toml
//! [[instruction]]
//! scenario   = "swap_exact_in"
//! program_id = "SwapPRogram1111111111111111111111111111"
//! data       = "01ab"          # hex-encoded instruction data
//!
//!   [[instruction.account]]
//!   pubkey   = "11111111111111111111111111111111"
//!   signer   = true
//!   writable = true
//!   lamports = 1000000
//! ```

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A set of instruction fixtures to benchmark, parsed from a `bench.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BenchPlan {
    /// One entry per instruction to execute and measure.
    #[serde(default, rename = "instruction")]
    pub instructions: Vec<InstructionFixture>,
}

/// A single instruction to execute against the program under test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionFixture {
    /// The scenario name this instruction measures (keys it to a `[scenario.<name>]`).
    pub scenario: String,
    /// The program's base58 address.
    pub program_id: String,
    /// Hex-encoded instruction data (empty string for a no-arg instruction).
    #[serde(default)]
    pub data: String,
    /// Accounts passed to the instruction, in order.
    #[serde(default, rename = "account")]
    pub accounts: Vec<AccountFixture>,
}

/// One account in an instruction's account list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountFixture {
    /// The account's base58 address.
    pub pubkey: String,
    /// Whether the account signs the transaction.
    #[serde(default)]
    pub signer: bool,
    /// Whether the instruction may write to the account.
    #[serde(default)]
    pub writable: bool,
    /// Starting lamport balance.
    #[serde(default)]
    pub lamports: u64,
    /// Owning program (base58), if the account should be pre-owned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Hex-encoded initial account data, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl BenchPlan {
    /// Parse and validate a plan from TOML.
    ///
    /// # Errors
    /// Returns [`Error::Config`] for malformed TOML, an unknown key, a non-base58
    /// program/account address, or non-hex instruction/account data.
    pub fn from_toml(s: &str) -> Result<Self> {
        let plan: BenchPlan = toml::from_str(s).map_err(|e| Error::Config(e.to_string()))?;
        plan.validate()?;
        Ok(plan)
    }

    /// Validate every fixture's addresses and encodings.
    ///
    /// # Errors
    /// Returns [`Error::Config`] describing the first invalid field found.
    pub fn validate(&self) -> Result<()> {
        if self.instructions.is_empty() {
            return Err(Error::Config(
                "bench plan has no `[[instruction]]` entries".to_string(),
            ));
        }
        for ix in &self.instructions {
            ix.validate()?;
        }
        Ok(())
    }
}

impl InstructionFixture {
    fn validate(&self) -> Result<()> {
        let ctx = format!("instruction `{}`", self.scenario);
        if self.scenario.is_empty() {
            return Err(Error::Config(
                "an instruction has an empty `scenario`".to_string(),
            ));
        }
        validate_base58(&self.program_id, &format!("{ctx}: program_id"))?;
        validate_hex(&self.data, &format!("{ctx}: data"))?;
        for acc in &self.accounts {
            validate_base58(&acc.pubkey, &format!("{ctx}: account pubkey"))?;
            if let Some(owner) = &acc.owner {
                validate_base58(owner, &format!("{ctx}: account owner"))?;
            }
            if let Some(data) = &acc.data {
                validate_hex(data, &format!("{ctx}: account data"))?;
            }
        }
        Ok(())
    }
}

/// The base58 alphabet Solana uses (Bitcoin alphabet: no `0`, `O`, `I`, `l`).
const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Validate that `s` looks like a base58-encoded 32-byte Solana address.
///
/// This checks the alphabet and the length window a 32-byte value encodes to
/// (32–44 characters); it does not decode (which would need a base58 dependency).
fn validate_base58(s: &str, what: &str) -> Result<()> {
    if !(32..=44).contains(&s.len()) {
        return Err(Error::Config(format!(
            "{what}: `{s}` is not a 32-byte base58 address (length {})",
            s.len()
        )));
    }
    if let Some(bad) = s.bytes().find(|b| !BASE58_ALPHABET.contains(b)) {
        return Err(Error::Config(format!(
            "{what}: `{s}` contains a non-base58 character `{}`",
            bad as char
        )));
    }
    Ok(())
}

/// Validate that `s` is valid hex (even length, hex digits only). Empty is allowed.
fn validate_hex(s: &str, what: &str) -> Result<()> {
    if s.len() % 2 != 0 {
        return Err(Error::Config(format!(
            "{what}: hex string has an odd length ({})",
            s.len()
        )));
    }
    if let Some(bad) = s.bytes().find(|b| !b.is_ascii_hexdigit()) {
        return Err(Error::Config(format!(
            "{what}: `{s}` contains a non-hex character `{}`",
            bad as char
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS: &str = "11111111111111111111111111111111";

    fn plan_toml() -> String {
        format!(
            "[[instruction]]\nscenario=\"swap\"\nprogram_id=\"{SYS}\"\ndata=\"01ab\"\n\
             [[instruction.account]]\npubkey=\"{SYS}\"\nsigner=true\nwritable=true\nlamports=1000000\n"
        )
    }

    #[test]
    fn parses_and_validates_a_plan() {
        let plan = BenchPlan::from_toml(&plan_toml()).unwrap();
        assert_eq!(plan.instructions.len(), 1);
        let ix = &plan.instructions[0];
        assert_eq!(ix.scenario, "swap");
        assert_eq!(ix.data, "01ab");
        assert_eq!(ix.accounts.len(), 1);
        assert!(ix.accounts[0].signer && ix.accounts[0].writable);
    }

    #[test]
    fn empty_plan_is_rejected() {
        assert!(BenchPlan::from_toml("").is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        let toml = format!("[[instruction]]\nscenario=\"s\"\nprogram_id=\"{SYS}\"\nbogus=1\n");
        assert!(BenchPlan::from_toml(&toml).is_err());
    }

    #[test]
    fn rejects_bad_base58_program_id() {
        let toml = "[[instruction]]\nscenario=\"s\"\nprogram_id=\"not-base58-0OIl\"\n";
        let err = BenchPlan::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("base58"), "{err}");
    }

    #[test]
    fn rejects_odd_and_nonhex_data() {
        let odd = format!("[[instruction]]\nscenario=\"s\"\nprogram_id=\"{SYS}\"\ndata=\"abc\"\n");
        assert!(
            BenchPlan::from_toml(&odd)
                .unwrap_err()
                .to_string()
                .contains("odd")
        );
        let nonhex =
            format!("[[instruction]]\nscenario=\"s\"\nprogram_id=\"{SYS}\"\ndata=\"zz\"\n");
        assert!(
            BenchPlan::from_toml(&nonhex)
                .unwrap_err()
                .to_string()
                .contains("non-hex")
        );
    }
}

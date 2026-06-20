//! `cu-profiler import` — turn a real transaction's logs into a scenario log.
//!
//! Two sources, both producing the *real* `logMessages` cu-profiler parses:
//! - a `getTransaction` **JSON file** (`cu-profiler import tx.json`), or
//! - a **signature** fetched live from an RPC (`cu-profiler import --signature
//!   <sig> [--rpc <url>]`), using a rustls TLS stack (no OpenSSL).
//!
//! Either way there is no mock or placeholder data: the logs are exactly what
//! the cluster recorded for that transaction.

use cu_profiler_core::Result;
use cu_profiler_core::error::Error;
use serde_json::Value;

use crate::args::ImportArgs;
use crate::commands::{MAX_LOG_BYTES, read_to_string_capped, validate_log_name};
use crate::exit::ExitCode;

/// The default public RPC (heavily rate-limited — users should pass their own).
const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";

/// Maximum RPC response body we will read into memory (a `getTransaction`
/// response is kilobytes; this guards against a hostile RPC returning gigabytes).
#[cfg(feature = "remote")]
const MAX_RPC_BYTES: u64 = 32 * 1024 * 1024;

/// Execute the `import` command.
pub fn run(args: &ImportArgs, quiet: bool) -> Result<ExitCode> {
    let (logs, default_name) = match (&args.file, &args.signature) {
        (Some(file), None) => (logs_from_file(file)?, file_stem_name(file)?),
        (None, Some(signature)) => {
            if !quiet && args.rpc == DEFAULT_RPC {
                eprintln!(
                    "note: `{DEFAULT_RPC}` is rate-limited; pass `--rpc <your-endpoint>` for reliable fetches."
                );
            }
            (
                fetch_logs(&args.rpc, signature, &args.commitment)?,
                signature_name(signature),
            )
        }
        // clap's `source` group guarantees exactly one; stay panic-free anyway.
        _ => {
            return Err(Error::Config(
                "provide exactly one of <file> or --signature".to_string(),
            ));
        }
    };

    if logs.is_empty() {
        return Err(Error::Simulation(
            "the transaction produced no log messages — nothing to import".to_string(),
        ));
    }

    let name = args.name.clone().unwrap_or(default_name);
    // Reject path-traversal names before the name becomes a file path.
    validate_log_name(&name)?;
    let out = args.logs_dir.join(format!("{name}.log"));
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut body = logs.join("\n");
    body.push('\n');
    std::fs::write(&out, body)?;

    if !quiet {
        println!("imported {} log line(s) -> {}", logs.len(), out.display());
        println!(
            "next: add `[scenario.{name}]` to your config, then `cu-profiler run --scenario {name}`"
        );
    }
    Ok(ExitCode::Success)
}

/// Read logs from a `getTransaction` JSON file.
fn logs_from_file(file: &std::path::Path) -> Result<Vec<String>> {
    let text = read_to_string_capped(file, MAX_LOG_BYTES)?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| Error::Config(format!("`{}` is not valid JSON: {e}", file.display())))?;
    find_log_messages(&value).ok_or_else(|| {
        Error::Config(format!(
            "no `logMessages` array found in `{}` — expected a Solana getTransaction response",
            file.display()
        ))
    })
}

/// Scenario name from a file's stem.
fn file_stem_name(file: &std::path::Path) -> Result<String> {
    file.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::Config(format!(
                "cannot derive a scenario name from `{}`; pass --name",
                file.display()
            ))
        })
}

/// Scenario name from a signature: `tx_<first 8 chars>` (base58 is filename-safe).
fn signature_name(signature: &str) -> String {
    let head: String = signature.chars().take(8).collect();
    format!("tx_{head}")
}

/// Recursively locate the first non-empty `logMessages` string array, so any
/// envelope (`result.meta.logMessages`, a bare tx object, a CLI dump) works.
fn find_log_messages(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(arr)) = map.get("logMessages") {
                let logs: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                if !logs.is_empty() {
                    return Some(logs);
                }
            }
            map.values().find_map(find_log_messages)
        }
        Value::Array(arr) => arr.iter().find_map(find_log_messages),
        _ => None,
    }
}

/// Build the `getTransaction` JSON-RPC request body.
#[cfg(feature = "remote")]
fn build_rpc_request(signature: &str, commitment: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getTransaction",
        "params": [
            signature,
            { "encoding": "json", "commitment": commitment, "maxSupportedTransactionVersion": 0 }
        ]
    })
}

/// Turn a `getTransaction` JSON-RPC response into log lines, or a clear error
/// for the RPC-error and not-found cases. Pure (no I/O), so it is unit-tested.
#[cfg(feature = "remote")]
fn logs_from_response(response: &Value, signature: &str, rpc: &str) -> Result<Vec<String>> {
    if let Some(err) = response.get("error") {
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| err.to_string(), str::to_string);
        return Err(Error::Simulation(format!(
            "RPC error from `{rpc}`: {message}"
        )));
    }
    match response.get("result") {
        None | Some(Value::Null) => Err(Error::Simulation(format!(
            "transaction `{signature}` not found at `{rpc}` (try --commitment finalized or a different RPC)"
        ))),
        Some(result) => find_log_messages(result).ok_or_else(|| {
            Error::Simulation(format!("transaction `{signature}` returned no logMessages"))
        }),
    }
}

/// Fetch a transaction's logs from an RPC over rustls.
#[cfg(feature = "remote")]
fn fetch_logs(rpc: &str, signature: &str, commitment: &str) -> Result<Vec<String>> {
    use std::time::Duration;

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    let agent: ureq::Agent = config.into();

    let body = build_rpc_request(signature, commitment);
    let mut response = agent
        .post(rpc)
        .send_json(&body)
        .map_err(|e| Error::Simulation(format!("RPC request to `{rpc}` failed: {e}")))?;
    let value: Value = response
        .body_mut()
        .with_config()
        .limit(MAX_RPC_BYTES)
        .read_json::<Value>()
        .map_err(|e| Error::Simulation(format!("invalid RPC response from `{rpc}`: {e}")))?;

    logs_from_response(&value, signature, rpc)
}

/// Without the `remote` feature, `--signature` is a clear configuration error.
#[cfg(not(feature = "remote"))]
fn fetch_logs(_rpc: &str, _signature: &str, _commitment: &str) -> Result<Vec<String>> {
    Err(Error::Config(
        "`--signature` requires the `remote` feature (on by default); rebuild with \
         `--features remote`, or import a `getTransaction` JSON file instead"
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn finds_logs_in_rpc_envelope() {
        let v: Value = serde_json::from_str(
            r#"{"result":{"meta":{"logMessages":["Program P invoke [1]","Program P success"]}}}"#,
        )
        .unwrap();
        let logs = find_log_messages(&v).unwrap();
        assert_eq!(logs, vec!["Program P invoke [1]", "Program P success"]);
    }

    #[test]
    fn finds_logs_when_nested_in_array() {
        let v: Value =
            serde_json::from_str(r#"[{"meta":{"logMessages":["Program X success"]}}]"#).unwrap();
        assert_eq!(find_log_messages(&v).unwrap(), vec!["Program X success"]);
    }

    #[test]
    fn none_when_absent() {
        let v: Value = serde_json::from_str(r#"{"meta":{"err":null}}"#).unwrap();
        assert!(find_log_messages(&v).is_none());
    }

    #[test]
    fn name_defaults_to_file_stem() {
        assert_eq!(
            file_stem_name(Path::new("/tmp/okx_swap.json")).unwrap(),
            "okx_swap"
        );
    }

    #[test]
    fn signature_name_is_short_and_safe() {
        assert_eq!(signature_name("4ReKprwf3WdLHRrzp4ctPWNBsQ"), "tx_4ReKprwf");
        assert_eq!(signature_name("abc"), "tx_abc");
    }

    #[cfg(feature = "remote")]
    #[test]
    fn rpc_request_has_correct_shape() {
        let body = build_rpc_request("SIG", "finalized");
        assert_eq!(body["method"], "getTransaction");
        assert_eq!(body["params"][0], "SIG");
        assert_eq!(body["params"][1]["commitment"], "finalized");
        assert_eq!(body["params"][1]["encoding"], "json");
        assert_eq!(body["params"][1]["maxSupportedTransactionVersion"], 0);
    }

    #[cfg(feature = "remote")]
    #[test]
    fn response_success_extracts_logs() {
        let v: Value = serde_json::from_str(
            r#"{"result":{"meta":{"logMessages":["Program A invoke [1]","Program A consumed 10 of 200000 compute units","Program A success"]}}}"#,
        )
        .unwrap();
        let logs = logs_from_response(&v, "SIG", "rpc").unwrap();
        assert_eq!(logs.len(), 3);
    }

    #[cfg(feature = "remote")]
    #[test]
    fn response_null_result_is_not_found() {
        let v: Value = serde_json::from_str(r#"{"result":null}"#).unwrap();
        let err = logs_from_response(&v, "SIG", "rpc").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[cfg(feature = "remote")]
    #[test]
    fn response_rpc_error_is_surfaced() {
        let v: Value =
            serde_json::from_str(r#"{"error":{"code":-32602,"message":"Invalid param"}}"#).unwrap();
        let err = logs_from_response(&v, "SIG", "rpc").unwrap_err();
        assert!(err.to_string().contains("Invalid param"));
    }
}

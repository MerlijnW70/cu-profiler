//! `cu-profiler import <tx.json>` — turn a real transaction's logs into a
//! scenario log file.
//!
//! Solana's `getTransaction` (CLI `--output json`, or an RPC response) carries
//! the executed transaction's `logMessages` — the exact `Program … consumed …
//! compute units` lines cu-profiler parses. This command extracts that array
//! (wherever it is nested) and writes it to `<logs-dir>/<name>.log`, so a real
//! on-chain transaction can be profiled with `cu-profiler run` — no live RPC or
//! Solana toolchain required.

use cu_profiler_core::Result;
use cu_profiler_core::error::Error;
use serde_json::Value;

use crate::args::ImportArgs;
use crate::exit::ExitCode;

/// Execute the `import` command.
pub fn run(args: &ImportArgs, quiet: bool) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.file)
        .map_err(|e| Error::Config(format!("cannot read `{}`: {e}", args.file.display())))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| Error::Config(format!("`{}` is not valid JSON: {e}", args.file.display())))?;

    let logs = find_log_messages(&value).ok_or_else(|| {
        Error::Config(format!(
            "no `logMessages` array found in `{}` — expected a Solana getTransaction response",
            args.file.display()
        ))
    })?;
    if logs.is_empty() {
        return Err(Error::Config(format!(
            "`logMessages` in `{}` was empty — nothing to import",
            args.file.display()
        )));
    }

    let name = resolve_name(args)?;
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

/// The scenario name: `--name`, else the input file's stem.
fn resolve_name(args: &ImportArgs) -> Result<String> {
    if let Some(name) = &args.name {
        return Ok(name.clone());
    }
    args.file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::Config(format!(
                "cannot derive a scenario name from `{}`; pass --name",
                args.file.display()
            ))
        })
}

/// Recursively locate the first non-empty `logMessages` string array, so the
/// command accepts the RPC envelope (`{ "result": { "meta": { "logMessages" }}}`),
/// a bare transaction object, or a CLI `--output json` dump alike.
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
        let args = ImportArgs {
            file: Path::new("/tmp/okx_swap.json").to_path_buf(),
            name: None,
            logs_dir: Path::new(".cu/logs").to_path_buf(),
        };
        assert_eq!(resolve_name(&args).unwrap(), "okx_swap");
    }
}

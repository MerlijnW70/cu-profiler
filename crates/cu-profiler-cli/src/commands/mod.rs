//! Command implementations. Each is a thin adapter: parse inputs, call
//! `cu-profiler-core`, render with `cu-profiler-report`, choose an exit code.

mod baseline;
mod ci;
mod comment;
mod compare;
mod explain;
mod import;
mod init;
mod inspect;
mod run;

pub use baseline::{approve as baseline_approve, save as baseline_save};
pub use ci::run as ci;
pub use comment::run as comment;
pub use compare::run as compare;
pub use explain::run as explain;
pub use import::run as import;
pub use init::run as init;
pub use inspect::run as inspect;
pub use run::run;

use std::path::{Component, Path};

/// Maximum size of a log file we will read into memory (guards against OOM on a
/// hostile or runaway log). 64 MiB is far above any real transaction's logs.
pub(crate) const MAX_LOG_BYTES: u64 = 64 * 1024 * 1024;

/// Reject a scenario/log name that would escape the logs directory. Hierarchical
/// names (`swap/happy_path`) are allowed; `..`, absolute/root paths, backslashes
/// and NUL are not — they would turn `logs_dir.join("{name}.log")` into an
/// arbitrary-path read or write.
pub(crate) fn validate_log_name(name: &str) -> Result<()> {
    let safe = !name.is_empty()
        && !name.contains('\0')
        && !name.contains('\\')
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "scenario/log name `{name}` is invalid — it must be a relative name without `..`, \
             leading `/`, backslashes or NUL"
        )))
    }
}

/// Read a file into a string, refusing anything larger than `max_bytes`.
pub(crate) fn read_to_string_capped(path: &Path, max_bytes: u64) -> Result<String> {
    let len = std::fs::metadata(path)
        .map_err(|e| Error::Config(format!("cannot stat `{}`: {e}", path.display())))?
        .len();
    if len > max_bytes {
        return Err(Error::Config(format!(
            "`{}` is {len} bytes, over the {max_bytes}-byte limit",
            path.display()
        )));
    }
    std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("cannot read `{}`: {e}", path.display())))
}

use cu_profiler_core::baseline::BaselineStore;
use cu_profiler_core::config::Config;
use cu_profiler_core::metadata::RunMetadata;
use cu_profiler_core::model::Report;
use cu_profiler_core::profiler::Profiler;
use cu_profiler_core::program_registry::ProgramRegistry;
use cu_profiler_core::scenario::Scenario;
use cu_profiler_core::{Error, Result};
use cu_profiler_report::Format;

use crate::args::CommonRun;

/// A loaded configuration together with its raw text (hashed into fingerprints).
struct Loaded {
    config: Config,
    config_text: String,
}

fn load_config(path: &Path) -> Result<Loaded> {
    let config_text = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("cannot read config `{}`: {e}", path.display())))?;
    let config = Config::from_toml(&config_text)?;
    Ok(Loaded {
        config,
        config_text,
    })
}

fn build_registry(config: &Config) -> ProgramRegistry {
    let mut registry = ProgramRegistry::with_builtins();
    registry.extend_from(
        config
            .program_labels
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    registry
}

/// Label the program from its Anchor IDL when `[anchor] idl` is configured and
/// the crate was built with the `anchor` feature. A no-op otherwise.
#[cfg(feature = "anchor")]
fn apply_anchor_idl(config: &Config, registry: &mut ProgramRegistry) -> Result<()> {
    use cu_profiler_core::anchor::AnchorIdl;
    if let Some(path) = &config.anchor.idl {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::Config(format!("cannot read Anchor IDL `{}`: {e}", path.display()))
        })?;
        AnchorIdl::from_json(&text)?.apply_labels(registry);
    }
    Ok(())
}

#[cfg(not(feature = "anchor"))]
fn apply_anchor_idl(config: &Config, _registry: &mut ProgramRegistry) -> Result<()> {
    if config.anchor.idl.is_some() {
        eprintln!(
            "note: `[anchor] idl` is set but this build lacks the `anchor` feature; ignoring it"
        );
    }
    Ok(())
}

/// Marker written into `init`'s scaffolded example logs.
const DEMO_MARKER: &str = "DEMO_DATA_ONLY";

/// True if any selected scenario's log file is the scaffolded demo fixture (so a
/// run can warn that its output is not a real measurement).
fn any_demo_logs(scenarios: &[Scenario], logs_dir: &Path) -> bool {
    use std::io::{BufRead, BufReader};
    scenarios.iter().any(|s| {
        let path = logs_dir.join(format!("{}.log", s.name));
        // The marker is always the first line — read only that, never the whole
        // (possibly large) file.
        std::fs::File::open(&path)
            .ok()
            .and_then(|f| BufReader::new(f).lines().next()?.ok())
            .is_some_and(|line| line.contains(DEMO_MARKER))
    })
}

/// Print the demo-data warning to **stderr** (so it never corrupts JSON/JUnit or
/// `--output` files, which go to stdout). No-op under `--quiet`.
fn warn_if_demo(scenarios: &[Scenario], logs_dir: &Path, quiet: bool) {
    if quiet || !any_demo_logs(scenarios, logs_dir) {
        return;
    }
    eprintln!(
        "\u{26a0} WARNING: profiling scaffolded DEMO fixture data — these numbers are NOT a real measurement."
    );
    eprintln!(
        "  Replace the files in {} with your own program logs, or use the Mollusk backend",
        logs_dir.display()
    );
    eprintln!("  for live SBF compute-unit metering. See the docs.");
    eprintln!("  ------------------------------------------------------------");
}

/// Warn (stderr) when the config asks for a live `mode` the CLI doesn't execute
/// (the CLI profiles recorded logs; live backends are library-only).
fn warn_if_live_mode(config: &Config, quiet: bool) {
    if quiet || config.mode_is_recorded() {
        return;
    }
    eprintln!(
        "note: project mode = `{}` is not executed by the CLI, which profiles recorded logs.",
        config.project.mode
    );
    eprintln!(
        "  For live execution use the integration backends (cu-profiler-mollusk / cu-profiler-program-test),"
    );
    eprintln!("  or `cu-profiler import <tx.json>` to profile a real transaction's logs.");
}

/// Filter the configured scenarios by `--scenario` / `--tag`, applying the
/// `--samples` override when given.
fn select_scenarios(config: &Config, common: &CommonRun) -> Vec<Scenario> {
    config
        .scenarios()
        .into_iter()
        .filter(|s| common.scenarios.is_empty() || common.scenarios.contains(&s.name))
        .filter(|s| common.tags.is_empty() || common.tags.iter().any(|t| s.has_tag(t)))
        .map(|mut s| {
            if let Some(n) = common.samples {
                s.samples = n.max(1);
            }
            s
        })
        .collect()
}

/// Build a recorded-logs backend by reading `<logs_dir>/<scenario>.log`.
///
/// Missing files are *not* fatal here: the profiler turns an unrunnable scenario
/// into an `Unknown` status, which the exit-code logic maps to a simulation
/// failure. This keeps one missing fixture from masking the rest of the run.
fn build_backend(
    scenarios: &[Scenario],
    logs_dir: &Path,
) -> cu_profiler_core::backend::RecordedLogsBackend {
    use cu_profiler_core::backend::RecordedLogsBackend;
    let mut backend = RecordedLogsBackend::new();
    for s in scenarios {
        // Names are validated in `profile()` before we get here, so the join
        // cannot escape `logs_dir`.
        let path = logs_dir.join(format!("{}.log", s.name));
        if !path.exists() {
            continue;
        }
        match read_to_string_capped(&path, MAX_LOG_BYTES) {
            // Don't guess success from a substring — the parser detects failure
            // from structured `Program <id> failed` lines. We pass `true` and let
            // `analyze` lower it.
            Ok(text) => backend.insert_blob(s.name.clone(), &text, true),
            Err(e) => eprintln!("warning: skipping `{}`: {e}", path.display()),
        }
    }
    backend
}

/// Run all selected scenarios, optionally comparing against a baseline file.
fn profile(
    loaded: &Loaded,
    common: &CommonRun,
    baseline_path: Option<&Path>,
) -> Result<(Report, Vec<Scenario>, Option<BaselineStore>)> {
    let scenarios = select_scenarios(&loaded.config, common);
    if scenarios.is_empty() {
        return Err(Error::Config(
            "no scenarios matched (check config, --scenario and --tag)".to_string(),
        ));
    }
    // Reject path-traversal scenario names before any name becomes a file path.
    for s in &scenarios {
        validate_log_name(&s.name)?;
    }
    let mut registry = build_registry(&loaded.config);
    apply_anchor_idl(&loaded.config, &mut registry)?;
    let backend = build_backend(&scenarios, &common.logs_dir);

    // When a baseline was explicitly requested, a missing file is a real error
    // (exit code 4 — "stale or missing baseline"), not an empty comparison that
    // silently passes.
    let baseline = match baseline_path {
        Some(p) if !p.exists() => {
            return Err(Error::Baseline(format!(
                "baseline file `{}` not found — run `cu-profiler baseline save` first",
                p.display()
            )));
        }
        Some(p) => Some(BaselineStore::load(p)?),
        None => None,
    };

    let profiler = Profiler::new()
        .with_registry(registry)
        .with_config_repr(loaded.config_text.clone());
    let metadata = RunMetadata::recorded(cu_profiler_core::VERSION);
    let report = profiler.run(&backend, &scenarios, baseline.as_ref(), metadata);
    Ok((report, scenarios, baseline))
}

/// Resolve the effective output format from a flag or the config default.
fn resolve_format(config: &Config, flag: Option<&str>) -> Result<Format> {
    match flag {
        Some(f) => f.parse(),
        None => config.output.default_format.parse(),
    }
}

/// Write rendered output to a path (creating parents) or to stdout.
fn emit(rendered: &str, output: Option<&Path>, quiet: bool) -> Result<()> {
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(path, rendered)?;
            if !quiet {
                println!("wrote {}", path.display());
            }
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_log_name_accepts_safe_and_hierarchical() {
        for ok in ["swap_exact_in", "swap/happy_path", "tx_4ReKprwf", "a.b-c"] {
            assert!(validate_log_name(ok).is_ok(), "should accept `{ok}`");
        }
    }

    #[test]
    fn validate_log_name_rejects_traversal_and_absolute() {
        for bad in [
            "../x",
            "../../etc/passwd",
            "/etc/passwd",
            r"a\b",
            "",
            "a\0b",
            "..",
        ] {
            assert!(validate_log_name(bad).is_err(), "should reject `{bad:?}`");
        }
    }

    #[test]
    fn read_to_string_capped_enforces_limit() {
        let path = std::env::temp_dir().join(format!("cu-cap-{}.txt", std::process::id()));
        std::fs::write(&path, b"0123456789").unwrap();
        assert!(read_to_string_capped(&path, 100).is_ok());
        let err = read_to_string_capped(&path, 4).unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
        let _ = std::fs::remove_file(&path);
    }
}

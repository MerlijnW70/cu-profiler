//! End-to-end CLI tests: invoke the built `cu-profiler` binary in a scratch
//! directory and assert on its stdout and exit codes.

use std::path::PathBuf;
use std::process::Command;

/// Path to the compiled binary, provided by Cargo to integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_cu-profiler");

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cu-profiler-it-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("binary runs")
}

#[test]
fn init_then_run_reports_table_and_exits_zero() {
    let dir = scratch_dir("run");
    let init = run(&dir, &["init"]);
    assert!(init.status.success(), "init failed: {init:?}");

    let out = run(&dir, &["run"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("swap_exact_in"), "stdout: {stdout}");
    assert!(stdout.contains("Status"));
}

#[test]
fn baseline_save_then_regression_compare_exits_one() {
    let dir = scratch_dir("regress");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["baseline", "save"]).status.success());

    // Introduce a regression by rewriting the swap log above budget.
    let log = dir.join(".cu/logs/swap_exact_in.log");
    let text = std::fs::read_to_string(&log)
        .unwrap()
        .replace("96812", "120000");
    std::fs::write(&log, text).unwrap();

    let out = run(&dir, &["compare"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected budget/regression exit code 1"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("FAIL"));
}

#[test]
fn compare_without_baseline_is_missing_baseline_exit_four() {
    let dir = scratch_dir("nobaseline");
    assert!(run(&dir, &["init"]).status.success());
    // No `baseline save` was run, so .cu/baseline.json does not exist.
    let out = run(&dir, &["compare"]);
    assert_eq!(
        out.status.code(),
        Some(4),
        "expected missing-baseline exit code 4"
    );
}

#[test]
fn missing_config_is_config_error_exit_two() {
    let dir = scratch_dir("noconfig");
    let out = run(&dir, &["run"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected config error exit code 2"
    );
}

#[test]
fn json_output_then_inspect_round_trips() {
    let dir = scratch_dir("inspect");
    assert!(run(&dir, &["init"]).status.success());
    let out = run(
        &dir,
        &["run", "--format", "json", "--output", "report.json"],
    );
    assert!(out.status.success());

    let inspect = run(&dir, &["inspect", "report.json"]);
    assert!(inspect.status.success());
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("swap_exact_in"));
}

#[cfg(feature = "anchor")]
#[test]
fn anchor_idl_labels_program_in_report() {
    let dir = scratch_dir("anchor");
    assert!(run(&dir, &["init"]).status.success());

    // IDL whose address matches the program ID in the example swap log.
    std::fs::write(
        dir.join("amm.idl.json"),
        r#"{"address":"SwapPRogram1111111111111111111111111111","metadata":{"name":"amm"},"instructions":[],"errors":[]}"#,
    )
    .unwrap();

    // Point the config at the IDL.
    let cfg = dir.join("cu-profiler.toml");
    let mut text = std::fs::read_to_string(&cfg).unwrap();
    text.push_str("\n[anchor]\nidl = \"amm.idl.json\"\n");
    std::fs::write(&cfg, text).unwrap();

    let out = run(
        &dir,
        &["run", "--scenario", "swap_exact_in", "--format", "json"],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"label\": \"amm\""),
        "IDL label missing:\n{stdout}"
    );
}

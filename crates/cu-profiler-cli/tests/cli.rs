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

#[test]
fn demo_fixtures_warn_on_stderr_not_stdout() {
    let dir = scratch_dir("demo-warn");
    assert!(run(&dir, &["init"]).status.success());

    let out = run(&dir, &["run"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stderr.contains("DEMO"),
        "expected demo warning on stderr: {stderr}"
    );
    // The report (stdout) must stay clean — no warning leaking into machine output.
    assert!(
        !stdout.contains("DEMO"),
        "warning leaked into stdout: {stdout}"
    );
}

#[test]
fn real_logs_emit_no_demo_warning() {
    let dir = scratch_dir("real-nowarn");
    assert!(run(&dir, &["init"]).status.success());
    // Replace a scaffolded log with a real (unmarked) one.
    std::fs::write(
        dir.join(".cu/logs/swap_exact_in.log"),
        "Program P invoke [1]\nProgram P consumed 1234 of 200000 compute units\nProgram P success\n",
    )
    .unwrap();
    let out = run(&dir, &["run", "--scenario", "swap_exact_in"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("DEMO"),
        "unexpected demo warning for real logs: {stderr}"
    );
}

#[test]
fn live_mode_warns_but_still_runs() {
    let dir = scratch_dir("livemode");
    assert!(run(&dir, &["init"]).status.success());
    let cfg = dir.join("cu-profiler.toml");
    let text = std::fs::read_to_string(&cfg)
        .unwrap()
        .replace("mode = \"recorded\"", "mode = \"program-test\"");
    std::fs::write(&cfg, text).unwrap();

    let out = run(&dir, &["run"]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("program-test") && stderr.contains("not executed by the CLI"),
        "expected live-mode note on stderr: {stderr}"
    );
    // The note must not leak into the report (stdout).
    assert!(!String::from_utf8_lossy(&out.stdout).contains("not executed"));
}

#[test]
fn import_real_tx_json_then_run_measures_it() {
    let dir = scratch_dir("import");
    assert!(run(&dir, &["init"]).status.success());

    // A getTransaction-shaped JSON with real-looking logMessages (nested under
    // result.meta, like an RPC response).
    let tx = r#"{"result":{"meta":{"logMessages":[
        "Program Vote111 invoke [1]",
        "Program Vote111 consumed 4321 of 200000 compute units",
        "Program Vote111 success"
    ]}}}"#;
    std::fs::write(dir.join("tx.json"), tx).unwrap();

    let imp = run(&dir, &["import", "tx.json", "--name", "real_vote"]);
    assert!(imp.status.success(), "import failed: {imp:?}");
    assert!(dir.join(".cu/logs/real_vote.log").exists());

    // Point a scenario at the imported log and run it.
    let cfg = dir.join("cu-profiler.toml");
    let mut text = std::fs::read_to_string(&cfg).unwrap();
    text.push_str("\n[scenario.real_vote]\nbudget = 200000\n");
    std::fs::write(&cfg, text).unwrap();

    let out = run(&dir, &["run", "--scenario", "real_vote"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("4,321"),
        "expected imported CU in report: {stdout}"
    );
    // Imported real logs carry no demo marker → no warning.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("DEMO"),
        "unexpected demo warning: {stderr}"
    );
}

#[test]
fn import_rejects_path_traversal_name() {
    let dir = scratch_dir("traversal");
    assert!(run(&dir, &["init"]).status.success());
    std::fs::write(
        dir.join("tx.json"),
        r#"{"result":{"meta":{"logMessages":["Program P invoke [1]","Program P success"]}}}"#,
    )
    .unwrap();

    let out = run(&dir, &["import", "tx.json", "--name", "../../ESCAPED"]);
    assert!(!out.status.success(), "traversal name should be rejected");
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid"));
    // Nothing was written outside the logs dir.
    assert!(!dir.parent().unwrap().join("ESCAPED.log").exists());
}

#[test]
fn run_rejects_path_traversal_scenario_name() {
    let dir = scratch_dir("traversal-cfg");
    assert!(run(&dir, &["init"]).status.success());
    let cfg = dir.join("cu-profiler.toml");
    let mut text = std::fs::read_to_string(&cfg).unwrap();
    text.push_str("\n[scenario.\"../../../etc/evil\"]\nbudget = 100000\n");
    std::fs::write(&cfg, text).unwrap();

    let out = run(&dir, &["run", "--scenario", "../../../etc/evil"]);
    assert!(
        !out.status.success(),
        "traversal scenario name should be rejected"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid"));
}

// `import` error path via local-file injection — deterministic, no sockets.
//
// This replaces two earlier tests that drove `import --signature` against a hand-rolled
// one-shot loopback HTTP server. That server was nondeterministic under sandboxed CI
// (the child process's connection was refused before the moved listener served),
// turning a transport detail into flaky failures. The `--signature` path shares its
// parsing with the file path and is unit-tested purely in `commands::import`
// (`logs_from_response`, `response_null_result_is_not_found`, …), and the happy
// file -> run -> report pipeline is already covered by
// `import_real_tx_json_then_run_measures_it`. What remained uncovered — a log-less
// response surfacing a clear error and a non-zero exit — is pinned here with no network.

#[test]
fn import_file_without_logs_reports_error() {
    let dir = scratch_dir("import-file-empty");
    assert!(run(&dir, &["init"]).status.success());

    // A getTransaction response carrying no logs (e.g. a not-found / null result):
    // the import must surface a clear error and exit non-zero, never write a log.
    let tx = dir.join("empty.json");
    std::fs::write(&tx, r#"{"jsonrpc":"2.0","id":1,"result":null}"#).unwrap();

    let out = run(&dir, &["import", tx.to_str().unwrap(), "--name", "empty"]);
    assert!(!out.status.success(), "expected failure for a log-less tx");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("logMessages"), "stderr: {stderr}");
    assert!(!dir.join(".cu/logs/empty.log").exists());
}

#[test]
fn bench_validates_a_plan_and_summarises() {
    let dir = scratch_dir("bench-ok");
    let fixtures = dir.join("bench.toml");
    std::fs::write(
        &fixtures,
        "[[instruction]]\nscenario=\"swap\"\nprogram_id=\"11111111111111111111111111111111\"\ndata=\"01ab\"\n",
    )
    .unwrap();

    let out = run(&dir, &["bench", "--fixtures", fixtures.to_str().unwrap()]);
    assert!(out.status.success(), "bench failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("bench plan OK: 1 instruction"),
        "summary: {stdout}"
    );
    assert!(stdout.contains("swap"), "scenario: {stdout}");
}

#[test]
fn bench_rejects_an_invalid_plan() {
    let dir = scratch_dir("bench-bad");
    let fixtures = dir.join("bench.toml");
    // Non-base58 program id must be rejected with a non-zero exit.
    std::fs::write(
        &fixtures,
        "[[instruction]]\nscenario=\"s\"\nprogram_id=\"not-valid-0OIl\"\n",
    )
    .unwrap();

    let out = run(&dir, &["bench", "--fixtures", fixtures.to_str().unwrap()]);
    assert!(!out.status.success(), "expected invalid plan to fail");
    assert!(String::from_utf8_lossy(&out.stderr).contains("base58"));
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

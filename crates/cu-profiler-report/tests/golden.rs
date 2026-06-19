//! Golden tests: drive the full pipeline from a recorded-log fixture and assert
//! the rendered output against committed expectations.
//!
//! The expected JSON is deterministic (no timestamps or randomness in the
//! report model). To regenerate after an intentional change, run:
//!
//! ```text
//! CU_PROFILER_BLESS=1 cargo test -p cu-profiler-report --test golden
//! ```

use std::path::{Path, PathBuf};

use cu_profiler_core::Profiler;
use cu_profiler_core::backend::RecordedLogsBackend;
use cu_profiler_core::budget::BudgetPolicy;
use cu_profiler_core::metadata::RunMetadata;
use cu_profiler_core::model::{Report, Status};
use cu_profiler_core::scenario::Scenario;
use cu_profiler_report::Format;

fn fixtures_dir() -> PathBuf {
    // crates/cu-profiler-report -> workspace root -> tests/fixtures
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
}

fn build_report(scenario_name: &str, log_file: &str, budget: u64) -> Report {
    let logs = std::fs::read_to_string(fixtures_dir().join("logs").join(log_file))
        .expect("fixture log readable");
    let mut backend = RecordedLogsBackend::new();
    backend.insert_blob(scenario_name, &logs, !logs.contains("failed"));

    let mut scenario = Scenario::new(scenario_name);
    scenario.budget = BudgetPolicy {
        absolute_max_cu: Some(budget),
        warn_at_budget_pct: Some(90.0),
        ..Default::default()
    };
    // Fixed profiler version keeps the golden JSON stable across crate bumps.
    Profiler::new().run(
        &backend,
        &[scenario],
        None,
        RunMetadata::recorded("0.0.0-golden"),
    )
}

fn assert_golden(name: &str, actual: &str) {
    let path = fixtures_dir().join("expected").join(name);
    if std::env::var("CU_PROFILER_BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; regenerate with CU_PROFILER_BLESS=1",
            path.display()
        )
    });
    assert_eq!(
        actual.replace("\r\n", "\n"),
        expected.replace("\r\n", "\n"),
        "golden mismatch for {name} (bless with CU_PROFILER_BLESS=1 if intended)"
    );
}

#[test]
fn swap_with_cpi_report_json_is_stable() {
    let report = build_report("swap_with_cpi", "swap_with_cpi.log", 100_000);

    // Structural assertions that document the expected analysis.
    let s = &report.scenarios[0];
    assert_eq!(s.status, Status::Warn); // 96,812 / 100,000 ≥ 90%
    assert_eq!(s.measurement.total_cu, 96_812);
    assert_eq!(s.measurement.cpi_count, 1);
    assert_eq!(s.measurement.cpi_depth, 1);
    assert_eq!(s.measurement.requested_limit, Some(200_000));

    let json = cu_profiler_report::render(&report, Format::Json).unwrap();
    assert_golden("swap_with_cpi.report.json", &json);
}

#[test]
fn swap_with_cpi_table_is_stable() {
    let report = build_report("swap_with_cpi", "swap_with_cpi.log", 100_000);
    let table = cu_profiler_report::render(&report, Format::Table).unwrap();
    assert_golden("swap_with_cpi.table.txt", &table);
}

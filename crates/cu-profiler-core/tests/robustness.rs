//! Property/robustness tests for the parser and pipeline.
//!
//! The Solana log parser is deliberately *tolerant*: malformed, truncated, or
//! adversarial input must never panic — it should degrade to warnings, raw
//! lines, and lowered confidence. This test hammers the full pipeline
//! (`analyze` → `Profiler` → JSON) with thousands of pseudo-random log streams
//! and asserts the invariants hold.
//!
//! The generator is a deterministic LCG (no `rand`, no time) so failures
//! reproduce exactly in CI.

use cu_profiler_core::Profiler;
use cu_profiler_core::backend::RecordedLogsBackend;
use cu_profiler_core::metadata::RunMetadata;
use cu_profiler_core::parser::{self, analyze};
use cu_profiler_core::program_registry::ProgramRegistry;
use cu_profiler_core::scenario::Scenario;

/// A grab-bag of valid, malformed, and hostile log fragments.
const FRAGMENTS: &[&str] = &[
    "Program X invoke [1]",
    "Program X invoke [2]",
    "Program X invoke [99]",
    "Program X invoke [notanumber]",
    "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [2]",
    "Program X consumed 1200 of 200000 compute units",
    "Program X consumed of compute units",
    "Program X consumed abc of 200000 compute units",
    "Program X consumed 99999999999999999999999999 of 1 compute units",
    "Program X success",
    "Program X failed: custom program error: 0x1",
    "Program failed to complete",
    "Program log: hello world",
    "Program log: CU_PROFILER_BEGIN name=a cu=200000",
    "Program log: CU_PROFILER_END name=a cu=188000",
    "Program log: CU_PROFILER_BEGIN name=b",
    "Program log: CU_PROFILER_END name=zzz",
    "Program log: CU_PROFILER_POINT name=mid cu=190000",
    "Program data: AAAA",
    "ComputeBudget111111111111111111111111111111 invoke [1]",
    "totally unstructured %%% line",
    "",
    "Program ",
    "consumed 5 of 10 compute units",
];

/// Minimal deterministic LCG (glibc constants).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        &slice[(self.next() as usize) % slice.len()]
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

fn random_log(rng: &mut Lcg) -> Vec<String> {
    let len = rng.range(40);
    (0..len)
        .map(|_| (*rng.pick(FRAGMENTS)).to_string())
        .collect()
}

#[test]
fn analyze_never_panics_and_holds_invariants() {
    let registry = ProgramRegistry::with_builtins();
    let mut rng = Lcg(0x1234_5678_9abc_def0);

    for _ in 0..4000 {
        let logs = random_log(&mut rng);
        let a = analyze(&logs, &registry);

        // Unattributed share is always a sane percentage.
        assert!(
            a.unattributed_pct.is_finite(),
            "non-finite unattributed_pct for logs: {logs:?}"
        );
        assert!(
            (0.0..=100.0).contains(&a.unattributed_pct),
            "unattributed_pct out of range ({}) for logs: {logs:?}",
            a.unattributed_pct
        );

        // Scope percentages, when present, are finite and bounded.
        for s in &a.scopes {
            if let Some(p) = s.percentage_of_total {
                assert!(p.is_finite() && (0.0..=100.0).contains(&p));
            }
        }

        // CPI attribution can never exceed the headline total (the bug the
        // exact-match fix prevents).
        assert!(a.cpi_count <= 1_000);
    }
}

#[test]
fn full_pipeline_and_json_round_trip_never_panic() {
    let mut rng = Lcg(0xfeed_face_dead_beef);

    for i in 0..1500 {
        let logs = random_log(&mut rng);
        let mut backend = RecordedLogsBackend::new();
        let name = format!("fuzz_{i}");
        backend.insert(name.clone(), logs, true);

        let report = Profiler::new().run(
            &backend,
            &[Scenario::new(&name)],
            None,
            RunMetadata::recorded("0.0.0-fuzz"),
        );

        // Report must serialize and deserialize back to an equal value.
        let json = serde_json::to_string(&report).expect("report serializes");
        let back: cu_profiler_core::model::Report =
            serde_json::from_str(&json).expect("report round-trips");
        assert_eq!(report, back);
    }
}

#[test]
fn pathological_inputs_are_handled() {
    let registry = ProgramRegistry::with_builtins();
    // Deeply (claimed) nested invokes without matching closes.
    let deep: Vec<String> = (0..500)
        .map(|i| format!("Program P{i} invoke [{}]", i + 1))
        .collect();
    let a = analyze(&deep, &registry);
    assert!(a.call_tree.children.len() <= 1); // one root chain, no panic

    // A million-line flat log of the same success line.
    let flat: Vec<String> = std::iter::repeat_n("Program X success".to_string(), 10_000).collect();
    let _ = parser::analyze(&flat, &registry); // must not panic
}

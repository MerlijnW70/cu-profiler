# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **HTML report** output (`--format html`): a self-contained static document
  (inline CSS, no scripts) with the summary, per-scenario measurement, CPI call
  tree, scopes and diagnostics. `ci` writes it to `[output] html_path`. All
  user-supplied text is HTML-escaped.
- Optional **Anchor IDL** support behind the `anchor` feature (off by default,
  so native Solana stays first-class). Parses both pre-0.30 and 0.30+ IDL
  layouts to label the program by its IDL name, expose instruction/account
  names, and decode `custom program error: 0x…` failure logs into Anchor error
  names. Wired through `[anchor] idl = "…"` in config (CLI built with
  `--features anchor`).
- Scope-level CU estimation from optional `cu=<remaining>` compute snapshots in
  markers. When a scope's BEGIN and END both carry a snapshot, the profiler
  reports the (inclusive) delta with `attribution_method = "log-delta"`;
  otherwise the scope stays unquantified (`"estimated"`). New
  `markers::{begin,end,point}_line_cu` builders emit the snapshots.
- Three additional diagnostics completing the spec's detection set:
  `high_cpi_share`, `event_log_bloat`, and `late_validation` (marker-gated,
  evidence-based — fires only when a validation scope opens after a CPI).
- Per-instruction CU breakdown (`measurement.per_instruction`), derived from
  top-level program invocations.

### Fixed
- CPI attribution no longer falls back to "any open frame"; a `consumed` line is
  attributed only on an exact program-ID match, preventing misattribution on
  malformed or out-of-order logs.
- `compare` (and any explicitly requested baseline) now fails with exit code 4
  when the baseline file is missing, instead of silently comparing against an
  empty baseline.
- Failure detection no longer relies on a fragile `"failed"` substring match;
  it uses the parser's structured `Program <id> failed` events.
- Markdown output escapes pipes, backticks and newlines in scenario names and
  diagnostic text, so untrusted names cannot corrupt the table.

### Initial v1
- Initial v1 workspace: `cu-profiler-core`, `cu-profiler-report`,
  `cu-profiler-cli`, `cu-profiler-instrumentation`.
- Solana log parser: CPI call-tree reconstruction, compute-budget heuristics,
  and explicit scope markers.
- Budget policy engine (absolute/warn/regression/CPI/unattributed thresholds).
- Baseline system with input fingerprinting and staleness detection.
- Confidence scoring with explicit reasons.
- Diagnostic engine with Solana-specific recommendations.
- Report rendering: table, JSON, Markdown, JUnit.
- CLI: `init`, `run`, `compare`, `baseline save/approve`, `ci`, `explain`,
  `inspect`, with stable, documented exit codes.
- `RecordedLogsBackend` (v1 backend) plus designed skeletons for
  `program-test` / `banks-client` execution backends.
- Golden fixtures and end-to-end CLI tests.
- Documentation set (architecture, scenarios, baselines, CI, instrumentation,
  report schema, config) and a GitHub Actions example.

[Unreleased]: https://github.com/MerlijnW70/cu-profiler/commits/main

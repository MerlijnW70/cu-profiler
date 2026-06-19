# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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

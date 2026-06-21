# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-21

### Added
- **`cu-profiler comment`** — post the Markdown report as a *sticky* pull-request
  comment: one comment per PR, created once and updated in place on every run
  (identified by a hidden `<!-- cu-profiler-report -->` marker). Reuses the existing
  `remote`/`ureq` stack (rustls, openssl-free); auth via `$GITHUB_TOKEN`. Supports
  `--input report.md`, `--pr`, `--repo`, `--marker`, and `--dry-run` (no network).
  `init --workflow` now scaffolds a render-and-comment step with
  `permissions: pull-requests: write`.
- **Multi-sample runs + variance.** `Scenario.samples` (and a `--samples` override)
  now run a scenario N times on non-deterministic backends and record a `SampleStats`
  distribution (count/min/median/max/variance/std-dev/CV) in the measurement. Variance
  folds into the confidence score (CV ≥2% → Medium, ≥10% → Low), implementing the
  spec §12 "sample variance" factor. The deterministic recorded backend ignores
  `samples` and never fabricates a spread (`ExecutionBackend::is_deterministic`).
- **`cu-profiler bench` (turnkey real-CU).** A declarative bench-plan schema
  (`cu_profiler_core::bench::BenchPlan`: instructions, program id, hex data, accounts)
  with base58/hex validation, and a `bench` subcommand that validates the plan,
  optionally builds the program (`--build` via `cargo build-sbf`), and — with
  `--program-name` — measures real compute units by delegating to the Linux
  `cu-profiler-bench` executor over `PATH`. The executor links the Solana stack and is
  a runtime sibling, not a build dependency, so the main CLI stays Solana-free; when
  it is absent, `bench` validates the plan and fails with the exact command to run.
- **Mollusk turnkey execution + `cu-profiler-bench` binary (Linux-only).** The
  detached `cu-profiler-mollusk` crate gains `MolluskBackend::from_plan` (parses a
  `BenchPlan`'s base58/hex fixtures into Solana `Instruction`/`Account` types and
  meters real compute units) and `run_plan` (plan → metered `Report`), exposed as a
  thin `cu-profiler-bench` binary that the main CLI's `bench` delegates to. Validated
  by the SBF CI job. This is the executor that produces the real CU end to end.

## [0.1.2] - 2026-06-20

### Security
- **Path traversal fixed (high):** scenario/`--name` values are validated before
  becoming a file path, so `--name ../../x` or a malicious config scenario name
  can no longer read or write outside the logs directory. Hierarchical names
  (`swap/happy_path`) are still allowed.
- **DoS via deep CPI trees fixed:** adversarial logs with tens of thousands of
  unclosed `invoke` lines built a call tree deep enough to overflow the stack on
  serialize/traverse. The tree now caps nesting at 64 levels (real Solana CPI
  depth is ≤ a handful), flattening beyond and emitting a parser warning.
- **Resource limits added:** log/JSON files are read with a 64 MiB cap, and RPC
  `getTransaction` responses with a 32 MiB cap, so a hostile file or RPC can't
  exhaust memory. The demo-marker check now reads only the first line.

  Audit also confirmed clean: TLS validation active via rustls, a 20 s RPC
  timeout, output escaped for HTML/XML/Markdown, no `unwrap`/`expect` in library
  code, and `serde_json`'s depth limit already guards JSON parsing.

### Added
- **`cu-profiler import --signature <sig> [--rpc <url>] [--commitment]`** — fetch a
  transaction's logs **live** from an RPC `getTransaction` over a rustls TLS stack
  (no OpenSSL, no nasm/cmake; builds on Windows). Behind the `remote` feature
  (on by default); without it, `--signature` is a clear configuration error.
  Surfaces RPC errors and not-found cleanly. Verified by unit tests plus a real
  HTTP round-trip against a local server (no live RPC, no mock product data).

### Changed
- Honesty pass on misleading placeholders a user could hit:
  - `[project] mode` now defaults to `recorded` (was `program-test`), is
    validated against the known set (unknown = config error), and `run`/`ci`
    print a stderr note when a live mode is set explaining those backends are
    library-only — the CLI profiles recorded logs.
  - The core `ProgramTestBackend` / `BanksClientBackend` stubs now return an
    actionable error pointing to the `cu-profiler-program-test` /
    `cu-profiler-mollusk` integration crates, and their docs say so.
  - `Scenario.samples` is documented as reserved (the deterministic recorded
    backend ignores it).

## [0.1.1] - 2026-06-20

### Added
- **Live `mollusk-svm` backend** (`integration/cu-profiler-mollusk`) — runs a
  compiled **SBF** program through Mollusk and feeds its **real**
  `compute_units_consumed` into the cu-profiler pipeline (translated into the
  canonical log line the parser reads). Ships a tiny SBF demo program (built with
  `cargo build-sbf`) that doubles as a runnable example. Detached crate, verified
  by a dedicated Linux CI job — the first path that measures genuine CU end to end.
- **Live `program-test` backend** (`integration/cu-profiler-program-test`): runs a
  scenario in `solana-program-test`'s in-process runtime and captures the real
  transaction `log_messages`, which feed the same parser as recorded logs. Kept
  as a workspace-detached crate (the Solana stack is heavy and `openssl-sys` does
  not build on Windows), built by a dedicated Linux CI job so the core crates and
  the local gate stay Solana-free. Note: real CU metering requires an SBF (`.so`)
  program — the runtime does not meter in-process native `processor!` functions;
  the backend captures whatever logs the runtime emits.
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

### Added
- **`cu-profiler import <tx.json>`** — turns a real transaction's `logMessages`
  (Solana `getTransaction --output json`, or any JSON containing them, at any
  nesting) into a scenario log, so a real on-chain transaction can be profiled
  with `run` — no live RPC or Solana toolchain. Name defaults to the file stem.

### Changed
- `init`'s scaffolded example logs now carry a `DEMO_DATA_ONLY` marker, and
  `run`/`ci` print a clear warning to **stderr** when profiling that demo
  fixture data — so its numbers can't be mistaken for a real measurement. The
  warning goes to stderr only, keeping JSON/JUnit/`--output` machine output clean.
- crates.io publish-readiness: each crate has `keywords`, `categories`, and a
  per-crate `README.md`; the project README gained CI / license / MSRV badges.
  Verified with `cargo package` (publish order: core → instrumentation →
  report → cli).

### Fixed
- A scope whose `cu=` snapshot delta exceeds the program's measured total (only
  possible with inconsistent logs) now **withholds** `percentage_of_total` and
  emits a warning, instead of reporting a nonsensical >100% share. Found by a new
  property/fuzz test that hammers the pipeline with thousands of adversarial log
  streams and asserts no panics and bounded invariants.
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

# `cu-profiler` — Project Reference

> Canonical reference for the design and scope of `cu-profiler`.
> This document is the source of truth for *what* we are building and *why*.
> Implementation details live in the code; this document defines the contract.

---

## 1. Product Vision

`cu-profiler` is a **100% Rust** library and CLI tool for Solana programs that
**measures, explains, compares, and enforces** compute-unit (CU) consumption in CI.

It is not a simple log parser. It is a **Solana-native compute intelligence toolkit**.

It helps Solana developers answer:

- How many compute units does my program use?
- Which instruction is the most expensive?
- Which CPIs cause the most compute?
- Which scope/module/function caused a regression?
- Which scenarios sit close to their CU budget?
- Which PR increases CU consumption?
- Should CI fail because compute is too high?
- Is the measurement reliable enough to act on?
- Where is the biggest optimization opportunity?

**End goal:** `cu-profiler` becomes the standard compute regression suite for Solana programs.

Think *gas snapshots for Solana*, but broader: scenarios, CPI call trees, scope
markers, baselines, budget policies, JSON/JUnit/Markdown output, and CI exit codes.

The product as a whole is:

```
Solana compute observability
+ regression testing
+ budget enforcement
+ scenario intelligence
+ CPI attribution
+ CI-native reporting
```

---

## 2. Core Constraints

Built entirely in **Rust**. No TypeScript, JavaScript, Python, shell-heavy
architecture, or external runtime dependency for core functionality.

The codebase must be:

- idiomatic Rust;
- async-ready;
- modular;
- library-first;
- CLI as a thin layer over the core;
- well testable;
- feature-flag driven;
- backed by stable report schemas;
- equipped with professional error handling;
- easily extensible;
- CI-friendly;
- suitable for public open-source release.

Use clear domain-driven modules. Avoid monolithic files.

---

## 3. Main Components

The project is a Rust workspace with at least these crates:

```
cu-profiler/
├── crates/
│   ├── cu-profiler-core/
│   ├── cu-profiler-cli/
│   ├── cu-profiler-instrumentation/
│   └── cu-profiler-report/
├── examples/
├── tests/
├── docs/
├── .github/
└── Cargo.toml
```

### 3.1 `cu-profiler-core`

Contains all core logic:

- scenario model;
- profiler engine;
- simulation abstraction;
- log parser;
- CPI parser;
- scope attribution;
- budget policy engine;
- baseline comparison;
- confidence scoring;
- diagnostic engine;
- core data types;
- error types.

This crate must **not** depend on CLI-specific code.

### 3.2 `cu-profiler-cli`

CLI interface only. Uses `clap`.

Commands:

```
cu-profiler init
cu-profiler run
cu-profiler compare
cu-profiler baseline save
cu-profiler baseline approve
cu-profiler ci
cu-profiler explain
cu-profiler inspect
```

The CLI must use library calls from `cu-profiler-core`.

### 3.3 `cu-profiler-instrumentation`

Lightweight helpers/macros for Solana programs:

- mark scopes;
- log begin/end markers;
- optionally log compute snapshots;
- feature-gated instrumentation.

Instrumentation must be **optional** and clearly incur overhead. The profiler
must be able to report that overhead.

### 3.4 `cu-profiler-report`

Output formats:

- table;
- JSON;
- Markdown;
- JUnit XML;
- HTML (later).

This crate must keep raw data and rendering separated.

---

## 4. Technical Foundation

Use Solana local simulation as the primary v1 backend.

The core abstracts over execution backends:

```
ExecutionBackend
├── ProgramTestBackend
├── BanksClientBackend
├── RpcSimulationBackend (later)
└── RecordedLogsBackend (for tests)
```

For v1:

- implement `ProgramTestBackend`;
- implement `BanksClientBackend` as a wrapper/abstraction;
- design interfaces for `RpcSimulationBackend` up front, but it need not fully work yet.

Use `solana-program-test` for local simulation. The tool must be able to analyze
transaction simulation results and logs.

---

## 5. Measurement Model

Measure at minimum:

- total compute units per scenario;
- compute units per transaction;
- compute units per instruction;
- compute units per program invocation;
- CPI count;
- CPI depth;
- Compute Budget requested limit;
- actual consumed;
- budget margin;
- over-requested compute;
- failed simulation path;
- logs;
- parsing warnings;
- confidence score.

Scope-level attribution may **only** be done via explicit markers or reliable log
structures. **Make no false claims about automatic source-line profiling.**

---

## 6. Scenario Model

Introduce a first-class `Scenario`. A scenario is not just a test — it is a
**reproducible compute benchmark**.

A scenario conceptually contains:

```
Scenario
- name
- description
- tags
- criticality
- owner
- instruction builder
- account fixtures
- expected result
- budget policy
- sample count
- metadata
```

Supported scenarios such as:

```
swap/happy_path
swap/large_pool
swap/referral_enabled
swap/token_2022
swap/failure_invalid_owner
initialize_pool/minimal
initialize_pool/full_config
liquidate/max_position
liquidate/stale_oracle_failure
```

**Also measure failure paths.** A failed instruction that consumes a lot of CU is
relevant for both performance and security.

---

## 7. Budget Policy Engine

Budget policies are first-class. Support at minimum:

```
absolute max CU
warning threshold
max regression percentage
max regression units
minimum margin percentage
max CPI count
max CPI depth
max unattributed CU percentage
max instrumentation overhead warning
```

Each policy produces a structured result:

```
PolicyResult
- policy_id
- status: pass | warn | fail
- severity
- actual
- expected
- message
- remediation hint
```

CI must be able to fail on:

- absolute budget exceeded;
- regression threshold exceeded;
- stale baseline;
- scenario failed;
- low confidence (when strict mode is on).

---

## 8. Baseline System

Baselines are essential. Implement:

```
cu-profiler baseline save
cu-profiler compare
cu-profiler baseline approve
```

A baseline stores not only CU but also fingerprint metadata. A baseline record contains:

```
scenario name
actual units
budget
timestamp
git commit (if available)
program binary hash
scenario hash
fixture hash
config hash
solana crate versions (if available)
profiler version
instrumentation mode
confidence score
```

When fingerprints do not match, the tool must be able to say:

```
Baseline is stale because fixture hash changed.
Comparison confidence: Low.
```

---

## 9. Log Parser

A robust parser for Solana logs. It must recognize at minimum:

```
Program <id> invoke [depth]
Program <id> consumed X of Y compute units
Program <id> success
Program <id> failed
ComputeBudget instructions
custom CU_PROFILER_BEGIN markers
custom CU_PROFILER_END markers
custom CU_PROFILER_POINT markers
```

It must build a call tree:

```
root transaction
└── user_program::swap
    ├── spl_token::transfer
    ├── associated_token_account::create
    │   ├── system_program::create_account
    │   └── spl_token::initialize_account
    └── oracle_program::read_price
```

Each node contains:

```
program id
program label (if known)
invoke depth
units consumed (if available)
children
logs
status
```

The parser must be **tolerant**:

- incomplete logs must not panic;
- preserve unknown lines as raw logs;
- collect parser warnings;
- lower confidence on inconsistencies.

---

## 10. Program ID Labeling

A registry for known program IDs. Support at minimum:

```
System Program
Compute Budget Program
SPL Token
Token-2022
Associated Token Account Program
Memo Program
```

The registry is extensible via config. For unknown programs:

```
Unknown Program <pubkey>
```

---

## 11. Scope Attribution

Explicit profiler markers, conceptual log form:

```
CU_PROFILER_BEGIN name=swap::validate_accounts
CU_PROFILER_POINT name=after_validation
CU_PROFILER_END name=swap::validate_accounts
```

The parser links scopes to logs and compute deltas where possible. Each scope result:

```
ScopeResult
- name
- parent
- units_estimated
- percentage_of_total
- attribution_method
- confidence
- warnings
```

Rules:

- balanced markers give higher confidence;
- unbalanced markers produce a parser warning;
- many markers raise the instrumentation overhead warning;
- scope-level CU is an **estimate** unless derived directly from reliable logs.

---

## 12. Confidence Scoring

Every measurement carries a confidence score:

```
High | Medium | Low | Unknown
```

Factors:

```
simulation success
logs complete
parser consistency
baseline fingerprint match
sample variance
instrumentation overhead
unattributed CU percentage
scope marker quality
runtime/version metadata availability
```

**Sample variance** folds in when a scenario is multi-sampled (`samples > 1`) on a
non-deterministic backend: a high coefficient of variation across the runs demotes
confidence (≥2% → Medium, ≥10% → Low) with a reason. The deterministic recorded
backend ignores `samples`, so it never reports variance it did not observe.

Always report **why** confidence is not `High`. Example:

```
Confidence: Medium
Reasons:
- 22% unattributed CU
- 14 scope markers detected
- baseline matched
- logs parsed successfully
```

---

## 13. Diagnostic Engine

Detects anti-patterns. At minimum:

```
near budget limit
absolute budget exceeded
regression exceeded
expensive failure path
late validation
CPI explosion
high CPI share
high CPI depth
event/log bloat
over-requested compute budget
high unattributed CU
stale baseline
low confidence measurement
```

Each diagnostic:

```
Diagnostic
- id
- title
- severity
- scenario
- evidence
- recommendation
```

Recommendations must be **Solana-specific**, not generic. Examples:

```
Move cheap validation before CPI.
Reduce event emission in hot path.
Add scope markers around account validation and math.
Check duplicate ATA creation.
Consider zero-copy for large account deserialization.
Lower requested compute limit if consistently over-requested.
```

---

## 14. Reporting

Keep raw data and rendering separated. Support multiple outputs.

### 14.1 Table Output (local CLI)

```
Scenario                  Actual CU    Budget     Delta      Status
swap_exact_in              96,812      100,000    +6.1%      WARN
initialize_pool            78,902       80,000   +10.6%      FAIL
close_position             38,991       45,000    -2.1%      PASS
```

### 14.2 JSON Output (machines / CI)

Stable schema via `serde`. Must contain all details:

```
summary
scenarios
budgets
baseline_comparison
call_trees
scopes
diagnostics
confidence
metadata
```

### 14.3 Markdown Output

For GitHub PR comments. `cu-profiler run/ci --format markdown` renders the report as
Markdown; `cu-profiler comment` (see §15) delivers it as a **sticky** PR comment —
one comment per pull request, created once and updated in place on every later run,
identified by a hidden `<!-- cu-profiler-report -->` marker. (The "GitHub PR comments"
capability listed under §35 is delivered here.)

### 14.4 JUnit XML Output

For CI test dashboards. A scenario that fails budget must map to a failed test case.

### 14.5 HTML (later)

Keep the report layer extensible so HTML/flamegraphs can be added later.

---

## 15. CLI UX

The CLI must feel professional.

### `cu-profiler init`

Generates:

```
cu-profiler.toml
.cu/baseline.json (if desired)
.github/workflows/cu-profiler.yml (if flag)
examples/scenarios.rs
```

### `cu-profiler run`

Runs scenarios and shows the report. Flags:

```
--config
--format table|json|markdown|junit
--output
--scenario
--tag
--samples
--strict
--fail-on-budget
--fail-on-regression
--fail-on-low-confidence
```

### `cu-profiler compare`

Compares the current run against the baseline.

### `cu-profiler ci`

Optimized CI mode:

- deterministic output;
- clear exit codes;
- JSON/Markdown artifacts;
- no interactive prompts.

### `cu-profiler comment`

Posts the Markdown report as a sticky pull-request comment (§14.3). Flags:

```
--input        post a pre-rendered report.md instead of re-rendering from config
--pr           PR number (defaults to the GitHub Actions event, then refs/pull/<n>/merge)
--repo         owner/repo (defaults to $GITHUB_REPOSITORY)
--marker       hidden marker identifying the sticky comment (default: cu-profiler-report)
--dry-run      render and print the comment body without contacting GitHub
```

Auth is the `$GITHUB_TOKEN` env var (never a flag); the workflow needs
`permissions: pull-requests: write`. On a non-PR build (e.g. `push`) the command
no-ops. Requires the `remote` feature (on by default), like `import --signature`.
`cu-profiler init --workflow` scaffolds a workflow that renders and posts the comment.

### `cu-profiler explain <scenario>`

Diagnoses a single scenario.

### `cu-profiler inspect <report.json>`

Reads an existing report and shows analysis without re-simulating.

---

## 16. Exit Codes

Stable and documented:

```
0 = success
1 = budget or regression failure
2 = configuration error
3 = simulation failure
4 = stale or missing baseline
5 = parser/report error
6 = low confidence in strict mode
```

---

## 17. Config File

Uses `cu-profiler.toml`. Parse strictly, but give clear error messages.

```toml
[project]
name = "my-solana-program"
program_id = "..."
mode = "program-test"

[defaults]
warn_at_budget_pct = 90
max_regression_pct = 5
fail_on_budget = true
fail_on_regression = true
fail_on_stale_baseline = false

[output]
default_format = "table"
json_path = "target/cu-profiler/report.json"
markdown_path = "target/cu-profiler/report.md"
junit_path = "target/cu-profiler/junit.xml"

[program_labels]
"11111111111111111111111111111111" = "System Program"

[scenario.swap_exact_in]
budget = 100000
warn_at_budget_pct = 90
max_regression_pct = 5
critical = true
tags = ["swap", "hot-path", "user-facing"]

[scenario.initialize_pool]
budget = 80000
max_regression_pct = 3
critical = true
tags = ["admin", "setup"]
```

---

## 18. Testing Strategy

**Unit tests** for: log parsing, call tree reconstruction, budget policies,
baseline comparison, confidence scoring, report serialization, config parsing,
diagnostics.

**Golden tests** with fixture logs and expected JSON output:

```
tests/fixtures/logs/spl_token_transfer.log
tests/fixtures/expected/spl_token_transfer.report.json
```

**Integration tests** with `solana-program-test` — at least one small test program
or mock scenario.

**Snapshot tests** for table/markdown output.

**Fuzz/property tests** (later) for parser robustness.

---

## 19. Error Handling

Use `thiserror` for typed errors. **No `unwrap()` or `expect()` in library code**
(except in tests). Errors must be useful.

Bad:

```
Parse failed
```

Good:

```
Failed to parse compute unit line at log index 42:
"Program xyz consumed abc compute units"
Reason: expected integer after "consumed"
```

---

## 20. Tool Observability

Use `tracing`. CLI flags:

```
-v
-vv
--quiet
--trace
```

The core may emit tracing events but must not pollute stdout.

---

## 21. Profiler Performance

The profiler itself must be efficient:

- the parser must run linearly over logs;
- avoid unnecessary clones;
- use `Arc<str>` or owned strings where sensible;
- report structs must be serializable;
- large logs must not be duplicated everywhere;
- raw logs optional in JSON via config.

---

## 22. Feature Flags

```
default = ["json", "table"]
json
table
markdown
junit
program-test
anchor
instrumentation
ci
html (later)
```

Crates must remain compilable with minimal features where logical.

---

## 23. Anchor Support

Optional, via feature flag. For v1 it need not be complete, but design for:

```
IDL parsing
instruction name mapping
account name mapping
Anchor event parsing
Anchor error mapping
Anchor constraint overhead hints
```

- native Solana must remain first-class;
- Anchor must not be a hard dependency in the core default.

---

## 24. Documentation

Maintain docs from the start. At minimum:

```
README.md
docs/architecture.md
docs/scenarios.md
docs/baselines.md
docs/ci.md
docs/instrumentation.md
docs/report-schema.md
docs/config.md
```

README must contain: what it does, quickstart, example output, CI example,
limitations, confidence explanation.

Be explicit about limitations:

```
Function-level attribution requires explicit markers.
Program-test results may differ from mainnet runtime conditions.
Instrumentation adds overhead.
Baselines are only valid when fingerprints match.
```

---

## 25. GitHub Actions Example

```yaml
name: CU Profiler

on:
  pull_request:
  push:
    branches: [main]

jobs:
  cu-profiler:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write   # for the sticky PR comment
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --workspace
      - run: cargo test --workspace
      - run: cargo run -p cu-profiler -- ci --config cu-profiler.toml --format markdown --output target/cu-profiler/report.md
      - if: ${{ always() && github.event_name == 'pull_request' }}
        run: cargo run -p cu-profiler -- comment --input target/cu-profiler/report.md
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - uses: actions/upload-artifact@v4
        if: ${{ always() }}
        with:
          name: cu-profiler-report
          path: target/cu-profiler/
```

---

## 26. Security and Audit Use Cases

Treat audit diagnostics seriously. Detect:

```
expensive failure path
validation after CPI
near-limit critical instruction
high writable account count (later)
high CPI depth
stale baseline
missing critical scenario
```

Leave room for later:

```
compute-aware fuzzing
worst-case CU search
mainnet transaction log import
account snapshot realism
```

---

## 27. Non-Goals for v1

```
perfect source-line profiling
automatic Rust AST attribution
remote mainnet observability dashboard
hosted SaaS
AI-generated unsafe optimization patches
full Anchor internals
custom Solana runtime fork
```

Focus on reliability, good architecture, and strong data.

---

## 28. Deliverables

A working first version with:

```
workspace setup
core crate
CLI crate
report crate
instrumentation crate
config parser
scenario model
program-test backend skeleton
log parser
CPI call tree parser
scope marker parser
budget engine
baseline engine
confidence scoring
diagnostic engine
table output
JSON output
Markdown output
JUnit output skeleton
tests
fixtures
docs
GitHub Actions example
```

For anything not yet fully implemented, use clear traits, structs, TODOs, and
typed errors. Avoid half-baked code.

---

## 29. Architecture Principles

```
library-first
CLI-thin-wrapper
typed domain model
structured reports
stable schema
no fake precision
explicit confidence
feature-gated integrations
deterministic CI behavior
clear errors
composable traits
minimal global state
no hidden side effects
```

---

## 30. Expected Project Structure

`crates/cu-profiler-core/src/`

```
├── lib.rs
├── error.rs
├── scenario.rs
├── profiler.rs
├── backend/
│   ├── mod.rs
│   ├── program_test.rs
│   ├── banks_client.rs
│   └── recorded.rs
├── parser/
│   ├── mod.rs
│   ├── solana_logs.rs
│   ├── compute_budget.rs
│   ├── cpi_tree.rs
│   └── scope_markers.rs
├── budget/
│   ├── mod.rs
│   ├── policy.rs
│   └── result.rs
├── baseline/
│   ├── mod.rs
│   ├── fingerprint.rs
│   └── compare.rs
├── diagnostics/
│   ├── mod.rs
│   └── rules.rs
├── confidence.rs
├── program_registry.rs
└── metadata.rs
```

`crates/cu-profiler-report/src/`

```
├── lib.rs
├── model.rs
├── table.rs
├── json.rs
├── markdown.rs
└── junit.rs
```

`crates/cu-profiler-cli/src/`

```
├── main.rs
├── args.rs
├── commands/
│   ├── mod.rs
│   ├── init.rs
│   ├── run.rs
│   ├── compare.rs
│   ├── baseline.rs
│   ├── ci.rs
│   ├── explain.rs
│   └── inspect.rs
└── exit.rs
```

`crates/cu-profiler-instrumentation/src/`

```
├── lib.rs
├── markers.rs
└── macros.rs
```

---

## 31. Quality Bar

Required:

```
cargo fmt
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
no unwraps in library code
clear module boundaries
public API documented
examples compile
JSON schema stable
fixtures included
```

Use where useful:

```
serde
serde_json
serde_with
thiserror
anyhow (only in CLI or boundary layers)
clap
toml
tracing
prettytable-rs or comfy-table
quick-xml (for JUnit if needed)
insta (for snapshots if appropriate)
```

Pin one compatible Solana/Agave crate family deliberately and prevent dependency
mismatch.

---

## 32. First Implementation Phase

Order of work:

1. Create workspace and crates.
2. Design core domain types.
3. Design report model.
4. Build config parser.
5. Build recorded-logs backend for parser tests.
6. Build Solana log parser.
7. Build CPI call tree.
8. Build scope marker parser.
9. Build budget policy engine.
10. Build baseline comparison.
11. Build confidence scoring.
12. Build diagnostics.
13. Build table/JSON/Markdown output.
14. Wire CLI commands to core.
15. Build program-test backend skeleton.
16. Add tests and fixtures.
17. Add docs and GitHub Actions example.

**Start with recorded log fixtures** so parser, reports, and CI logic can be
developed stably without immediately depending on complex Solana integration tests.

---

## 33. Definition of Success

The first version is successful if a Solana team can:

```
1. run cu-profiler init
2. define scenarios
3. run cu-profiler run
4. see a table report
5. get a JSON export
6. save a baseline
7. compare a PR against the baseline
8. fail CI on regression
9. see which CPI/scope consumes the most CU
10. understand how reliable the measurement is
```

The tool must be honest about limitations from day one and make no unreliable claims.

---

## 34. Product Tone

Professional and exact. Avoid hype in errors/output. Use clear language:

```
PASS | WARN | FAIL | UNKNOWN
```

Give actionable messages:

```
Scenario `swap/referral_enabled` exceeded regression policy.
Baseline: 91,204 CU
Current: 96,812 CU
Delta: +6.15%
Allowed: +5.00%
Recommended action: inspect CPI count and referral account validation path.
```

---

## 35. End Goal

Build `cu-profiler` as if it were a serious open-source infrastructure tool for
Solana teams, auditors, and CI pipelines.

Keep the code modular enough to later extend with:

```
Anchor IDL support
HTML reports
flamegraphs
RPC simulation
mainnet account snapshots
compute-aware fuzzing
GitHub PR comments
historical trends
hosted dashboards
```

Keep v1 reliable, clean, and professional.

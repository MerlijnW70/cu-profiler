# Roadmap

Where cu-profiler is headed. Grouped by priority, not date. Each item notes a
rough **effort** and whether it's **pure-Rust** (builds everywhere, gated locally)
or **Solana-heavy** (pulls the Agave stack — won't build on Windows, lives in a
detached `integration/*` crate and is verified on Linux CI only).

Status legend: ✅ shipped · 🔜 next · 🧭 planned · 💭 exploratory · 🚫 non-goal.

---

## ✅ Shipped (v0.1.x)

The v1 spec surface, then taken to **FRONTIER** on the SOTA rubric:

- Log parser · CPI call tree · scope markers · compute-budget heuristics
- Budget policy engine · stable CI exit codes
- Baselines + input fingerprinting + staleness detection
- Confidence scoring · Solana-specific diagnostics
- Reports: table · JSON · Markdown · JUnit · **HTML**
- Backends: `RecordedLogsBackend` · live **`solana-program-test`** · live **`mollusk-svm` (real CU)**
- Optional **Anchor IDL** support (feature-gated)
- **`import`** — profile a real transaction from its `getTransaction` JSON,
  **or live by `--signature` from an RPC** (rustls, no OpenSSL; `remote` feature)
- Demo-data guard · property/fuzz harness · published to crates.io (v0.1.1)

### ✅ Shipped (v0.2.x) — the SOTA-scan "Next" list, now closed

- **Multi-sample runs + variance** — `samples` runs N times, reports min/median/variance, and folds variance into the confidence score (spec §12).
- **PR-comment integration** — `comment` posts the Markdown report as a sticky PR comment.
- **Turnkey real-CU path** — `cu-profiler bench` validates a plan and delegates the real Mollusk measurement to the Linux `cu-profiler-bench` executor (see [docs/bench.md](docs/bench.md)).
- **Reusable GitHub Action** — `uses: MerlijnW70/cu-profiler@v1` (composite [`action.yml`](action.yml)); **no peer in the field has one** — a lead, not catch-up.
- **Hardened test suite** — zero-survivor mutation testing (core + report) plus a dense float-clause property/law sweep; published to crates.io (v0.2.0).

---

## 🔜 Next — high value, mostly pure-Rust

| Item | Why | Effort | Kind |
| --- | --- | --- | --- |
| **`RpcSimulationBackend`** | `simulateTransaction` over RPC (designed, not built; spec §4) — a no-toolchain real-ish CU path. | ~2d | pure-Rust + rustls |
| **CU flamegraph** | HTML/SVG flamegraph of CU by scope/CPI (spec §35; litesvm ships a flamegraph script). | ~2d | pure-Rust |
| **Smooth the bench executor install** | The Linux `cu-profiler-bench` install is the one onboarding soft-spot; consider a prebuilt release binary so it's a download, not a build. | ~1d | Solana-heavy |

---

## 🧭 Planned — complete the depth

### Backends (finish the execution matrix)
- 🧭 **`BanksClientBackend`** — real impl against a test validator (currently a stub). *Solana-heavy.*
- 💭 **Mainnet account snapshots** — load real account state into program-test/mollusk for realistic CU (spec §26). *Solana-heavy.*

### Attribution
- 🧭 **Scope CU from `CU_PROFILER_POINT` deltas** — sub-scope timing between points, not just begin/end. *pure-Rust.*
- 🧭 **Anchor instruction/account-name mapping** — decode instruction data against the IDL when present (spec §23). *pure-Rust.*
- 💭 **Anchor event parsing + constraint-overhead hints** (spec §23). *pure-Rust.*

### Reporting & visualization
- 🧭 **Historical trends** — persist runs and chart CU over time / sparklines (spec §35). *pure-Rust.*

### Security / audit (spec §26)
- 💭 **Compute-aware fuzzing / worst-case-CU search** — drive inputs toward the CU ceiling (borrowed from the Solana fuzzing cluster: mollusk-fuzz, Crucible). *Solana-heavy.*
- 🧭 **More audit diagnostics** — high writable-account count, missing-critical-scenario (spec §26). *pure-Rust.*

---

## 💭 Exploratory — bigger bets

- 💭 **Worktree/monorepo mode** — profile many programs in one workspace with a shared baseline.
- 💭 **docs site / mdBook** beyond the `docs/` folder.
- 💭 **`cu-profiler.toml` JSON-schema** for editor validation/autocomplete.

---

## 🚫 Non-goals (per spec §27, still out of scope)

- Perfect automatic source-line profiling (we require explicit markers — by design).
- A hosted SaaS dashboard / mainnet observability service.
- A custom Solana runtime fork.
- AI-generated "unsafe optimization" patches.

---

## Guiding constraints

1. **Honesty over coverage** — never fake precision; surface confidence and limitations.
2. **Core stays pure Rust** — the Agave stack lives only in detached `integration/*` crates so the library, the local gate, and Windows users stay Solana-free.
3. **rustls, never openssl** — any new networking uses rustls to keep cross-platform builds working.
4. **Every feature is a verifiable test** — gate-certified at grade A before it ships.

Have an idea or a priority swap? Open an issue or a discussion.

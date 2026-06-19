# Report schema (JSON)

The JSON output is the serialized `Report` type from `cu-profiler-core::model`.
It is stable and is exactly what `cu-profiler inspect` reads back. Optional fields
are omitted when empty/absent.

## Top level

```jsonc
{
  "summary":  { "total_scenarios", "passed", "warned", "failed", "total_cu" },
  "scenarios": [ /* ScenarioReport */ ],
  "metadata": {
    "profiler_version", "backend", "instrumentation",
    "git_commit?", "solana_versions?", "generated_at?"
  }
}
```

## ScenarioReport

```jsonc
{
  "name": "swap_with_cpi",
  "status": "pass | warn | fail | unknown",
  "measurement": {
    "total_cu", "consumed",
    "requested_limit?", "over_requested?",
    "cpi_count", "cpi_depth",
    "unattributed_pct",
    "instrumentation_overhead_pct?",
    "per_instruction?": [ { "index", "program_id", "label?", "consumed?" } ],
    "simulation_success"
  },
  "call_tree?":   { /* CallNode (recursive) */ },
  "scopes?":      [ /* ScopeResult */ ],
  "policy_results?": [ { "policy_id", "status", "severity", "actual?", "expected?", "message", "remediation?" } ],
  "diagnostics?": [ { "id", "title", "severity", "scenario", "evidence", "recommendation" } ],
  "confidence":   { "level": "high|medium|low|unknown", "reasons": [ ... ] },
  "baseline_comparison?": { "matched", "stale_reasons?", "baseline_units", "current_units", "delta_units", "delta_pct" },
  "parser_warnings?": [ ... ],
  "raw_logs?": [ ... ]   // only when explicitly enabled (can be large)
}
```

## CallNode (recursive)

```jsonc
{
  "program_id", "label?", "depth",
  "units_consumed?",
  "status": "success | failed | unknown",
  "logs?": [ ... ],
  "children?": [ /* CallNode */ ]
}
```

## ScopeResult

```jsonc
{
  "name", "parent?",
  "units_estimated?", "percentage_of_total?",
  "attribution_method": "log-delta | estimated | unknown",
  "warnings?": [ ... ]
}
```

`units_estimated` / `percentage_of_total` are populated (and
`attribution_method` is `"log-delta"`) only when the scope's BEGIN and END
markers both carry a `cu=<remaining>` snapshot. Otherwise the scope is recorded
for structure with `attribution_method: "estimated"` and no CU figure — the tool
does not guess. See [instrumentation](instrumentation.md).

## Stability

The model carries no timestamps or randomness, so JSON output is deterministic —
which is why it can be golden-tested (`tests/fixtures/expected/*.report.json`).
`generated_at` is only populated if a caller explicitly stamps it.

# Baselines

A baseline records what a scenario *used to* cost, so later runs can detect
regressions. A baseline stores not only the CU figure but **fingerprint metadata**
that decides whether a later comparison is still valid.

## Commands

```sh
cu-profiler baseline save              # write current results to .cu/baseline.json
cu-profiler compare                    # compare current run vs baseline (exit 1 on regression)
cu-profiler baseline approve <name>    # mark a record as reviewed/approved
```

## What a record contains

```
scenario name        program binary hash (when available)
actual units         scenario / fixture / config hashes (the fingerprint)
budget               solana crate versions (when available)
timestamp            profiler version
git commit           instrumentation mode
                     confidence score at record time
```

## Staleness

A comparison is **stale** when any fingerprint component differs. The tool says
exactly why, and lowers comparison confidence:

```
Baseline is stale because fixture hash changed.
Comparison confidence: Low.
```

A stale baseline never silently passes a regression check. Re-record with
`baseline save` once you've confirmed the change is intended, then `compare`
again. Use `fail_on_stale_baseline = true` in config to make staleness fail CI.

## Fingerprinting

Fingerprints use FNV-1a (dependency-free, stable across Rust versions and
platforms) so persisted baselines compare reliably over time.

# Instrumentation

Scope/function attribution requires **explicit markers** — there is no automatic
source-line profiling. The `cu-profiler-instrumentation` crate emits the marker
lines that the core parser recognises.

## Marker format

Markers are ordinary program log lines:

```
CU_PROFILER_BEGIN name=swap::validate_accounts
CU_PROFILER_POINT name=after_validation
CU_PROFILER_END name=swap::validate_accounts
```

In a Solana program you emit them via `msg!`:

```rust
use cu_profiler_instrumentation::markers;

msg!("{}", markers::begin_line("swap::validate_accounts"));
// ... validation ...
msg!("{}", markers::end_line("swap::validate_accounts"));
```

Or with the feature-gated macros (auto-closing scope guard):

```rust
// Cargo.toml: cu-profiler-instrumentation = { version = "...", features = ["instrumentation"] }
cu_profiler_instrumentation::cu_scope!(|line| msg!("{line}"), "swap::math");
```

## Opt-in, and honest about cost

Instrumentation is **off by default** (the `instrumentation` feature is empty),
because markers add real compute overhead. With the feature off, every macro
expands to a no-op that still type-checks its arguments — leaving markers in your
code costs nothing in production builds.

## Compute snapshots → real per-scope CU

A marker may carry the **remaining compute units** at that point as `cu=<n>`:

```
CU_PROFILER_BEGIN name=swap::math cu=200000
CU_PROFILER_END   name=swap::math cu=188000
```

Emit them with the snapshot builders (pass `sol_remaining_compute_units()`):

```rust
use cu_profiler_instrumentation::markers;
msg!("{}", markers::begin_line_cu("swap::math", sol_remaining_compute_units()));
// ... math ...
msg!("{}", markers::end_line_cu("swap::math", sol_remaining_compute_units()));
```

When **both** the begin and end of a scope carry a snapshot, the profiler reports
that scope's CU as the (inclusive) delta — `200000 − 188000 = 12000 CU` — with
`attribution_method = "log-delta"` (a reliable figure). Without snapshots the
scope is still recorded for structure, but its CU stays unquantified
(`attribution_method = "estimated"`, `units_estimated = null`).

## How the parser uses markers

- **Balanced** begin/end pairs raise confidence.
- **Unbalanced** markers produce a parser warning and lower confidence.
- **Many** markers raise an instrumentation-overhead warning.
- Scope-level CU is an **estimate** unless derived from `cu=` snapshots; the
  report labels the attribution method (`log-delta` vs `estimated`) so you always
  know which.

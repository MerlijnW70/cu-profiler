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

## How the parser uses markers

- **Balanced** begin/end pairs raise confidence.
- **Unbalanced** markers produce a parser warning and lower confidence.
- **Many** markers raise an instrumentation-overhead warning.
- Scope-level CU is an **estimate** unless derived directly from reliable logs;
  the report labels the attribution method so you know which.

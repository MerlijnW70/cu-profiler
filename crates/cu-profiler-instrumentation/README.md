# cu-profiler-instrumentation

Lightweight, opt-in scope markers for instrumenting Solana programs with
[`cu-profiler`](https://github.com/MerlijnW70/cu-profiler).

Emits the marker lines that `cu-profiler-core` parses. It has no Solana
dependency — you supply an `emit` closure (usually wrapping `msg!`) — and is
`no_std`. Instrumentation is **off by default** (gated behind the
`instrumentation` feature) because markers add real compute overhead, which the
profiler can detect and report.

```rust
use cu_profiler_instrumentation::markers;
// In a program: msg!("{}", markers::begin_line_cu("swap::math", sol_remaining_compute_units()));
assert_eq!(markers::begin_line("x"), "CU_PROFILER_BEGIN name=x");
```

See the [project README](https://github.com/MerlijnW70/cu-profiler) for details.
Licensed under MIT OR Apache-2.0.

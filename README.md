# cu-profiler

A compute-unit profiler, written in Rust.

> Greenfield project. Quality is enforced by [quality-gate](https://h/quality-gate): every
> change is graded A–F on build, test, lint (clippy) and format, and the gate
> floor is **A**.

## Layout

- `src/lib.rs` — the `cu_profiler` library (the profiling API lives here).
- `src/main.rs` — the CLI entry point.

## Development

```sh
cargo build      # compile
cargo test       # run tests
cargo clippy --all-targets -- -D warnings   # lint
cargo fmt --check                            # format check
nh check         # run the full quality gate (must be grade A)
```

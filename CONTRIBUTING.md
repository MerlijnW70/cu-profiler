# Contributing to cu-profiler

Thanks for your interest in improving `cu-profiler`. This document explains how to
get set up and what we expect from a contribution.

## Getting started

```sh
git clone https://github.com/MerlijnW70/cu-profiler
cd cu-profiler
cargo build --workspace --all-features
cargo test  --workspace --all-features
```

Try the CLI end-to-end:

```sh
cargo run -p cu-profiler-cli -- init
cargo run -p cu-profiler-cli -- run
```

## Quality bar

Every change must pass, with **no warnings**:

```sh
cargo build  --workspace --all-features
cargo test   --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
```

CI runs exactly these. PRs that don't pass will not be merged.

Additional expectations:

- **No `unwrap()` / `expect()` in library code** (tests excepted). Use the typed
  errors in `cu-profiler-core::Error`.
- **No fake precision.** If the data doesn't support a claim, lower the confidence
  score and say why — don't invent a number.
- **New behaviour needs a test.** Parser/report changes should come with a unit
  test or a golden fixture. Regenerate golden files intentionally with
  `CU_PROFILER_BLESS=1 cargo test -p cu-profiler-report --test golden`.
- **Keep the layers clean.** Analysis lives in `cu-profiler-core`; rendering in
  `cu-profiler-report`; the CLI is a thin wrapper. Don't leak CLI concerns into
  the core.

## Architecture

See [docs/architecture.md](docs/architecture.md) for the crate boundaries and
data flow. A good first read before touching the parser or report model.

## Commit & PR guidance

- Keep commits focused; write a clear subject line.
- Reference any issue the PR closes.
- Describe *why*, not just *what*, in the PR body.
- Be explicit about limitations — this project values honesty over hype.

## Reporting bugs

Open an issue with a minimal reproduction. For parser issues, attaching the
recorded Solana log that triggers the problem is the fastest path to a fix.

## License

By contributing, you agree that your contributions will be dual-licensed under
the [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE) licenses.

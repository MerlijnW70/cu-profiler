# cu-profiler-cli

The `cu-profiler` command-line tool — compute-unit profiling, regression testing
and budget enforcement for Solana programs. See the
[project README](https://github.com/MerlijnW70/cu-profiler) for the full picture.

```sh
cargo install cu-profiler-cli      # installs the `cu-profiler` binary
cu-profiler init                   # scaffold config + example logs
cu-profiler run                    # reads recorded logs from .cu/logs by default
cu-profiler baseline save
cu-profiler compare                # exit 1 on regression
```

The CLI is a thin wrapper over the `cu-profiler-core` and `cu-profiler-report`
libraries. Exit codes are stable and documented. Licensed under MIT OR Apache-2.0.

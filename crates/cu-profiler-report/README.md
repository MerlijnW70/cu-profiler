# cu-profiler-report

Report rendering for [`cu-profiler`](https://github.com/MerlijnW70/cu-profiler).

Renders a `cu_profiler_core::model::Report` to **table**, **JSON**, **Markdown**,
**JUnit XML**, or self-contained **HTML**. The crate holds no analysis logic — it
only formats already-computed data, keeping the raw-data/presentation boundary
clean.

```rust
use cu_profiler_report::{render, Format};
// let report = ...; // from cu-profiler-core
// let html = render(&report, Format::Html)?;
```

See the [project README](https://github.com/MerlijnW70/cu-profiler) for details.
Licensed under MIT OR Apache-2.0.

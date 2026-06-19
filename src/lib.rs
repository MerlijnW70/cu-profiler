//! `cu-profiler` — a compute-unit profiler.
//!
//! Greenfield scaffold. The public surface here is intentionally minimal; the
//! real profiling API will grow from here.

/// Returns the version of the `cu-profiler` crate.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_package_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}

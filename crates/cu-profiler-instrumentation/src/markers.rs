//! The wire format for profiler scope markers.
//!
//! Markers are emitted as ordinary program log lines (`msg!`/`sol_log`) that the
//! profiler's parser recognises. Keeping the format here — shared, canonical —
//! means the emitter and the parser cannot drift apart.

use alloc::format;
use alloc::string::String;

/// Prefix tokens. These must match `cu-profiler-core`'s scope-marker parser.
pub const BEGIN: &str = "CU_PROFILER_BEGIN";
/// End-of-scope token.
pub const END: &str = "CU_PROFILER_END";
/// Point-in-time token.
pub const POINT: &str = "CU_PROFILER_POINT";

/// Build a `BEGIN` marker line for `name`.
#[must_use]
pub fn begin_line(name: &str) -> String {
    format!("{BEGIN} name={name}")
}

/// Build an `END` marker line for `name`.
#[must_use]
pub fn end_line(name: &str) -> String {
    format!("{END} name={name}")
}

/// Build a `POINT` marker line for `name`.
#[must_use]
pub fn point_line(name: &str) -> String {
    format!("{POINT} name={name}")
}

/// Build a `BEGIN` marker carrying a compute snapshot (`remaining` = remaining
/// compute units, e.g. from `sol_remaining_compute_units()`). When both the
/// begin and end markers carry a snapshot, the profiler derives a reliable
/// per-scope CU figure.
#[must_use]
pub fn begin_line_cu(name: &str, remaining: u64) -> String {
    format!("{BEGIN} name={name} cu={remaining}")
}

/// Build an `END` marker carrying a compute snapshot.
#[must_use]
pub fn end_line_cu(name: &str, remaining: u64) -> String {
    format!("{END} name={name} cu={remaining}")
}

/// Build a `POINT` marker carrying a compute snapshot.
#[must_use]
pub fn point_line_cu(name: &str, remaining: u64) -> String {
    format!("{POINT} name={name} cu={remaining}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_marker_lines_have_expected_shape() {
        assert_eq!(
            begin_line_cu("swap::math", 200_000),
            "CU_PROFILER_BEGIN name=swap::math cu=200000"
        );
        assert_eq!(
            end_line_cu("swap::math", 188_000),
            "CU_PROFILER_END name=swap::math cu=188000"
        );
        assert_eq!(
            point_line_cu("mid", 190_000),
            "CU_PROFILER_POINT name=mid cu=190000"
        );
    }

    #[test]
    fn marker_lines_have_expected_shape() {
        assert_eq!(
            begin_line("swap::validate"),
            "CU_PROFILER_BEGIN name=swap::validate"
        );
        assert_eq!(
            end_line("swap::validate"),
            "CU_PROFILER_END name=swap::validate"
        );
        assert_eq!(point_line("after"), "CU_PROFILER_POINT name=after");
    }
}

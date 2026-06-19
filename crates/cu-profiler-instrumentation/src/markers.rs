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

#[cfg(test)]
mod tests {
    use super::*;

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

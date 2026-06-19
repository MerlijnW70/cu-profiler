//! Feature-gated scope macros.
//!
//! Instrumentation is opt-in via the `instrumentation` feature. With the feature
//! **off**, every macro expands to a no-op that still type-checks its arguments,
//! so leaving markers in your program costs nothing in production builds. With
//! the feature **on**, a marker line is emitted through the caller-supplied
//! `emit` closure (typically wrapping Solana's `msg!`), and scopes auto-close on
//! drop.
//!
//! Gating happens at the macro *definition* site (two `cfg`-selected
//! definitions) so the behaviour follows *this* crate's feature, not the
//! caller's.

#[cfg(feature = "instrumentation")]
mod imp {
    /// RAII guard: emits a `BEGIN` marker on creation and an `END` marker when
    /// dropped, so a scope is always balanced even on early return.
    pub struct ScopeGuard<F: Fn(&str)> {
        name: alloc::string::String,
        emit: F,
    }

    impl<F: Fn(&str)> ScopeGuard<F> {
        /// Open a scope named `name`, emitting its begin marker immediately.
        pub fn new(name: &str, emit: F) -> Self {
            emit(&crate::markers::begin_line(name));
            Self {
                name: alloc::string::ToString::to_string(name),
                emit,
            }
        }
    }

    impl<F: Fn(&str)> Drop for ScopeGuard<F> {
        fn drop(&mut self) {
            (self.emit)(&crate::markers::end_line(&self.name));
        }
    }
}

#[cfg(feature = "instrumentation")]
pub use imp::ScopeGuard;

/// Open a profiler scope that auto-closes at the end of the current block.
///
/// `$emit` is any `Fn(&str)` that forwards a line to the program log.
#[cfg(feature = "instrumentation")]
#[macro_export]
macro_rules! cu_scope {
    ($emit:expr, $name:expr) => {
        let __cu_scope_guard = $crate::ScopeGuard::new($name, $emit);
    };
}

/// No-op form when instrumentation is disabled.
#[cfg(not(feature = "instrumentation"))]
#[macro_export]
macro_rules! cu_scope {
    ($emit:expr, $name:expr) => {
        // Type-check the arguments but emit nothing.
        let _ = (&$emit, &$name);
    };
}

/// Emit a point-in-time marker.
#[cfg(feature = "instrumentation")]
#[macro_export]
macro_rules! cu_point {
    ($emit:expr, $name:expr) => {
        ($emit)(&$crate::markers::point_line($name));
    };
}

/// No-op point marker when instrumentation is disabled.
#[cfg(not(feature = "instrumentation"))]
#[macro_export]
macro_rules! cu_point {
    ($emit:expr, $name:expr) => {
        let _ = (&$emit, &$name);
    };
}

#[cfg(all(test, feature = "instrumentation"))]
mod tests {
    use std::cell::RefCell;

    #[test]
    fn scope_guard_emits_balanced_markers() {
        let log: RefCell<Vec<String>> = RefCell::new(Vec::new());
        {
            let _g = crate::ScopeGuard::new("swap::validate", |line: &str| {
                log.borrow_mut().push(line.to_string());
            });
        }
        let lines = log.borrow();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "CU_PROFILER_BEGIN name=swap::validate");
        assert_eq!(lines[1], "CU_PROFILER_END name=swap::validate");
    }
}

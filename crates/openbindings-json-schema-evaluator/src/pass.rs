//! What one evaluation pass met that the SDK's own keywords report: a string
//! that reached a pattern the evaluator does not match, or exact arithmetic
//! too large to decide. Either ends the pass with no verdict.
use crate::pattern::Unsupported;
use std::cell::Cell;

/// Why a pass gives no verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// A `multipleOf` division the exact arithmetic declined.
    Arithmetic,
    /// A pattern the evaluator does not match.
    Unmatched(Unsupported),
    /// The stand-in for a schema resource that was not supplied, by index.
    Missing(u32),
}

thread_local! {
    static STOP: Cell<Option<Stop>> = const { Cell::new(None) };
}
// Restores an enclosing pass's state, also on unwinding.
struct Restore(Option<Stop>);
impl Drop for Restore {
    fn drop(&mut self) {
        STOP.set(self.0);
    }
}
/// Run one pass, also returning why it gives no verdict, if it met a reason.
pub(crate) fn scope<T>(evaluate: impl FnOnce() -> T) -> (T, Option<Stop>) {
    let _restore = Restore(STOP.replace(None));
    let result = evaluate();
    (result, STOP.get())
}
/// Record that the pass reached a pattern it cannot match.
pub(crate) fn unmatched(unsupported: Unsupported) {
    if STOP.get().is_none() {
        STOP.set(Some(Stop::Unmatched(unsupported)));
    }
}
/// Record that the pass reached the stand-in for a resource that was not
/// supplied.
pub(crate) fn missing(at: u32) {
    if STOP.get().is_none() {
        STOP.set(Some(Stop::Missing(at)));
    }
}
/// Record that the pass met arithmetic it cannot decide; this takes precedence
/// over an unmatched pattern.
pub(crate) fn arithmetic() {
    STOP.set(Some(Stop::Arithmetic));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pass_reports_what_it_met_and_restores_the_enclosing_pass() {
        let (outer, stop) = scope(|| {
            unmatched(Unsupported::Backtracking);
            let inner = scope(arithmetic);
            assert_eq!(inner.1, Some(Stop::Arithmetic));
            unmatched(Unsupported::PropertyEscape);
            7
        });
        assert_eq!(outer, 7);
        assert_eq!(stop, Some(Stop::Unmatched(Unsupported::Backtracking)));
        assert_eq!(scope(|| ()).1, None);
        let (_, stop) = scope(|| {
            unmatched(Unsupported::Backtracking);
            arithmetic();
        });
        assert_eq!(stop, Some(Stop::Arithmetic));
    }
    #[test]
    fn a_pass_left_by_unwinding_restores_the_enclosing_pass() {
        let (_, stop) = scope(|| {
            let _ = std::panic::catch_unwind(|| {
                scope(|| {
                    arithmetic();
                    panic!("pass probe")
                })
            });
        });
        assert_eq!(stop, None);
    }
}

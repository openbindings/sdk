//! What one evaluation pass met that the SDK's own keywords report: a string
//! that reached a pattern the evaluator does not match, a stand-in for a
//! problem preparation set aside, or exact arithmetic too large to decide.
//! Any of them ends the pass with no verdict, unless the stand-in's answer
//! cannot change the value's (`crate::bounds`).
use crate::pattern::Unsupported;
use std::cell::Cell;

/// Why a pass gives no verdict. It is `Copy`, so the pass's record is a
/// plain thread-local slot with nothing to drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// A `multipleOf` division the exact arithmetic declined.
    Arithmetic,
    /// A pattern the evaluator does not match, and the site of the `pattern`
    /// keyword that holds it, by the index its value contract's patterns give it.
    Unmatched(Unsupported, u32),
    /// The stand-in for a schema resource that was not supplied, by index.
    Missing(u32),
    /// The stand-in for a schema that would apply again to the same
    /// instance, by index.
    Cycle(u32),
    /// The stand-in for the `patternProperties` names of one schema that the
    /// evaluator does not match, reached by a nonempty object, by index.
    Names(u32),
}
impl Stop {
    /// Whether the pass stopped at a stand-in, whose answer the value's may
    /// not depend on: a missing resource, an in-place cycle, or a pattern the
    /// evaluator does not match for its syntax rather than for a limit.
    pub(crate) fn stands_in(self) -> bool {
        match self {
            Stop::Missing(_) | Stop::Cycle(_) | Stop::Names(_) => true,
            Stop::Unmatched(unsupported, _) => unsupported.stands_in(),
            Stop::Arithmetic => false,
        }
    }
}

thread_local! {
    static STOP: Cell<Option<Stop>> = const { Cell::new(None) };
    static LOWER: Cell<bool> = const { Cell::new(false) };
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
/// Record that the pass reached a pattern it cannot match, held by the
/// `pattern` keyword at `site`.
pub(crate) fn unmatched(unsupported: Unsupported, site: u32) {
    if STOP.get().is_none() {
        STOP.set(Some(Stop::Unmatched(unsupported, site)));
    }
}
/// Record that the pass reached a stand-in: for a resource that was not
/// supplied, a schema that would apply again, or names the evaluator does not
/// match.
pub(crate) fn stand_in(stop: Stop) {
    if STOP.get().is_none() {
        STOP.set(Some(stop));
    }
}
/// Record that the pass met arithmetic it cannot decide; this takes precedence
/// over an unmatched pattern.
pub(crate) fn arithmetic() {
    STOP.set(Some(Stop::Arithmetic));
}
/// Run `evaluate` as a bounds pass, the lower bound's when `lower` holds.
pub(crate) fn bound<T>(lower: bool, evaluate: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            LOWER.set(self.0);
        }
    }
    let _restore = Restore(LOWER.replace(lower));
    evaluate()
}
/// Whether the current pass is a bounds pass for the lower bound.
pub(crate) fn lower() -> bool {
    LOWER.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pass_reports_what_it_met_and_restores_the_enclosing_pass() {
        let (outer, stop) = scope(|| {
            unmatched(Unsupported::Backtracking, 1);
            let inner = scope(arithmetic);
            assert_eq!(inner.1, Some(Stop::Arithmetic));
            unmatched(Unsupported::PropertyEscape, 2);
            stand_in(Stop::Missing(3));
            7
        });
        assert_eq!(outer, 7);
        assert_eq!(stop, Some(Stop::Unmatched(Unsupported::Backtracking, 1)));
        assert_eq!(scope(|| ()).1, None);
        let (_, stop) = scope(|| {
            unmatched(Unsupported::Backtracking, 1);
            arithmetic();
        });
        assert_eq!(stop, Some(Stop::Arithmetic));
        let (_, stop) = scope(|| {
            stand_in(Stop::Missing(3));
            unmatched(Unsupported::Backtracking, 1);
        });
        assert_eq!(stop, Some(Stop::Missing(3)));
        let (_, stop) = scope(|| {
            stand_in(Stop::Cycle(4));
            stand_in(Stop::Names(5));
        });
        assert_eq!(stop, Some(Stop::Cycle(4)));
        assert!(!lower());
        assert!(bound(true, || bound(false, lower) || lower()));
        assert!(!lower());
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

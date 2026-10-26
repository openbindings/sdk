//! The default evaluator's optional evaluation work allowance and stack guard.
//!
//! An application that sets the evaluator's work cap gets one allowance per
//! evaluation pass. This crate's adapter charges a unit for each element,
//! member and member-name lookup the library reads and for each step of an
//! exact literal comparison; the evaluator charges its own keywords. Outside a
//! pass with an allowance, as in core's fixed checks, nothing is counted.
//!
//! A guarded pass may also use only so much stack. Every read the library
//! makes of a child instance passes through a charge, so each level of the
//! value is checked: the charge compares the stack address it runs at with the
//! address the guard started at (stacks grow toward lower addresses on every
//! target the SDK supports), and once the pass has used more than it may,
//! every charge fails, so the library stops descending.
use std::cell::Cell;

/// The pass's allowance ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spent;

#[derive(Clone, Copy)]
struct Allowance {
    active: bool,
    remaining: usize,
    spent: bool,
}
const NONE: Allowance = Allowance {
    active: false,
    remaining: 0,
    spent: false,
};
// One thread's work state, reached through one thread-local access per
// charge.
struct State {
    // Whether an allowance or a guard is active; a charge outside both reads
    // only this.
    engaged: Cell<bool>,
    allowance: Cell<Allowance>,
    // A guarded pass's starting stack address and the bytes it may use.
    stack: Cell<Option<(usize, usize)>>,
    // Whether the guarded pass has used more.
    deep: Cell<bool>,
}
impl State {
    fn engage(&self) {
        self.engaged
            .set(self.allowance.get().active || self.stack.get().is_some());
    }
}
thread_local! {
    static STATE: State = const {
        State {
            engaged: Cell::new(false),
            allowance: Cell::new(NONE),
            stack: Cell::new(None),
            deep: Cell::new(false),
        }
    };
}
// Restores the enclosing allowance, less what this one used, also on unwinding.
struct Scope {
    outer: Allowance,
    granted: usize,
}
impl Drop for Scope {
    fn drop(&mut self) {
        STATE.with(|state| {
            let inner = state.allowance.get();
            let mut outer = self.outer;
            if outer.active {
                outer.remaining = outer
                    .remaining
                    .saturating_sub(self.granted.saturating_sub(inner.remaining));
                outer.spent |= inner.spent;
            }
            state.allowance.set(outer);
            state.engage();
        });
    }
}

// The address of a local in a frame of its own, near the top of the stack.
#[inline(never)]
fn stack_address() -> usize {
    let probe = 0u8;
    std::hint::black_box(std::ptr::addr_of!(probe)) as usize
}

/// Run `evaluate` using at most `bytes` of stack below this call, also
/// returning whether it used more: from then on every charge failed.
pub fn guard<T>(bytes: usize, evaluate: impl FnOnce() -> T) -> (T, bool) {
    struct Restore(Option<(usize, usize)>, bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            STATE.with(|state| {
                state.stack.set(self.0);
                state.deep.set(self.1);
                state.engage();
            });
        }
    }
    let _restore = STATE.with(|state| {
        let saved = Restore(state.stack.get(), state.deep.get());
        state.stack.set(Some((stack_address(), bytes)));
        state.deep.set(false);
        state.engage();
        saved
    });
    let result = evaluate();
    (result, STATE.with(|state| state.deep.get()))
}

/// Run `evaluate` with an allowance of `units` (no more than an enclosing
/// allowance has left); [`Spent`] if a charge exceeded it.
pub fn scope<T>(units: usize, evaluate: impl FnOnce() -> T) -> Result<T, Spent> {
    let _scope = STATE.with(|state| {
        let outer = state.allowance.get();
        let granted = if outer.active {
            units.min(outer.remaining)
        } else {
            units
        };
        state.allowance.set(Allowance {
            active: true,
            remaining: granted,
            spent: outer.active && outer.spent,
        });
        state.engage();
        Scope { outer, granted }
    });
    let result = evaluate();
    if STATE.with(|state| state.allowance.get().spent) {
        Err(Spent)
    } else {
        Ok(result)
    }
}

/// Run `evaluate` within whatever allowance an enclosing [`scope`] leaves, or
/// without a limit outside one, also returning the units it charged;
/// [`Spent`] if a charge exceeded that allowance.
pub fn measure<T>(evaluate: impl FnOnce() -> T) -> Result<(T, usize), Spent> {
    let remaining = || STATE.with(|state| state.allowance.get().remaining);
    scope(usize::MAX, || {
        let granted = remaining();
        let result = evaluate();
        (result, granted - remaining())
    })
}

/// Whether an allowance or a guard is active, so that a [`charge`] counts or
/// checks anything. Outside both every charge returns `true` and changes
/// nothing; this holds until the enclosing [`scope`] or [`guard`] ends.
#[inline]
pub fn counting() -> bool {
    STATE.with(|state| state.engaged.get())
}

/// Charge `units` to the current allowance; `false` once it is spent, so the
/// caller stops. Without an allowance this counts nothing and returns `true`.
/// In a guarded pass it also fails once the pass has used more stack than it
/// may.
#[inline]
pub fn charge(units: usize) -> bool {
    STATE.with(|state| !state.engaged.get() || engaged(state, units))
}
// A charge inside an allowance or a guard, kept out of line so that every read
// inlines only the flag test.
#[inline(never)]
fn engaged(state: &State, units: usize) -> bool {
    if let Some((start, bytes)) = state.stack.get() {
        if state.deep.get() {
            return false;
        }
        let probe = 0u8;
        let here = std::hint::black_box(std::ptr::addr_of!(probe)) as usize;
        if start.saturating_sub(here) > bytes {
            state.deep.set(true);
            return false;
        }
    }
    let mut allowance = state.allowance.get();
    if !allowance.active {
        return true;
    }
    if allowance.spent || units > allowance.remaining {
        allowance.spent = true;
        allowance.remaining = 0;
    } else {
        allowance.remaining -= units;
    }
    state.allowance.set(allowance);
    !allowance.spent
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_allowance_is_spent_by_its_charges_and_restored_after() {
        assert!(charge(usize::MAX));
        assert_eq!(scope(3, || (charge(2), charge(1))), Ok((true, true)));
        assert_eq!(scope(3, || (charge(2), charge(2))), Err(Spent));
        assert_eq!(scope(0, || charge(1)), Err(Spent));
        assert_eq!(scope(0, || 7), Ok(7));
        assert!(charge(usize::MAX));
    }
    #[test]
    fn a_measured_pass_counts_its_charges_within_the_enclosing_allowance() {
        assert_eq!(measure(|| (charge(2), charge(3))), Ok(((true, true), 5)));
        assert_eq!(measure(|| 7), Ok((7, 0)));
        let outcome = scope(10, || {
            let inner = measure(|| charge(4));
            (inner, charge(6), charge(1))
        });
        assert_eq!(outcome, Err(Spent));
        assert_eq!(scope(10, || measure(|| charge(11))), Err(Spent));
        assert_eq!(
            scope(10, || measure(|| charge(4)).map(|(_, used)| used)),
            Ok(Ok(4))
        );
        assert!(charge(usize::MAX));
    }
    #[test]
    fn an_inner_allowance_shares_the_outer_one() {
        let outcome = scope(5, || {
            let inner = scope(10, || charge(4));
            (inner, charge(1), charge(1))
        });
        assert_eq!(outcome, Err(Spent));
        let outcome = scope(5, || scope(10, || charge(6)));
        assert_eq!(outcome, Err(Spent));
    }
    #[test]
    fn a_guarded_pass_fails_every_charge_once_it_uses_more_stack_than_it_may() {
        fn nest(levels: usize) -> bool {
            let frame = [0u8; 256];
            std::hint::black_box(&frame);
            if levels == 0 {
                charge(1)
            } else {
                nest(levels - 1) && charge(1)
            }
        }
        assert_eq!(guard(64 * 1024, || nest(8)), (true, false));
        assert_eq!(guard(4 * 1024, || nest(64)), (false, true));
        // The guard holds inside and around an allowance, and ends with it.
        assert_eq!(
            guard(4 * 1024, || scope(1_000_000, || nest(64))),
            (Ok(false), true)
        );
        assert_eq!(
            scope(1_000_000, || guard(4 * 1024, || nest(64))),
            Ok((false, true))
        );
        assert!(nest(64));
    }
    #[test]
    fn a_scope_left_by_unwinding_restores_the_enclosing_allowance() {
        let unwound = std::panic::catch_unwind(|| scope(0, || panic!("scope probe")));
        assert!(unwound.is_err());
        assert!(charge(usize::MAX));
        let outcome = scope(2, || {
            let _ = std::panic::catch_unwind(|| {
                scope(9, || {
                    charge(1);
                    panic!("inner probe")
                })
            });
            charge(1)
        });
        assert_eq!(outcome, Ok(true));
    }
}

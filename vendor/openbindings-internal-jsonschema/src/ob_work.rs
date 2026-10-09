//! OpenBindings qualification patch: evaluator entry work/depth accounting.
//! A bounded call must check its outcome before exposing any engine verdict.
use std::cell::Cell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Work,
    Depth,
    Cycle,
    Diagnostics,
    Arithmetic,
}
#[derive(Clone, Copy)]
struct State {
    active: bool,
    remaining: usize,
    depth: usize,
    max_depth: usize,
    stop: Option<Stop>,
}
const IDLE: State = State {
    active: false,
    remaining: usize::MAX,
    depth: 0,
    max_depth: usize::MAX,
    stop: None,
};
thread_local! {static STATE:Cell<State>=const{Cell::new(IDLE)};}
struct Scope {
    previous: State,
    initial: usize,
}
impl Drop for Scope {
    fn drop(&mut self) {
        let after = STATE.get();
        let mut previous = self.previous;
        if previous.active {
            previous.remaining = previous
                .remaining
                .saturating_sub(self.initial.saturating_sub(after.remaining));
            if previous.stop.is_none() {
                previous.stop = after.stop;
            }
        }
        STATE.set(previous);
    }
}
pub fn bounded<T>(steps: usize, max_depth: usize, f: impl FnOnce() -> T) -> Result<T, Stop> {
    let prior = STATE.get();
    let initial = steps.min(prior.remaining);
    let _scope = Scope {
        previous: prior,
        initial,
    };
    STATE.set(State {
        active: true,
        remaining: initial,
        depth: prior.depth,
        max_depth: max_depth.min(prior.max_depth),
        stop: prior.stop,
    });
    let value = f();
    let after = STATE.get();
    match after.stop {
        Some(reason) => Err(reason),
        None => Ok(value),
    }
}
pub(crate) struct Frame;
impl Drop for Frame {
    fn drop(&mut self) {
        let mut s = STATE.get();
        s.depth = s.depth.saturating_sub(1);
        STATE.set(s);
    }
}
pub(crate) fn enter() -> Option<Frame> {
    let mut s = STATE.get();
    if !s.active {
        return Some(Frame);
    }
    if s.stop.is_none() {
        if s.remaining == 0 {
            s.stop = Some(Stop::Work);
        } else if s.depth >= s.max_depth {
            s.stop = Some(Stop::Depth);
        }
    }
    if s.stop.is_some() {
        STATE.set(s);
        return None;
    }
    s.remaining -= 1;
    s.depth += 1;
    STATE.set(s);
    Some(Frame)
}
pub(crate) fn cycle() {
    let mut s = STATE.get();
    if s.active {
        s.stop.get_or_insert(Stop::Cycle);
    }
    STATE.set(s);
}

/// Charge owned adapter work within the current scope. The enclosing caller must inspect `bounded`.
pub fn charge(amount: usize) -> bool {
    let mut s = STATE.get();
    if !s.active {
        return true;
    }
    if amount > s.remaining {
        s.stop.get_or_insert(Stop::Work);
        s.remaining = 0;
    } else {
        s.remaining -= amount;
    }
    let keep = s.stop.is_none();
    STATE.set(s);
    keep
}
/// A selected arithmetic primitive exceeded its finite admission domain.
pub fn arithmetic_limit() {
    let mut state = STATE.get();
    if state.active {
        state.stop.get_or_insert(Stop::Arithmetic);
    }
    STATE.set(state);
}

// A separate allocation admission counter for the eager dependency diagnostic pass.
#[derive(Clone, Copy)]
struct DiagnosticState {
    active: bool,
    remaining: usize,
    exhausted: bool,
}
thread_local! {static DIAGNOSTICS:Cell<DiagnosticState>=const{Cell::new(DiagnosticState{active:false,remaining:usize::MAX,exhausted:false})};}
struct DiagnosticScope {
    prior: DiagnosticState,
    initial: usize,
}
impl Drop for DiagnosticScope {
    fn drop(&mut self) {
        let after = DIAGNOSTICS.get();
        let mut prior = self.prior;
        if prior.active {
            prior.remaining = prior
                .remaining
                .saturating_sub(self.initial.saturating_sub(after.remaining));
            prior.exhausted |= after.exhausted;
        }
        DIAGNOSTICS.set(prior);
    }
}
pub fn diagnostics<T>(maximum: usize, call: impl FnOnce() -> T) -> Result<T, Stop> {
    let prior = DIAGNOSTICS.get();
    let initial = maximum.min(prior.remaining);
    let _scope = DiagnosticScope { prior, initial };
    DIAGNOSTICS.set(DiagnosticState {
        active: true,
        remaining: initial,
        exhausted: prior.exhausted,
    });
    let value = call();
    if DIAGNOSTICS.get().exhausted {
        Err(Stop::Diagnostics)
    } else {
        Ok(value)
    }
}
/// Delay diagnostic construction until allocation admission succeeds.
pub(crate) fn push_error<T>(errors: &mut Vec<T>, make: impl FnOnce() -> T) {
    if diagnostic_admit() {
        errors.push(make());
    }
}
pub(crate) fn diagnostic_admit() -> bool {
    let mut state = DIAGNOSTICS.get();
    if state.active {
        if state.remaining == 0 {
            state.exhausted = true;
            DIAGNOSTICS.set(state);
            return false;
        }
        state.remaining -= 1;
        DIAGNOSTICS.set(state);
    }
    true
}

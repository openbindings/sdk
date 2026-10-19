//! OpenBindings qualification patch: evaluator entry work/depth accounting, and
//! compilation work accounting for schema builds.
//! A bounded call must check its outcome before exposing any engine verdict.
use std::cell::Cell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Work,
    Depth,
    Cycle,
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

/// Schema compilation work: a scope separate from evaluation's, so meta-validation
/// and evaluation budgets never mix with it. Each compiled schema node is charged
/// with the dynamic scope that keys its caches, and each `$ref` deferral with the
/// in-progress placeholders it copies. Scopes do not nest: an inner one replaces
/// the outer until it returns.
#[derive(Clone, Copy)]
struct Compiling {
    active: bool,
    remaining: usize,
    spent: bool,
    static_anchors: bool,
}
const NOT_COMPILING: Compiling = Compiling {
    active: false,
    remaining: usize::MAX,
    spent: false,
    static_anchors: false,
};
thread_local! {static COMPILING:Cell<Compiling>=const{Cell::new(NOT_COMPILING)};}
struct Restore(Compiling);
impl Drop for Restore {
    fn drop(&mut self) {
        COMPILING.set(self.0);
    }
}
/// Run one schema build with `units` of compilation work. `Err(Stop::Work)` when it
/// needed more; the build stopped at its next compilation and its result is not a
/// prepared validator. `static_anchors` states that the build reaches no
/// `$dynamicAnchor` and no Draft 2019-09 `$recursiveAnchor`: no reference then
/// resolves through the dynamic scope, so compiled schemas are cached by location
/// alone instead of once per scope, and a reference that does fails the build.
pub fn compiling<T>(units: usize, static_anchors: bool, f: impl FnOnce() -> T) -> Result<T, Stop> {
    let _restore = Restore(COMPILING.get());
    COMPILING.set(Compiling {
        active: true,
        remaining: units,
        spent: false,
        static_anchors,
    });
    let value = f();
    if COMPILING.get().spent {
        Err(Stop::Work)
    } else {
        Ok(value)
    }
}
/// Charge compilation work; `false` once the current scope is spent.
pub(crate) fn compile_charge(amount: usize) -> bool {
    let mut state = COMPILING.get();
    if !state.active {
        return true;
    }
    if state.spent || amount > state.remaining {
        state.spent = true;
        state.remaining = 0;
    } else {
        state.remaining -= amount;
    }
    COMPILING.set(state);
    !state.spent
}
/// Whether the current build's caller stated that its resources declare no dynamic anchor.
pub(crate) fn static_anchors() -> bool {
    COMPILING.get().static_anchors
}
/// Run `f` outside any compilation scope: schema meta-validation is evaluation.
pub(crate) fn uncharged<T>(f: impl FnOnce() -> T) -> T {
    let _restore = Restore(COMPILING.get());
    COMPILING.set(NOT_COMPILING);
    f()
}

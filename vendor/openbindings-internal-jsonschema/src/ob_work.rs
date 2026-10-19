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
/// and evaluation budgets never mix with it. Work is charged by what the build does,
/// with the weights in [`cost`]. Scopes do not nest: an inner one replaces the outer
/// until it returns.
#[derive(Clone, Copy)]
struct Compiling {
    active: bool,
    remaining: u64,
    spent: bool,
    static_anchors: bool,
}
const NOT_COMPILING: Compiling = Compiling {
    active: false,
    remaining: u64::MAX,
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
pub fn compiling<T>(units: u64, static_anchors: bool, f: impl FnOnce() -> T) -> Result<T, Stop> {
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
/// What compilation work costs, in units of about a nanosecond of compilation time
/// or 0.7 bytes of the memory it allocates, whichever is larger, in the reference
/// release build. Each weight was fitted to measured builds of many contract shapes;
/// the charge is a sum of counts, never a measurement, so it is the same everywhere.
pub(crate) mod cost {
    /// A compiled schema node, or the node built for one `dependentRequired` entry.
    pub(crate) const NODE: u64 = 830;
    /// Each byte of a compiled node's location, which its keywords copy and its caches hash.
    pub(crate) const LOCATION_BYTE: u64 = 8;
    /// Each entry of the dynamic scope keying a node's caches.
    pub(crate) const SCOPE_ENTRY: u64 = 30;
    /// A `$ref` or `$dynamicRef` resolved and entered.
    pub(crate) const REFERENCE: u64 = 750;
    /// Each entry of the dynamic scope searched for a reference's dynamic anchor.
    pub(crate) const ANCHOR_SEARCH: u64 = 440;
    /// Each in-progress placeholder copied when a `$ref` target is deferred.
    pub(crate) const PLACEHOLDER: u64 = 10;
    /// A property name copied out of `required` or `dependentRequired`.
    pub(crate) const NAME: u64 = 110;
    /// Each byte of a copied property name.
    pub(crate) const NAME_BYTE: u64 = 2;
    /// Each byte of a `pattern`, which every compilation of its schema analyzes and copies.
    pub(crate) const PATTERN_BYTE: u64 = 1;

    /// A node at a location of `location_bytes`, its caches keyed by `scope_entries`.
    pub(crate) fn node(location_bytes: usize, scope_entries: usize) -> u64 {
        NODE + LOCATION_BYTE * location_bytes as u64 + SCOPE_ENTRY * scope_entries as u64
    }
    /// `count` property names of `bytes` in all.
    pub(crate) fn names(count: usize, bytes: usize) -> u64 {
        NAME * count as u64 + NAME_BYTE * bytes as u64
    }
    /// The node for a `dependentRequired` entry, whose name is `name_bytes` long.
    pub(crate) fn entry(name_bytes: usize) -> u64 {
        NODE + NAME_BYTE * name_bytes as u64
    }
}
/// Charge compilation work; `false` once the current scope is spent.
pub(crate) fn compile_charge(amount: u64) -> bool {
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

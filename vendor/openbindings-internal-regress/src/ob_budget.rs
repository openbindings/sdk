//! Qualification patch: bounded interpreter work per synchronous call.
use std::cell::Cell;
#[derive(Clone, Copy)]
struct State {
    remaining: usize,
    exceeded: bool,
    active: bool,
}
const IDLE: State = State {
    remaining: usize::MAX,
    exceeded: false,
    active: false,
};
thread_local! {static STATE:Cell<State>=const{Cell::new(IDLE)};}
#[derive(Debug)]
pub struct Exhausted;
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
            previous.exceeded |= after.exceeded;
        }
        STATE.set(previous);
    }
}
pub fn run<T>(limit: usize, action: impl FnOnce() -> T) -> Result<T, Exhausted> {
    let previous = STATE.get();
    let initial = previous.remaining.min(limit);
    let _scope = Scope { previous, initial };
    STATE.set(State {
        remaining: initial,
        exceeded: previous.exceeded,
        active: true,
    });
    let result = action();
    if STATE.get().exceeded {
        Err(Exhausted)
    } else {
        Ok(result)
    }
}
pub(crate) fn tick() -> bool {
    let mut state = STATE.get();
    if state.remaining == 0 {
        state.exceeded = true;
    } else {
        state.remaining -= 1;
    }
    STATE.set(state);
    !state.exceeded
}
/// Nested entry points consume their caller's scope; unwinding restores it too.
pub fn top_level<T>(limit: usize, action: impl FnOnce() -> T) -> Result<T, Exhausted> {
    run(limit, action)
}

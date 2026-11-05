//! Nested/unwind-safe stop state for the closed value backend.
use super::pattern::Unsupported;
use std::cell::Cell;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Arithmetic,
    Unmatched(Unsupported),
}
thread_local! {static STOP:Cell<Option<Stop>>=const{Cell::new(None)};}
struct Restore(Option<Stop>);
impl Drop for Restore {
    fn drop(&mut self) {
        STOP.set(self.0);
    }
}
pub(crate) fn scope<T>(evaluate: impl FnOnce() -> T) -> (T, Option<Stop>) {
    let _restore = Restore(STOP.replace(None));
    let value = evaluate();
    (value, STOP.get())
}
pub(crate) fn unmatched(reason: Unsupported) {
    if STOP.get().is_none() {
        STOP.set(Some(Stop::Unmatched(reason)));
    }
}
pub(crate) fn arithmetic() {
    STOP.set(Some(Stop::Arithmetic));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_and_unwound_stops_restore_the_enclosing_pass() {
        let (_, outer) = scope(|| {
            arithmetic();
            let (_, inner) = scope(|| ());
            assert_eq!(inner, None);
            let _ = std::panic::catch_unwind(|| scope(|| panic!("probe")));
        });
        assert_eq!(outer, Some(Stop::Arithmetic));
        assert_eq!(scope(|| ()).1, None);
    }
}

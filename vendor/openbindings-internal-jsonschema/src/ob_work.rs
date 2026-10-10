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
        let value = make();
        if !metadata_exhausted() {
            errors.push(value);
        }
    }
}
pub(crate) fn diagnostic_admit() -> bool {
    if metadata_exhausted() {
        return false;
    }
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

// The SDK needs only keyword/type metadata, instance pointers and original schema
// coordinates. This mode avoids unused upstream error payloads and admits copied
// strings before allocation. It is never active during the verdict pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiagnosticUsage {
    pub copied_bytes: usize,
    pub rejected_copies: usize,
    /// Optional schema operands omitted before copying; does not suppress base errors.
    pub omitted_optional_copies: usize,
    pub collection_items: usize,
    pub collection_truncated: bool,
}
#[derive(Clone, Copy)]
struct MetadataState {
    active: bool,
    remaining: usize,
    items_remaining: usize,
    usage: DiagnosticUsage,
}
thread_local! {static METADATA:Cell<MetadataState>=const{Cell::new(MetadataState{
    active:false, remaining:usize::MAX, items_remaining:usize::MAX,
    usage:DiagnosticUsage{copied_bytes:0,rejected_copies:0,omitted_optional_copies:0,collection_items:0,collection_truncated:false}
})};}
struct MetadataScope {
    prior: MetadataState,
    initial_bytes: usize,
    initial_items: usize,
}
impl Drop for MetadataScope {
    fn drop(&mut self) {
        let after = METADATA.get();
        let mut prior = self.prior;
        if prior.active {
            prior.remaining = prior
                .remaining
                .saturating_sub(self.initial_bytes.saturating_sub(after.remaining));
            prior.items_remaining = prior
                .items_remaining
                .saturating_sub(self.initial_items.saturating_sub(after.items_remaining));
            prior.usage.copied_bytes = prior
                .usage
                .copied_bytes
                .saturating_add(after.usage.copied_bytes);
            prior.usage.rejected_copies = prior
                .usage
                .rejected_copies
                .saturating_add(after.usage.rejected_copies);
            prior.usage.omitted_optional_copies = prior
                .usage
                .omitted_optional_copies
                .saturating_add(after.usage.omitted_optional_copies);
            prior.usage.collection_items = prior
                .usage
                .collection_items
                .saturating_add(after.usage.collection_items);
            prior.usage.collection_truncated |= after.usage.collection_truncated;
        }
        METADATA.set(prior);
    }
}
/// Internal adapter mode; returned usage counts admitted logical string bytes,
/// not allocator bytes, capacities, input storage or total heap.
pub fn diagnostic_metadata<T>(
    bytes: usize,
    items: usize,
    call: impl FnOnce() -> T,
) -> (T, DiagnosticUsage) {
    let prior = METADATA.get();
    let _scope = MetadataScope {
        prior,
        initial_bytes: bytes.min(prior.remaining),
        initial_items: items.min(prior.items_remaining),
    };
    METADATA.set(MetadataState {
        active: true,
        remaining: bytes.min(prior.remaining),
        items_remaining: items.min(prior.items_remaining),
        usage: DiagnosticUsage::default(),
    });
    let result = call();
    (result, METADATA.get().usage)
}
pub(crate) fn metadata_only() -> bool {
    METADATA.get().active
}
pub(crate) fn metadata_exhausted() -> bool {
    METADATA.get().usage.rejected_copies != 0
}
pub(crate) fn diagnostic_copy(bytes: usize) -> bool {
    let mut state = METADATA.get();
    if !state.active {
        return true;
    }
    if metadata_exhausted() || bytes > state.remaining {
        state.usage.rejected_copies = state.usage.rejected_copies.saturating_add(1);
        METADATA.set(state);
        return false;
    }
    state.remaining -= bytes;
    state.usage.copied_bytes += bytes;
    METADATA.set(state);
    true
}
pub(crate) fn push_diagnostic_name(names: &mut Vec<String>, name: &str) {
    if !metadata_only() {
        push_error(names, || name.to_owned());
        return;
    }
    if diagnostic_collection_item() && diagnostic_copy(name.len()) {
        names.push(name.to_owned());
    }
}
/// Defer payload cloning which the metadata consumer never observes.
pub(crate) fn diagnostic_payload<T: Default>(make: impl FnOnce() -> T) -> T {
    if metadata_only() {
        T::default()
    } else {
        make()
    }
}

pub(crate) fn diagnostic_collection_item() -> bool {
    let mut state = METADATA.get();
    if !state.active {
        return diagnostic_admit();
    }
    if state.items_remaining == 0 {
        state.usage.collection_truncated = true;
        METADATA.set(state);
        return false;
    }
    state.items_remaining -= 1;
    state.usage.collection_items += 1;
    METADATA.set(state);
    true
}

// Only the selected SDK detail pass can request source operands. Nested calls
// restore their caller's choice, including unwinding. The verdict pass is unchanged.
thread_local! { static SCHEMA_DETAILS: Cell<bool> = const { Cell::new(false) }; }
/// Private adapter switch for admitted required-name metadata, default disabled.
pub fn diagnostic_schema_details<T>(enabled: bool, call: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCHEMA_DETAILS.set(self.0);
        }
    }
    let _restore = Restore(SCHEMA_DETAILS.replace(enabled));
    call()
}
/// Preserve one established missing member only after optional scratch admission.
/// Null is a deliberate unavailable/omitted operand, never a fabricated name.
pub(crate) fn diagnostic_required_name(name: &str) -> serde_json::Value {
    if !metadata_only() {
        return serde_json::Value::String(name.to_owned());
    }
    if !SCHEMA_DETAILS.get() {
        return serde_json::Value::Null;
    }
    let mut state = METADATA.get();
    if metadata_exhausted() || name.len() > state.remaining {
        state.usage.omitted_optional_copies = state.usage.omitted_optional_copies.saturating_add(1);
        METADATA.set(state);
        return serde_json::Value::Null;
    }
    state.remaining -= name.len();
    state.usage.copied_bytes += name.len();
    METADATA.set(state);
    serde_json::Value::String(name.to_owned())
}

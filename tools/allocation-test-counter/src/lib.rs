//! Test-only requested-live-heap meter. Production crates never depend on it.
//! Counters are thread-local: parallel test cases cannot charge one another.
#![deny(unsafe_op_in_unsafe_fn)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::cell::RefCell;
struct Events {
    active: bool,
    len: usize,
    items: [isize; 4096],
}
thread_local! {static EVENTS:RefCell<Events>=const{RefCell::new(Events{active:false,len:0,items:[0;4096]})};}
#[derive(Clone, Copy, Default)]
struct State {
    held: isize,
    peak: isize,
}
thread_local! { static STATE: Cell<State> = const { Cell::new(State { held: 0, peak: 0 }) }; }
fn change(delta: isize) {
    let _ = EVENTS.try_with(|events| {
        let mut e = events.borrow_mut();
        if e.active && e.len < 4096 {
            let i = e.len;
            e.items[i] = delta;
            e.len += 1;
        }
    });
    let _ = STATE.try_with(|cell| {
        let mut state = cell.get();
        state.held += delta;
        state.peak = state.peak.max(state.held);
        cell.set(state);
    });
}
pub fn trace<T>(run: impl FnOnce() -> T) -> (T, [isize; 4096], usize) {
    EVENTS.with(|events| {
        let mut e = events.borrow_mut();
        e.active = true;
        e.len = 0;
    });
    let result = run();
    let (items, len) = EVENTS.with(|events| {
        let mut e = events.borrow_mut();
        e.active = false;
        (e.items, e.len)
    });
    (result, items, len)
}
pub struct Meter;
// SAFETY: System owns every allocation; the counter never dereferences it and
// const-initialized TLS bookkeeping cannot allocate recursively. Successful
// reallocations replace the old request. Realloc overlap/metadata are excluded.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            change(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        change(-(layout.size() as isize));
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, old, size) };
        if !q.is_null() {
            change(size as isize - old.size() as isize);
        }
        q
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub peak: usize,
    pub retained: isize,
}
pub fn measure<T>(run: impl FnOnce() -> T) -> (T, Sample) {
    let baseline = STATE.with(|cell| {
        let mut s = cell.get();
        s.peak = s.held;
        cell.set(s);
        s.held
    });
    let result = run();
    let sample = STATE.with(|cell| {
        let s = cell.get();
        Sample {
            peak: (s.peak - baseline) as usize,
            retained: s.held - baseline,
        }
    });
    (result, sample)
}

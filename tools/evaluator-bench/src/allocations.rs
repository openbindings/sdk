//! Requested System heap, including a visible workload-carrying baseline.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
pub struct Counter;
fn add(size: usize) {
    PEAK.fetch_max(LIVE.fetch_add(size, Relaxed) + size, Relaxed);
    COUNT.fetch_add(1, Relaxed);
    BYTES.fetch_add(size, Relaxed);
}
// SAFETY: forward every pointer/layout pair unchanged to System; the atomics
// only count successful allocations and never allocate or access their contents.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add(layout.size());
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            add(layout.size());
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
            add(size);
        }
        next
    }
}
pub fn start() -> [usize; 3] {
    let live = LIVE.load(Relaxed);
    PEAK.store(live, Relaxed);
    [live, COUNT.load(Relaxed), BYTES.load(Relaxed)]
}
pub fn finish(start: [usize; 3], iterations: usize) -> serde_json::Value {
    let peak = PEAK.load(Relaxed);
    let live = LIVE.load(Relaxed);
    let count = COUNT.load(Relaxed) - start[1];
    let bytes = BYTES.load(Relaxed) - start[2];
    serde_json::json!({"baseline_live_bytes":start[0], "peak_live_bytes":peak,
        "incremental_peak_bytes":peak.saturating_sub(start[0]), "ending_live_bytes":live,
        "allocations_per_op":count as f64 / iterations as f64,
        "requested_bytes_per_op":bytes as f64 / iterations as f64})
}

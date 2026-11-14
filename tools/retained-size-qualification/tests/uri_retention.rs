//! Native qualification of the public snapshot estimate against live heap bytes.
#![allow(unsafe_code)]
use openbindings::JsonValue;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

struct Counter;
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
}
fn record(delta: isize) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        LIVE.with(|live| live.set(live.get() + delta));
    }
}
// SAFETY: forwards each allocator argument and result unchanged. Accounting
// uses only thread-local integers and never allocates or touches the pointers.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size() as isize);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record(layout.size() as isize);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, size) };
        if !result.is_null() {
            record(size as isize - layout.size() as isize);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

#[test]
fn dropped_resource_sets_do_not_leave_caller_uris_in_a_global_cache() {
    let value = JsonValue::parse("true").unwrap();
    let register = |uri: String| {
        openbindings::ExternalSchemas::new([openbindings::ExternalSchema {
            uri,
            document: value.clone(),
        }])
        .unwrap()
    };
    drop(register("https://uri-retention.test/warm".into()));
    LIVE.with(|v| v.set(0));
    ACTIVE.with(|v| v.set(true));
    for i in 0..2187 {
        drop(register(format!("https://uri-retention.test/resource/{i}")));
    }
    ACTIVE.with(|v| v.set(false));
    let retained = LIVE.with(Cell::get);
    eprintln!("retained_after_2187_dropped_sets={retained}");
    assert_eq!(
        retained, 0,
        "no caller identifier survives its resource set"
    );
}

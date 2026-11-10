//! Native qualification of the public snapshot estimate against live heap bytes.
#![allow(unsafe_code)]
use openbindings::Document;
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

fn check(document: &Document) {
    let measured = LIVE.with(Cell::get) as usize;
    let estimate = document.retained_bytes();
    // Collection node occupancy is not public. The estimate must stay within
    // 25% of requested live allocation bytes (allocator overhead is excluded).
    assert!(
        estimate.abs_diff(measured) <= measured / 4,
        "estimate {estimate}, measured {measured}"
    );
}
#[test]
fn snapshot_estimate_tracks_live_storage_before_and_after_lazy_lookups() {
    let typical = r##"{"openbindings":"0.2.0","operations":{"echo":{"aliases":["repeat"],"input":{"$ref":"#/schemas/text"},"examples":{"one":{"input":"hello"}}}},"sources":{"s":{"kind":"example.echo"}},"bindings":{"b":{"operation":"echo","source":"s"}},"dependencies":{"d":{"operation":"echo"}},"schemas":{"text":{"type":"string"}}}"##;
    let dense = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-nodes":[{}]}}"#,
        vec!["0"; 4096].join(",")
    );
    let refunds = include_str!("fixtures/acme-refunds.obi.json");
    // Exclude process-wide packaged schemas, which no individual snapshot owns.
    for text in [typical, refunds] {
        let document = Document::parse(text).unwrap();
        assert!(document.validate().unwrap().conformant().is_some());
        document.references();
    }
    for text in [typical, dense.as_str(), refunds] {
        LIVE.with(|live| live.set(0));
        ACTIVE.with(|active| active.set(true));
        let document = Document::parse(text).unwrap();
        check(&document);
        {
            let proof = document.validate().unwrap().conformant().unwrap();
            for operation in proof.operations() {
                operation.examples();
                operation.bindings();
            }
            proof.source("s");
            proof.dependency("d");
            document.references();
            let mut last = document.value().view();
            loop {
                if let Some(member) = last.members().and_then(Iterator::last) {
                    last = member.value;
                } else if let Some(element) = last.elements().and_then(Iterator::last) {
                    last = element;
                } else {
                    break;
                }
            }
            last.location();
            check(&document);
            assert_eq!(document.retained_bytes(), document.clone().retained_bytes());
            assert_eq!(document.retained_bytes(), proof.document().retained_bytes());
        }
        drop(document);
        ACTIVE.with(|active| active.set(false));
        assert_eq!(LIVE.with(Cell::get), 0, "snapshot storage is released");
    }
}

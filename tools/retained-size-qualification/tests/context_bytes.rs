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

use openbindings::JsonValue;
use openbindings_schema_comparison::{
    ComparisonContext, ComparisonLimits, OperationRole, check_interface,
};

fn source(count: usize) -> String {
    let operations = (0..count).map(|i| format!(
        r#""op{i}":{{"input":{{"$ref":"https://schema.test/external"}},"output":{{"$ref":"https://schema.test/external"}}}}"#,
    )).collect::<Vec<_>>().join(",");
    format!(r#"{{"openbindings":"0.2.0","operations":{{{operations}}}}}"#)
}
fn context(source: &str, warm: bool) -> ComparisonContext {
    let proof = Document::parse(source)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let resource = JsonValue::parse(
        r#"{"type":"object","properties":{"s":{"type":"string"}},"required":["s"]}"#,
    )
    .unwrap();
    let context = ComparisonContext::new(proof.document().value().clone())
        .with_resource("https://schema.test/external", resource.clone())
        .with_resource("https://schema.test/alias", resource);
    if warm {
        drop(
            check_interface(
                &proof,
                &proof,
                &context,
                &context,
                &|_, _| Some(OperationRole::Realized),
                &ComparisonLimits::default(),
            )
            .unwrap(),
        );
    }
    context
}

#[test]
fn context_estimates_track_live_bytes_at_ten_one_hundred_and_five_hundred_operations() {
    let mut previous = [0, 0];
    for count in [10, 100, 500] {
        let text = source(count);
        drop(context(&text, true)); // Packaged schemas are not context-owned.
        for (slot, warm) in [false, true].into_iter().enumerate() {
            LIVE.with(|live| live.set(0));
            ACTIVE.with(|active| active.set(true));
            let owner = context(&text, warm);
            let estimate = owner.retained_bytes();
            // A first lock can allocate platform mutex backing, as in Document's
            // estimate. Repeated walks retain no additional bookkeeping.
            let measured = LIVE.with(Cell::get) as usize;
            assert_eq!(estimate, owner.clone().retained_bytes());
            assert_eq!(LIVE.with(Cell::get) as usize, measured);
            assert!(estimate >= text.len());
            assert!(estimate > previous[slot]);
            previous[slot] = estimate;
            drop(owner);
            let released = LIVE.with(Cell::get);
            ACTIVE.with(|active| active.set(false));
            assert_eq!(released, 0);
            println!(
                "operations={count}, warm={warm}, estimate={estimate}, live={measured}, ratio={:.3}",
                estimate as f64 / measured as f64
            );
            assert!(estimate <= measured * 2 && measured <= estimate * 2);
        }
    }
}

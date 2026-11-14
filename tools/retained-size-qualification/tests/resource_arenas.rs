//! Native qualification of the public snapshot estimate against live heap bytes.
#![allow(unsafe_code)]
use openbindings::JsonValue;
use openbindings_internal_json::backend;
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
fn retained_arena_reservation_is_conservative_within_five_percent() {
    // Same dense-const generator as the resource stress probe, plus its depth,
    // members, escaped UTF-16, and long-token shapes. Allocate fixtures outside
    // the accounting interval. A 1.00..1.05 factor covers the future position
    // checkpoints; allocator bookkeeping is excluded on both sides.
    let fixtures = [
        format!("{{\"const\":[{}]}}", vec!["0"; 16_384].join(",")),
        format!("{}0{}", "[".repeat(6561), "]".repeat(6561)),
        format!(
            "{{{}}}",
            (0..6561)
                .map(|i| format!("\"k{i}\":0"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        format!("\"{}\"", "\\u0061".repeat(65_536)),
        format!("\"{}\"", "\\ud800".repeat(65_536)),
        format!("1e{}", "9".repeat(65_536)),
        format!("{}0{}", "{\"a\":".repeat(6561), "}".repeat(6561)),
        format!("{}0{}", "{\"a\":[".repeat(2187), "]}".repeat(2187)),
        format!("[{}]", vec!["{\"a\":[1,2,3]}"; 6561].join(",")),
        format!("{{{}}}", vec!["\"duplicate\":0"; 6561].join(",")),
        format!("{{\"{}\":0}}", "k".repeat(65_536)),
        format!("\"{}\"", "s".repeat(65_536)),
        format!("\"{}\"", "\\u0000".repeat(65_536)),
        format!(
            "{{{}}}",
            (0..6561)
                .map(|i| format!("\"é{i}\":0,\"e\\u0301{i}\":1"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        "7".repeat(65_536),
        format!("{}true", " ".repeat(65_536)),
    ];
    for text in fixtures {
        LIVE.with(|n| n.set(0));
        ACTIVE.with(|a| a.set(true));
        let value = JsonValue::parse(&text).unwrap();
        let estimate = backend::retained_reservation(&value);
        let measured = LIVE.with(Cell::get) as usize;
        assert!(
            estimate >= measured && estimate <= measured + measured / 20,
            "source={} measured={measured} reservation={estimate}",
            text.len()
        );
        let shared = value.clone();
        assert_eq!(
            backend::occurrence(value.view()).0,
            backend::occurrence(shared.view()).0
        );
        assert_eq!(estimate, backend::retained_reservation(&shared));
        // Lazy position lookup must fit the upfront allowance too.
        value
            .get("k6560")
            .unwrap_or_else(|| value.view())
            .location();
        assert!(
            LIVE.with(Cell::get) as usize <= estimate,
            "location source={} live={} estimate={estimate}",
            text.len(),
            LIVE.with(Cell::get)
        );
        drop(shared);
        drop(value);
        ACTIVE.with(|a| a.set(false));
        assert_eq!(LIVE.with(Cell::get), 0);
        eprintln!(
            "source={} measured={measured} reservation={estimate} factor={:.6}",
            text.len(),
            estimate as f64 / measured as f64
        );
    }
}

#[test]
fn small_owners_include_fixed_overhead_within_a_factor_of_one_point_five() {
    for text in [
        "null",
        "true",
        "0",
        "{}",
        "[]",
        r#""s""#,
        r#"{"a":[0,1,2]}"#,
    ] {
        LIVE.with(|n| n.set(0));
        ACTIVE.with(|a| a.set(true));
        let value = JsonValue::parse(text).unwrap();
        let estimate = backend::retained_reservation(&value);
        let measured = LIVE.with(Cell::get) as usize;
        assert!(estimate >= measured && estimate <= measured + measured / 2);
        drop(value);
        ACTIVE.with(|a| a.set(false));
        assert_eq!(LIVE.with(Cell::get), 0);
        eprintln!(
            "small_source={} measured={measured} reservation={estimate} factor={:.6}",
            text.len(),
            estimate as f64 / measured as f64
        );
    }
}

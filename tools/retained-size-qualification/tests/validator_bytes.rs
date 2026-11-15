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

use openbindings::{ExternalSchema, ExternalSchemas, JsonValue, PreparedSchema};
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};

fn source(count: usize) -> String {
    let operations = (0..count).map(|i| format!(
        r#""op{i}":{{"input":{{"$ref":"https://schema.test/external"}},"output":{{"$ref":"https://schema.test/external"}}}}"#,
    )).collect::<Vec<_>>().join(",");
    format!(r#"{{"openbindings":"0.2.0","operations":{{{operations}}}}}"#)
}
fn prepare(source: &str, sdk: bool, resource: &str) -> PreparedSchema<DefaultValidator> {
    let proof = Document::parse(source)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let value = JsonValue::parse(resource).unwrap();
    let external = ExternalSchemas::new([
        ExternalSchema {
            uri: "https://schema.test/external".into(),
            document: value.clone(),
        },
        ExternalSchema {
            uri: "https://schema.test/alias".into(),
            document: value,
        },
    ])
    .unwrap();
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_sdk_work_units: sdk.then_some(1_000_000),
        ..Limits::default()
    });
    proof
        .resolve_operation("op0")
        .unwrap()
        .input()
        .unwrap()
        .prepare(&evaluator, &external)
        .unwrap()
}

#[test]
fn validator_estimates_track_live_bytes_and_report_opaque_library_storage() {
    let resource = r#"{"type":"object","properties":{"s":{"type":"string"}},"required":["s"]}"#;
    let mut previous = 0;
    for count in [10, 100, 500] {
        let text = source(count);
        for sdk in [false, true] {
            drop(prepare(&text, sdk, resource));
            LIVE.with(|live| live.set(0));
            ACTIVE.with(|active| active.set(true));
            let owner = prepare(&text, sdk, resource);
            let estimate = owner.retained_bytes();
            let measured = LIVE.with(Cell::get) as usize;
            assert_eq!(estimate, owner.clone().retained_bytes());
            assert_eq!(LIVE.with(Cell::get) as usize, measured);
            drop(owner);
            let released = LIVE.with(Cell::get);
            ACTIVE.with(|active| active.set(false));
            assert_eq!(released, 0);
            println!(
                "operations={count}, sdk={sdk}, estimate={estimate:?}, live={measured}, ratio={:?}",
                estimate.map(|bytes| bytes as f64 / measured as f64)
            );
            if sdk {
                let estimate = estimate.expect("SDK lane owns inspectable storage");
                assert!(estimate >= text.len() + resource.len());
                assert!(estimate > previous);
                assert!(estimate <= measured * 2 && measured <= estimate * 2);
                previous = estimate;
            } else {
                assert_eq!(estimate, None);
            }
        }
    }
}

#[test]
fn sdk_estimates_cover_shared_patterns_and_wide_numeric_operands() {
    for resource in [
        r#"{"type":"number","minimum":123456789012345678901234567890e12345678901234567890}"#,
        r#"{"type":"object","properties":{"a":{"pattern":"^[a-z]+$"},"b":{"pattern":"^[a-z]+$"},"c":{"pattern":"a|bc"},"d":{"pattern":"a|bc"}}}"#,
        r#"{"$ref":"https://schema.test/missing"}"#,
    ] {
        let text = source(10);
        drop(prepare(&text, true, resource));
        LIVE.with(|live| live.set(0));
        ACTIVE.with(|active| active.set(true));
        let owner = prepare(&text, true, resource);
        let estimate = owner.retained_bytes().unwrap();
        let measured = LIVE.with(Cell::get) as usize;
        drop(owner);
        let released = LIVE.with(Cell::get);
        ACTIVE.with(|active| active.set(false));
        assert_eq!(released, 0);
        assert!(estimate >= text.len() + resource.len());
        assert!(estimate <= measured * 2 && measured <= estimate * 2);
    }
}

#[test]
fn sdk_estimate_includes_supplied_root_storage_after_inputs_drop() {
    fn owner(text: &str) -> PreparedSchema<DefaultValidator> {
        let external = ExternalSchemas::new([ExternalSchema {
            uri: "https://schema.test/root".into(),
            document: JsonValue::parse(text).unwrap(),
        }])
        .unwrap();
        external
            .prepare(
                "https://schema.test/root",
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_work_units: Some(1_000_000),
                    ..Limits::default()
                }),
            )
            .unwrap()
    }
    let text = format!(
        r#"{{"type":"string","description":"{}"}}"#,
        "x".repeat(32_768)
    );
    drop(owner(&text));
    LIVE.with(|live| live.set(0));
    ACTIVE.with(|active| active.set(true));
    let prepared = owner(&text);
    let estimate = prepared.retained_bytes().unwrap();
    let measured = LIVE.with(Cell::get) as usize;
    drop(prepared);
    let released = LIVE.with(Cell::get);
    ACTIVE.with(|active| active.set(false));
    assert_eq!(released, 0);
    assert!(estimate >= text.len());
    assert!(estimate <= measured * 2 && measured <= estimate * 2);
}

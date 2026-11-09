//! Isolated global allocator and private compiled-lane measurements.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    calls: usize,
    bytes: usize,
}
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts { calls: 0, bytes: 0 }) };
}
struct Counter;
fn record(size: usize) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        COUNTS.with(|counts| {
            let previous = counts.get();
            counts.set(Counts {
                calls: previous.calls + 1,
                bytes: previous.bytes + size,
            });
        });
    }
}
// SAFETY: every allocator argument and return value is forwarded unchanged.
// Counters use only thread-local integers and never allocate.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            record(size);
        }
        pointer
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
static HEALTHY: AtomicBool = AtomicBool::new(false);

fn prepare(schema: &str) -> DefaultValidator {
    let resources = ExternalSchemas::default()
        .with(
            "https://allocation.invalid/root",
            JsonValue::parse(schema).unwrap(),
        )
        .unwrap();
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://allocation.invalid/root"}}}}"#)
        .unwrap().validate().unwrap();
    document
        .conformant()
        .unwrap()
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &resources,
            &PreparationLimits::default(),
        )
        .unwrap()
        .into_validator()
}

fn sample(lane: &compiled::Lane, value: &JsonValue) -> Counts {
    compiled::entries::take();
    COUNTS.set(Counts::default());
    ACTIVE.set(true);
    let result = lane.outcome(value, &HEALTHY);
    ACTIVE.set(false);
    let counts = COUNTS.get();
    assert_eq!(compiled::entries::take(), [0, 0, 1]);
    assert!(matches!(
        result,
        Some(ValueOutcome::Satisfies | ValueOutcome::Fails { .. })
    ));
    counts
}

fn check(name: &str, schema: &str, value: impl Fn(usize) -> String) {
    let owner = prepare(schema);
    let lane = if std::env::var_os("OB_LITERAL_SCRATCH_BYPASS").is_some() {
        owner.compiled.without_scratch_estimate()
    } else {
        owner.compiled.clone()
    };
    let mut baseline = None;
    for size in [1024, 1024 * 1024, 16 * 1024 * 1024] {
        let mut text = value(size);
        // Padding stays inside the root token, so bytes() measures the target.
        let padding = " ".repeat(size.saturating_sub(text.len()));
        text.insert_str(text.len() - 1, &padding);
        let value = JsonValue::parse(text).unwrap();
        assert_eq!(value.bytes().len(), size);
        let cold = sample(&lane, &value);
        let warm = sample(&lane, &value);
        println!(
            "allocation {name} value_bytes={} cold={cold:?} warm={warm:?}",
            value.bytes().len()
        );
        assert_eq!(cold, warm, "{name}: first and repeated pass differ");
        assert_eq!(
            *baseline.get_or_insert(cold),
            cold,
            "{name}: allocation grows with value"
        );
    }
}

#[test]
fn allocations_do_not_grow() {
    check("const-string", r#"{"const":"x"}"#, |size| {
        format!("\"{}\"", "x".repeat(size - 2))
    });
    for choices in [1, 9, 10, 11, 32] {
        let schema = serde_json::json!({"enum": (0..choices).map(|i| format!("choice{i}")).collect::<Vec<_>>()});
        check(&format!("enum-{choices}"), &schema.to_string(), |size| {
            format!("\"{}\"", "x".repeat(size - 2))
        });
        check(
            &format!("enum-escaped-{choices}"),
            &schema.to_string(),
            |size| format!("\"{}\"", r"\u0078".repeat((size - 2) / 6)),
        );
    }
    check("const-escaped", r#"{"const":"\u0078\n"}"#, |size| {
        format!("\"{}\"", r"\u0078\n".repeat((size - 2) / 8))
    });
    check(
        "bounded-integers",
        r#"{"type":"array","items":{"type":"integer","minimum":0,"maximum":999999999999999999}}"#,
        |size| format!("[{}0]", "123456789012345678,".repeat((size - 3) / 19)),
    );
    check(
        "i64-limits",
        r#"{"items":{"type":"integer","minimum":-9223372036854775808,"maximum":9223372036854775807}}"#,
        |size| {
            let pair = "-9223372036854775808,9223372036854775807,";
            format!("[{}0]", pair.repeat((size - 3) / pair.len()))
        },
    );
    check("composite-vs-string", r#"{"const":"x"}"#, |size| {
        format!("[{}0]", "123456789012345678,".repeat((size - 3) / 19))
    });
    let literal = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n";
    let schema = serde_json::json!({"items":{"const":literal}}).to_string();
    check("matching-escaped-items", &schema, |size| {
        let item = r#""xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\u000a""#;
        format!(
            "[{}{}]",
            format!("{item},").repeat((size - item.len() - 2) / (item.len() + 1)),
            item
        )
    });
}

fn time(owner: &DefaultValidator, value: &JsonValue) -> u128 {
    let started = std::time::Instant::now();
    let result = std::hint::black_box(owner.evaluate(value, Detail::Outcome, &HEALTHY));
    let elapsed = started.elapsed().as_nanos();
    assert!(matches!(
        result,
        ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
    ));
    elapsed
}

fn hostile(name: &str, owner: &DefaultValidator, value: &JsonValue) {
    let original = DefaultValidator {
        plan: owner.plan.clone(),
        compiled: compiled::Lane::Original("forced-old-path"),
    };
    compiled::entries::take();
    let selected = owner.evaluate(value, Detail::Outcome, &HEALTHY);
    let route = compiled::entries::take()[2];
    let expected = original.evaluate(value, Detail::Outcome, &HEALTHY);
    assert_eq!(
        serde_json::to_value(selected).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let mut pairs = Vec::new();
    for index in 0..9 {
        let (old, new) = if index % 2 == 0 {
            (time(&original, value), time(owner, value))
        } else {
            let new = time(owner, value);
            (time(&original, value), new)
        };
        pairs.push((old, new));
    }
    println!(
        "hostile {name} value_bytes={} compiled_entries={route} pairs_ns={pairs:?}",
        value.bytes().len()
    );
}

#[test]
fn hostile_costs() {
    let strings: Vec<_> = (0..1024).map(|i| format!("choice-{i:08}")).collect();
    let owner = prepare(&serde_json::json!({"enum": strings}).to_string());
    let value = JsonValue::parse(format!("\"{}\"", "x".repeat(16 * 1024 * 1024 - 2))).unwrap();
    hostile("enum-1024-root", &owner, &value);
    let branches: Vec<_> = (0..256)
        .map(|i| serde_json::json!({"const":format!("choice-{i:08}")}))
        .collect();
    let owner = prepare(&serde_json::json!({"oneOf":branches}).to_string());
    hostile("oneof-256-root", &owner, &value);

    // All choices share their prefix with the matching final choice.
    let strings: Vec<_> = (0..64)
        .map(|i| format!("xxxxxxxxxxxxxxxxxxxxxxxx{i:08}"))
        .collect();
    let owner = prepare(&serde_json::json!({"items":{"enum":strings}}).to_string());
    let item = r#""xxxxxxxxxxxxxxxxxxxxxxxx00000063""#;
    let mut text = format!(
        "[{}{}]",
        format!("{item},").repeat((16 * 1024 * 1024 - item.len() - 2) / (item.len() + 1)),
        item
    );
    let padding = " ".repeat(16 * 1024 * 1024 - text.len());
    text.insert_str(text.len() - 1, &padding);
    hostile(
        "enum-64-dense-last-match",
        &owner,
        &JsonValue::parse(text).unwrap(),
    );

    let owner = prepare(
        r##"{"type":["object","null"],"properties":{"n":{"type":"integer","minimum":0},"next":{"$ref":"#"}}}"##,
    );
    let depth = owner.compiled.qualified_depth().unwrap();
    assert!(depth < 1000);
    for beyond in [0, 1] {
        let levels = depth + beyond;
        let shell = r#"{"n":0,"next":""}"#;
        let prefix = r#"{"n":0,"next":"#;
        let text = format!(
            "{}{{\"n\":0,\"padding\":\"{}\"}}{}",
            prefix.repeat(levels - 1),
            "x".repeat(16 * 1024 * 1024 - (levels - 1) * (prefix.len() + 1) - shell.len() - 3),
            "}".repeat(levels - 1)
        );
        let value = JsonValue::parse(text).unwrap();
        hostile(&format!("deep-{levels}-beyond-{beyond}"), &owner, &value);
    }
}

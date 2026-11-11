//! Isolated global allocator and private compiled-lane measurements.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    calls: usize,
    bytes: usize,
    live: usize,
    peak: usize,
}
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts { calls: 0, bytes: 0, live: 0, peak: 0 }) };
}
struct Counter;
fn record(size: usize) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        COUNTS.with(|counts| {
            let previous = counts.get();
            counts.set(Counts {
                calls: previous.calls + 1,
                bytes: previous.bytes + size,
                live: previous.live + size,
                peak: previous.peak.max(previous.live + size),
            });
        });
    }
}
fn release(size: usize) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        COUNTS.with(|counts| {
            let mut next = counts.get();
            next.live = next
                .live
                .checked_sub(size)
                .expect("untracked allocation freed");
            counts.set(next);
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
        release(layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            record(size);
            release(layout.size());
        }
        pointer
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;
static HEALTHY: AtomicBool = AtomicBool::new(false);

fn prepare(schema: &str) -> DefaultValidator {
    prepare_with_limits(schema, Limits::default())
}

fn prepare_with_limits(schema: &str, limits: Limits) -> DefaultValidator {
    let resources = ExternalSchemas::new([openbindings::ExternalSchema { uri: ("https://allocation.invalid/root").into(), document: JsonValue::parse(schema).unwrap() }])
        .unwrap();
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://allocation.invalid/root"}}}}"#)
        .unwrap().validate().unwrap();
    let selected = document.conformant().unwrap().resolve_operation("op").unwrap().input().unwrap();
    DefaultEvaluator::with_limits(limits).prepare(
        EvaluationRequest {
            root: openbindings::SchemaRoot::Contained(&selected),
            external_schemas: &resources,
        },
        &PreparationLimits::default(), &AtomicBool::new(false),
    ).unwrap()
}

fn sample(lane: &compiled::Lane, value: &JsonValue) -> Counts {
    compiled::entries::take();
    COUNTS.set(Counts::default());
    ACTIVE.set(true);
    let result = lane.outcome(value, &HEALTHY);
    let valid = matches!(result, Some(ValueOutcome::Satisfies));
    drop(result);
    ACTIVE.set(false);
    let counts = COUNTS.get();
    assert!(valid);
    assert_eq!(counts.live, 0, "scratch retained after the pass");
    counts
}

fn lane(owner: &DefaultValidator) -> compiled::Lane {
    if std::env::var_os("OB_DECIMAL_SCRATCH_BYPASS").is_some() {
        owner.compiled.without_scratch_estimate()
    } else {
        owner.compiled.clone()
    }
}

fn repeated(token: &str, size: usize) -> JsonValue {
    let slot = format!(
        "{token},{:width$}",
        "",
        width = 32usize.saturating_sub(token.len() + 1)
    );
    let count = (size - 2 - token.len()) / slot.len();
    let mut text = format!("[{}{token}]", slot.repeat(count));
    text.insert_str(text.len() - 1, &" ".repeat(size - text.len()));
    assert_eq!(text.len(), size);
    JsonValue::parse(text).unwrap()
}

fn check(name: &str, schema: &str, token: &str) {
    let schema_value = JsonValue::parse(schema).unwrap();
    let bound_bytes = openbindings_internal_json::backend::number_nodes(&schema_value)
        .flatten()
        .map(str::len)
        .max()
        .unwrap_or(0);
    let peak_bound = 4 * (token.len() + bound_bytes) + 512;
    let owner = prepare(schema);
    let lane = lane(&owner);
    for size in [1024, 1024 * 1024, 16 * 1024 * 1024] {
        let value = repeated(token, size);
        let cold = sample(&lane, &value);
        let warm = sample(&lane, &value);
        println!(
            "allocation {name} value_bytes={size} token_bytes={} cold={cold:?} warm={warm:?}",
            token.len()
        );
        assert!(cold.peak <= peak_bound);
        assert!(warm.peak <= peak_bound);
    }
}

#[test]
fn allocation_samples_preserve_verdicts_and_peak_bounds() {
    for (keyword, bound) in [
        ("minimum", "0"),
        ("exclusiveMinimum", "0.125"),
        ("maximum", "1000000"),
        ("exclusiveMaximum", "1000000.125"),
    ] {
        let schema = format!(r#"{{"items":{{"type":"number","{keyword}":{bound}}}}}"#);
        check(&format!("money-{keyword}"), &schema, "19.99");
        check(
            &format!("fraction-{keyword}"),
            &schema,
            "19.12345678901234567890123456789",
        );
    }
    for (name, bound, token) in [
        (
            "wide-bound",
            "1000000.12345678901234567890123456789",
            "19.12345678901234567890123456789",
        ),
        (
            "wide-exponents",
            "1e99999999999999999999999999999999999999999999999999",
            "1.2345678901234567890123456789e9223372036854775808",
        ),
        (
            "negative-exponents",
            "1e-9223372036854775808",
            "1.2345678901234567890123456789e-9223372036854775809",
        ),
        (
            "wide-bound-inline-value",
            "1e99999999999999999999999999999999999999999999999999",
            "19.99",
        ),
    ] {
        check(
            name,
            &format!(r#"{{"items":{{"maximum":{bound}}}}}"#),
            token,
        );
    }
    check(
        "integer-typing",
        r#"{"items":{"type":"integer","minimum":0}}"#,
        "12345678901234567890123456789.0",
    );
    check(
        "all-four-bounds",
        r#"{"items":{"minimum":0.1,"exclusiveMinimum":0,"maximum":1000000,"exclusiveMaximum":1000000.1}}"#,
        "19.12345678901234567890123456789",
    );
}

#[test]
fn token_length_costs() {
    for size in [32, 1024, 64 * 1024, 1024 * 1024, 16 * 1024 * 1024 - 2] {
        for (name, token) in [
            ("fraction", format!("0.{}", "1".repeat(size - 2))),
            ("tiny-inline", format!("0.{}1", "0".repeat(size - 3))),
            ("positive-exponent", format!("1e{}", "9".repeat(size - 2))),
            ("negative-exponent", format!("1e-{}", "9".repeat(size - 3))),
            (
                "fraction-exponent",
                format!(
                    "0.{}e-{}",
                    "1".repeat((size - 4) / 2),
                    "9".repeat(size - 4 - (size - 4) / 2)
                ),
            ),
        ] {
            // Equal wide bounds exercise both offset operands and carry paths.
            let schema = if size <= 64 * 1024 {
                format!(r#"{{"items":{{"minimum":{token},"maximum":{token}}}}}"#)
            } else if name == "positive-exponent" {
                r#"{"items":{"minimum":1}}"#.to_owned()
            } else {
                r#"{"items":{"minimum":0,"maximum":1}}"#.to_owned()
            };
            let owner = prepare(&schema);
            let bound_bytes = if size <= 64 * 1024 { token.len() } else { 1 };
            let peak_bound = 4 * (token.len() + bound_bytes) + 512;
            let value = JsonValue::parse(format!("[{token}]")).unwrap();
            // Above the peak boundary, measure only in the isolated copy and
            // separately assert that production declines before backend entry.
            let lane = if peak_bound > Limits::default().evaluation_sdk_reservation_bytes {
                compiled::entries::take();
                assert!(owner.compiled.outcome(&value, &HEALTHY).is_none());
                assert_eq!(compiled::entries::take()[2], 0);
                owner.compiled.without_scratch_estimate()
            } else {
                lane(&owner)
            };
            let counts = sample(&lane, &value);
            let timings: Vec<_> = (0..9)
                .map(|_| {
                    let started = std::time::Instant::now();
                    let outcome = std::hint::black_box(lane.outcome(&value, &HEALTHY));
                    let elapsed = started.elapsed().as_nanos();
                    assert!(matches!(outcome, Some(ValueOutcome::Satisfies)));
                    elapsed
                })
                .collect();
            println!(
                "length {name} token_bytes={} bound_bytes={bound_bytes} peak_bound={peak_bound} counts={counts:?} times_ns={timings:?}",
                token.len()
            );
            assert!(counts.peak <= peak_bound);
        }
    }
}

#[test]
fn allocation_proof() {
    allocation_samples_preserve_verdicts_and_peak_bounds();
    token_length_costs();
}

fn time(owner: &DefaultValidator, value: &JsonValue, iterations: usize) -> u128 {
    let started = std::time::Instant::now();
    for _ in 0..iterations {
        let result = std::hint::black_box(owner.evaluate(value, Detail::Outcome, &HEALTHY));
        assert!(
            !matches!(result, ValueOutcome::NoVerdict { ref detail } if detail.reason != NoVerdictReason::LimitExceeded),
            "{result:?}"
        );
    }
    started.elapsed().as_nanos() / iterations as u128
}

fn hostile(
    name: &str,
    schema: &str,
    owner: &DefaultValidator,
    value: &JsonValue,
    iterations: usize,
) {
    let original = DefaultValidator {
        plan: owner.plan.clone(),
        compiled: compiled::Lane::Original("forced-old-path"),
    };
    compiled::entries::take();
    let selected = owner.evaluate(value, Detail::Outcome, &HEALTHY);
    let route = compiled::entries::take()[2];
    let expected = original.evaluate(value, Detail::Outcome, &HEALTHY);
    let diagnostic_owner = prepare_with_limits(
        schema,
        Limits {
            evaluation_sdk_reservation_bytes: 2 * 1024 * 1024 * 1024,
            ..Limits::default()
        },
    );
    assert!(matches!(
        diagnostic_owner.compiled,
        compiled::Lane::Original(_)
    ));
    let diagnostic = diagnostic_owner.evaluate(value, Detail::Outcome, &HEALTHY);
    println!(
        "outcomes {name} selected={} original={} diagnostic_original={}",
        serde_json::to_string(&selected).unwrap(),
        serde_json::to_string(&expected).unwrap(),
        serde_json::to_string(&diagnostic).unwrap()
    );
    assert!(matches!(diagnostic, ValueOutcome::Satisfies));
    if !matches!(selected, ValueOutcome::NoVerdict { .. }) {
        assert!(matches!(selected, ValueOutcome::Satisfies));
    }
    if !matches!(expected, ValueOutcome::NoVerdict { .. }) {
        assert!(matches!(selected, ValueOutcome::Satisfies));
    }
    let mut pairs = Vec::new();
    for index in 0..9 {
        let (old, new) = if index % 2 == 0 {
            (
                time(&original, value, iterations),
                time(owner, value, iterations),
            )
        } else {
            let new = time(owner, value, iterations);
            (time(&original, value, iterations), new)
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
    for (name, token, bound) in [
        (
            "mib-fractions",
            format!("0.{}", "1".repeat(1024 * 1024 - 2)),
            "1".to_owned(),
        ),
        (
            "mib-positive-exponents",
            format!("1e{}", "9".repeat(1024 * 1024 - 2)),
            "1".to_owned(),
        ),
        (
            "mib-negative-exponents",
            format!("1e-{}", "9".repeat(1024 * 1024 - 3)),
            "1".to_owned(),
        ),
        (
            "near-value-limit-exponent",
            format!("1e-{}", "9".repeat(16 * 1024 * 1024 - 5)),
            "1".to_owned(),
        ),
    ] {
        let schema = if name == "mib-positive-exponents" {
            r#"{"items":{"minimum":1}}"#.to_owned()
        } else {
            format!(r#"{{"items":{{"minimum":0,"maximum":{bound}}}}}"#)
        };
        let owner = prepare(&schema);
        let value = repeated(&token, 16 * 1024 * 1024);
        hostile(name, &schema, &owner, &value, 1);
    }
    let schema = r##"{"type":["object","null"],"properties":{"n":{"minimum":0,"maximum":1000000},"next":{"$ref":"#"}}}"##;
    let owner = prepare(schema);
    let depth = owner.compiled.qualified_depth().unwrap();
    for beyond in [0, 1] {
        let levels = depth + beyond;
        let prefix = r#"{"n":19.99,"next":"#;
        let shell = r#"{"n":19.99,"padding":""}"#;
        let text = format!(
            "{}{}{}",
            prefix.repeat(levels - 1),
            shell.replace(
                r#""padding":"""#,
                &format!(
                    r#""padding":"{}""#,
                    "x".repeat(16 * 1024 * 1024 - (levels - 1) * (prefix.len() + 1) - shell.len())
                )
            ),
            "}".repeat(levels - 1)
        );
        hostile(
            &format!("deep-{levels}-beyond-{beyond}"),
            schema,
            &owner,
            &JsonValue::parse(text).unwrap(),
            1,
        );
    }
    let schema = r#"{"items":{"minimum":0,"maximum":1000000}}"#;
    let owner = prepare(schema);
    hostile(
        "below-estimate",
        schema,
        &owner,
        &repeated("19.99", 10_000),
        1000,
    );
    let schema = r#"{"type":"array","items":{"type":"object","required":["sku","price","tax","discount","status"],"properties":{"sku":{"type":"string"},"price":{"type":"number","minimum":0,"maximum":1000000},"tax":{"type":"number","minimum":0,"maximum":1000000},"discount":{"type":"number","minimum":0,"maximum":1000000},"status":{"enum":["open","paid","void"]}}}}"#;
    let owner = prepare(schema);
    for size in [25_000, 100_000, 16 * 1024 * 1024] {
        let sku = if size > 100_000 {
            "x".repeat(256)
        } else {
            "sku-0".into()
        };
        let item =
            format!(r#"{{"sku":"{sku}","price":19.99,"tax":1.65,"discount":0.5,"status":"paid"}}"#);
        hostile(
            &format!("money-{size}"),
            schema,
            &owner,
            &repeated(&item, size),
            1,
        );
    }
}

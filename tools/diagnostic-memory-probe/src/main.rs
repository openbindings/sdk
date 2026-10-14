//! Single-thread first-error allocation regression, excluding parsing/compilation.
use openbindings_internal_json::{
    JsonValue,
    backend::{self, FlatJson},
};
use openbindings_internal_schema::fixed_schema;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn add(n: usize) {
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}
// SAFETY: every allocation contract is forwarded unchanged to System. Counters
// do not allocate, dereference memory, or affect ownership/alignment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            add(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            add(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, size) };
        if !next.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
            add(size);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn peak(call: impl FnOnce()) -> usize {
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    call();
    PEAK.load(Relaxed) - before
}
fn main() {
    // Compile both pinned validators before measurement.
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), false, 1).unwrap();
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), true, 1).unwrap();
    let mut regressions = Vec::new();
    for count in [1_000, 100_000] {
        let payload = format!("[{}]", vec!["0"; count].join(","));
        let cases = [
            ("meta-type-union", true, format!(r#"{{"type":{payload}}}"#)),
            (
                "document-schema-type",
                false,
                format!(
                    r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{payload}}}}}}}"#
                ),
            ),
            (
                "document-opaque-payload",
                false,
                format!(
                    r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"const":{payload}}},"description":false}}}}}}"#
                ),
            ),
            (
                "meta-localized-child",
                true,
                format!(r#"{{"allOf":[{{"const":{payload}}},null]}}"#),
            ),
            (
                "meta-opaque-payload",
                true,
                format!(r#"{{"const":{payload},"minLength":-1}}"#),
            ),
        ];
        for (name, meta, text) in cases {
            let value = JsonValue::parse(&text).unwrap();
            let bytes = peak(|| {
                let result = fixed_schema::check(&value, meta, 4096).unwrap();
                assert!(result.violated && !result.entries.is_empty());
                drop(result);
            });
            println!(
                "{name} count={count} source={} first_error_peak={bytes}",
                text.len()
            );
            if bytes >= 256 * 1024 {
                regressions.push((name, count, bytes));
            }
        }
        // A deliberately amplifying unpinned schema verifies the detector. Keep
        // is_valid and validate separate: only first-error construction amplifies.
        let validator = jsonschema::options_for::<FlatJson>().build(&serde_json::json!({"anyOf":[{"items":{"type":"string"}},{"items":{"type":"boolean"}}]})).unwrap();
        let value = JsonValue::parse(&payload).unwrap();
        let validity = peak(|| assert!(!validator.is_valid(backend::view(&value))));
        let diagnostic = peak(|| {
            assert!(validator.validate(backend::view(&value)).is_err());
        });
        println!(
            "amplifying-control count={count} validity_peak={validity} first_error_peak={diagnostic}"
        );
        assert!(diagnostic > 256 * 1024 && diagnostic > payload.len() * 16);
    }
    assert!(
        regressions.is_empty(),
        "pinned-schema first-error amplification: {regressions:?}"
    );
}

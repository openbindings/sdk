//! Single-thread fixed-check and full-document allocation qualification.
use openbindings_internal_json::{JsonValue, backend};
use openbindings_internal_schema::fixed_schema;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

mod constructors;
mod limits;
mod near_misses;
mod patterns;
mod references;
mod values;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
fn add(n: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
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
    // A reallocation that moves holds the old block and the new one at once.
    // Every one is counted so, moved or not: peaks are conservative.
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, size) };
        if !next.is_null() {
            add(size);
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn measured<T>(call: impl FnOnce() -> T) -> (T, usize, u128) {
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    let start = std::time::Instant::now();
    let result = call();
    let nanos = start.elapsed().as_nanos();
    (result, PEAK.load(Relaxed).saturating_sub(before), nanos)
}
fn main() {
    if std::env::args().any(|arg| arg == "--references-only") {
        println!("{}", serde_json::json!(references::run()));
        return;
    }
    if std::env::args().any(|arg| arg == "--limits-only") {
        limits::run();
        return;
    }
    constructors::run();
    if std::env::args().any(|arg| arg == "--patterns-only") {
        patterns::run();
        return;
    }
    if std::env::args().any(|arg| arg == "--near-misses-only") {
        near_misses::run();
        return;
    }
    if std::env::args().any(|arg| arg == "--values-only") {
        // `--path NAME` runs one value path alone.
        let args: Vec<String> = std::env::args().collect();
        let only = args
            .iter()
            .position(|arg| arg == "--path")
            .map(|at| args[at + 1].as_str());
        values::run(only);
        drop(references::run());
        return;
    }
    use openbindings_internal_schema::RawDocument;
    use serde_json::{Value, json};
    for repetition in 0..3 {
        let (map, bytes, nanos) = measured(|| fixed_schema::compile_locations().unwrap());
        println!(
            "{}",
            json!({"phase":"location-map-compilation", "repetition":repetition,"peak_bytes":bytes,"nanos":nanos})
        );
        drop(map);
    }
    // Compile the fixed checks and, through a failing document and a failing
    // schema, the guidance each builds at its first failure.
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), false, 1).unwrap();
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), true, 1).unwrap();
    fixed_schema::check(&JsonValue::parse(r#"{"type":0}"#).unwrap(), true, 1).unwrap();
    let location_map = fixed_schema::compile_locations().unwrap();
    let manifest = std::env::args().nth(1).expect("input manifest");
    let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    for case in cases {
        let text = std::fs::read(case["path"].as_str().unwrap()).unwrap();
        let value = match JsonValue::parse(&text) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{}",
                    json!({"id":case["id"],"phase":"parse-refusal","code":error.code})
                );
                continue;
            }
        };
        let raw = RawDocument::new(value.clone());
        let lane = case["lane"].as_str().unwrap();
        let index = raw.schema_index();
        let schemas: Vec<JsonValue> = if lane == "document" {
            Vec::new()
        } else {
            index.nodes.iter().map(|node| index.owned(node)).collect()
        };
        let mut checks = Vec::new();
        if lane != "meta" {
            checks.push((&value, false));
        }
        checks.extend(schemas.iter().map(|schema| (schema, true)));
        for repetition in 0..3 {
            // Validation is cached on an owner; use a fresh parsed owner for every sample.
            let parsed = openbindings::Document::parse(&text).unwrap();
            let (decisions, validity_peak, validity_ns) = measured(|| {
                checks
                    .iter()
                    .map(|(v, m)| fixed_schema::validity(v, *m))
                    .collect::<Vec<_>>()
            });
            // Isolate the production map walk from authoritative validity and
            // meta checks. Only established document failures enter this region.
            let locate_document = lane != "meta" && decisions.first() == Some(&Ok(false));
            let (_, location_peak, location_ns) = measured(|| {
                if locate_document {
                    backend::fixed_strings(|| {
                        let mut positions = Vec::new();
                        let mut bytes = 8 * 1024 * 1024;
                        location_map.locate(value.view(), |at| {
                            if positions.len() >= 4096 {
                                return false;
                            }
                            let Some(size) = backend::location_pointer_size(at, bytes) else {
                                return false;
                            };
                            bytes -= size;
                            positions.push(at);
                            true
                        });
                        std::hint::black_box(backend::locations(&positions));
                    });
                }
            });
            let ((actual, locations, truncated), check_peak, check_ns) = measured(|| {
                let mut decisions = Vec::new();
                let mut positions = Vec::new();
                let mut truncated = false;
                let mut pointer_bytes = 8 * 1024 * 1024;
                for &(value, meta) in &checks {
                    match fixed_schema::check(value, meta, 4096 - positions.len()) {
                        Err(e) => decisions.push(Err(e)),
                        Ok(problems) => {
                            decisions.push(Ok(!problems.violated));
                            truncated |= problems.truncated;
                            for problem in problems.entries {
                                if let Some(size) =
                                    backend::location_pointer_size(problem.at, pointer_bytes)
                                {
                                    pointer_bytes -= size;
                                    positions.push(problem.at);
                                } else {
                                    truncated = true;
                                }
                            }
                        }
                    }
                }
                (decisions, backend::locations(&positions), truncated)
            });
            assert_eq!(
                actual, decisions,
                "validity/refusal changed: {}",
                case["id"]
            );
            // On these frozen generators, validity itself peaks below 18 MiB.
            // Guidance may retain 8 MiB of pointers and up to 4096 findings; the
            // extra 2 MiB covers their tables, traversal frames and name scratch.
            assert!(
                validity_peak <= 18 * 1024 * 1024,
                "validity baseline: {}: {validity_peak}",
                case["id"]
            );
            assert!(
                check_peak <= validity_peak + 10 * 1024 * 1024,
                "fixed finding allocation: {}: {check_peak} vs {validity_peak}",
                case["id"]
            );
            let (validation, sdk_peak, sdk_ns) = measured(|| parsed.validate().unwrap());
            // The complete SDK adds native rule tables, proof and report storage.
            assert!(
                sdk_peak <= validity_peak + 12 * 1024 * 1024,
                "document allocation: {}: {sdk_peak} vs {validity_peak}",
                case["id"]
            );
            let finding_count = locations.len();
            // A JSON tree holds a pointer through an unpaired name as its units.
            let locations: Vec<_> = locations.iter().map(|at| {
                if case["group"] == "corpus" {
                    let pointer = at.pointer.as_ref().map(|p| match p.as_str() {
                        Some(text) => json!(text),
                        None => json!({"utf16": p.utf16_units().collect::<Vec<_>>()}),
                    });
                    json!({"pointer":pointer,"byte_offset":at.byte_offset,"line":at.line,"byte_column":at.byte_column})
                }
                else { json!({"byte_offset":at.byte_offset,"line":at.line,"byte_column":at.byte_column,
                    "pointer_bytes":at.pointer.as_ref().map_or(0, backend::pointer_bytes)}) }
            }).collect();
            println!(
                "{}",
                json!({"id":case["id"],"group":case.get("group"),"phase":"document", "repetition":repetition,
                "source_bytes":text.len(), "validity_peak_bytes":validity_peak,"validity_nanos":validity_ns,
                "location_walk":locate_document,"location_peak_bytes":location_peak,"location_nanos":location_ns,
                "fixed_peak_bytes":check_peak,"fixed_nanos":check_ns,"sdk_peak_bytes":sdk_peak,"sdk_nanos":sdk_ns,
                "decisions":decisions,"finding_count":finding_count,"locations":locations,"truncated":truncated,"conclusion":validation.report().conclusion})
            );
        }
    }
    values::run(None);
    // Preserve the frozen probes' cache initialization and measurement order.
    drop(references::run());
    near_misses::run();
    patterns::run();
    limits::run();
}

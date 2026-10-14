//! Single-thread fixed-check and full-document allocation qualification.
use openbindings_internal_json::{JsonValue, backend};
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
fn measured<T>(call: impl FnOnce() -> T) -> (T, usize, u128) {
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    let start = std::time::Instant::now();
    let result = call();
    let nanos = start.elapsed().as_nanos();
    (result, PEAK.load(Relaxed).saturating_sub(before), nanos)
}
fn main() {
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
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), false, 1).unwrap();
    fixed_schema::check(&JsonValue::parse("{}").unwrap(), true, 1).unwrap();
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
        let mut checks = Vec::new();
        if lane != "meta" {
            checks.push((&value, false));
        }
        if lane != "document" {
            checks.extend(
                raw.schema_index()
                    .nodes
                    .iter()
                    .map(|node| (&node.value, true)),
            );
        }
        for repetition in 0..3 {
            // Validation is cached on an owner; use a fresh parsed owner for every sample.
            let parsed = openbindings::ParsedDocument::parse(&text).unwrap();
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
                        jsonschema::ob_work::bounded(2_000_000, 1024, || {
                            jsonschema::ob_ecma::top_level(2_000_000, || {
                                let mut positions = Vec::new();
                                let mut bytes = 8 * 1024 * 1024;
                                location_map.locate(value.view(), |at| {
                                    if positions.len() >= 4096 {
                                        return false;
                                    }
                                    let Some(size) = backend::location_pointer_size(at, bytes)
                                    else {
                                        return false;
                                    };
                                    bytes -= size;
                                    positions.push(at);
                                    true
                                });
                                std::hint::black_box(backend::locations(&positions));
                            })
                        })
                    })
                    .ok();
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
            let locations: Vec<_> = locations.iter().map(|at| {
                if case["group"] == "corpus" { serde_json::to_value(at).unwrap() }
                else { json!({"byte_offset":at.byte_offset,"line":at.line,"byte_column":at.byte_column,
                    "pointer_bytes":at.pointer.as_ref().map_or(0, String::len)}) }
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
}

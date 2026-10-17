//! One JSON request per fresh child; one JSON-line observation on stdout.
mod sdk;
use sdk::ROOT;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    hint::black_box,
    io::{self, Read},
    time::Instant,
};
#[cfg(feature = "allocations")]
mod allocations;
#[cfg(feature = "allocations")]
#[global_allocator]
static ALLOCATOR: allocations::Counter = allocations::Counter;
#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    document: String,
    #[serde(default)]
    schema: String,
    #[serde(default)]
    value: String,
    expected: Value,
    #[serde(default)]
    resources: BTreeMap<String, String>,
    #[serde(default)]
    upstream: bool,
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    limits: sdk::Limits,
    #[serde(default = "sdk")]
    engine: String,
    #[serde(default = "eval")]
    mode: String,
    #[serde(default = "one")]
    iterations: usize,
}
fn sdk() -> String {
    "sdk".into()
}
fn eval() -> String {
    "eval".into()
}
fn one() -> usize {
    1
}
// Explain selects full available diagnostics during preparation. `bench` decides
// this once per request, so no timed region compares mode names.
fn context(r: &Request, upstream: bool) -> sdk::Context {
    sdk::context(&r.schema, &r.resources, &r.limits, upstream)
}
fn prepare(r: &Request, upstream: bool) -> sdk::Prepared {
    context(r, upstream).prepare_input()
}
fn outcome(p: &sdk::Prepared, value: &sdk::ParsedValue, cancelled: bool) -> sdk::Outcome {
    p.validate(value, cancelled)
}
fn assert_expected(actual: &Value, expected: &Value) {
    for (key, value) in expected.as_object().expect("expected object") {
        assert_eq!(&actual[key], value, "unexpected {key}: {actual}");
    }
}
fn peer(r: &Request) -> jsonschema::Validator {
    struct Explicit(BTreeMap<String, Value>);
    impl jsonschema::Retrieve for Explicit {
        fn retrieve(
            &self,
            uri: &jsonschema::Uri<String>,
        ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
            self.0
                .get(uri.as_str())
                .cloned()
                .ok_or_else(|| format!("absent explicit resource: {uri}").into())
        }
    }
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .with_base_uri(ROOT)
        .with_retriever(Explicit(
            r.resources
                .iter()
                .map(|(uri, text)| (uri.clone(), parse_peer(text)))
                .collect(),
        ))
        .build(&parse_peer(&r.schema))
        .unwrap()
}
fn parse_peer(text: &str) -> Value {
    let mut parser = serde_json::Deserializer::from_str(text);
    parser.disable_recursion_limit();
    let value = Value::deserialize(&mut parser).unwrap();
    parser.end().unwrap();
    value
}
fn bench(r: &Request) -> Value {
    assert!(r.iterations > 0);
    assert!(matches!(r.engine.as_str(), "sdk" | "jsonschema-rs"));
    let upstream = r.upstream || r.mode == "explain";
    // Cold first use is timed before any preparation. Its result is checked
    // immediately after the timer; all other modes check outcomes before timing.
    if r.mode == "cold" {
        assert_eq!(r.iterations, 1);
        let (observed, nanos, memory) = if r.engine == "sdk" {
            let (p, nanos, memory) = first_use(|| prepare(r, upstream));
            let observed = outcome(&p, &sdk::parse_value(&r.value), r.cancelled).observation();
            (observed, nanos, memory)
        } else {
            let (rs, nanos, memory) = first_use(|| peer(r));
            let valid = rs.is_valid(&parse_peer(&r.value));
            let observed = json!({"status":if valid {"satisfies"} else {"fails"}});
            (observed, nanos, memory)
        };
        assert_expected(&observed, &r.expected);
        return json!({"nanoseconds":nanos,"iterations":1,"observation":observed,"outcomes":{"prepared":1},"memory":memory});
    }
    // Every other mode selects its operation once, here, before any timing.
    // Each closure is a distinct type, so each mode gets its own instance of
    // `measure`, and no timed iteration dispatches on the engine or mode.
    if matches!(r.mode.as_str(), "document" | "read") {
        assert_eq!(r.engine, "sdk");
        let document = sdk::validate_document_bytes(r.document.as_bytes());
        let mut observed = document.observation();
        if r.mode == "read" {
            observed["reads"] = serde_json::to_value(document.read()).unwrap();
        }
        assert_expected(&observed, &r.expected);
        return if r.mode == "document" {
            measure(r, observed, || {
                black_box(sdk::validate_document_bytes(black_box(
                    r.document.as_bytes(),
                )))
                .status()
            })
        } else {
            measure(r, observed, || {
                black_box(document.read());
                "read"
            })
        };
    }
    let context = (r.engine == "sdk").then(|| context(r, upstream));
    let prepared = context.as_ref().map(|c| c.prepare_input());
    let value = (r.engine == "sdk").then(|| sdk::parse_value(&r.value));
    let rs = (r.engine == "jsonschema-rs").then(|| peer(r));
    let pv = (r.engine == "jsonschema-rs").then(|| parse_peer(&r.value));
    let mut observed = if let Some(p) = &prepared {
        if r.mode == "explain" {
            p.explain(value.as_ref().unwrap(), r.cancelled)
                .observation()
        } else {
            outcome(p, value.as_ref().unwrap(), r.cancelled).observation()
        }
    } else {
        json!({"status":if rs.as_ref().unwrap().is_valid(pv.as_ref().unwrap()) {"satisfies"} else {"fails"}})
    };
    if r.mode == "explain" {
        let problems = observed["problems"]
            .as_array()
            .expect("explain requires failure");
        let count = problems.len();
        let first = problems
            .first()
            .expect("explain requires a problem")
            .clone();
        observed["problem_count"] = json!(count);
        observed["first_problem"] = first;
    }
    assert_expected(&observed, &r.expected);
    if let (Some(context), Some(prepared), Some(value)) = (&context, &prepared, &value) {
        return match r.mode.as_str() {
            "baseline" => measure(r, observed, || {
                black_box(&r.value);
                "baseline"
            }),
            "parse" => measure(r, observed, || {
                drop(black_box(sdk::parse_value(black_box(&r.value))));
                "parsed"
            }),
            "compile" => measure(r, observed, || {
                assert!(black_box(prepare(black_box(r), upstream)).is_ready());
                "prepared"
            }),
            "context" => measure(r, observed, || {
                assert!(black_box(context.prepare_input()).is_ready());
                "prepared"
            }),
            "explain" => measure(r, observed, || {
                black_box(prepared.explain(black_box(value), r.cancelled)).status()
            }),
            "eval" => measure(r, observed, || {
                black_box(outcome(prepared, black_box(value), r.cancelled)).status()
            }),
            "full" => measure(r, observed, || {
                black_box(outcome(
                    &prepare(black_box(r), upstream),
                    &sdk::parse_value(black_box(&r.value)),
                    r.cancelled,
                ))
                .status()
            }),
            _ => panic!("unknown mode"),
        };
    }
    let (rs, pv) = (rs.as_ref().unwrap(), pv.as_ref().unwrap());
    match r.mode.as_str() {
        "baseline" => measure(r, observed, || {
            black_box(&r.value);
            "baseline"
        }),
        "parse" => measure(r, observed, || {
            drop(black_box(parse_peer(black_box(&r.value))));
            "parsed"
        }),
        "compile" => measure(r, observed, || {
            drop(black_box(peer(black_box(r))));
            "prepared"
        }),
        "eval" => measure(r, observed, || {
            if black_box(rs.is_valid(black_box(pv))) {
                "satisfies"
            } else {
                "fails"
            }
        }),
        "full" => measure(r, observed, || {
            if black_box(peer(black_box(r)).is_valid(&parse_peer(black_box(&r.value)))) {
                "satisfies"
            } else {
                "fails"
            }
        }),
        _ => panic!("unsupported peer mode"),
    }
}
/// Times one first-use preparation in this fresh process. The result leaves
/// only after the timer stops, so its destruction is never measured.
#[inline(never)]
fn first_use<T>(prepare: impl FnOnce() -> T) -> (T, u128, Value) {
    #[cfg(feature = "allocations")]
    let memory_start = allocations::start();
    let start = Instant::now();
    let prepared = prepare();
    let nanos = start.elapsed().as_nanos();
    #[cfg(feature = "allocations")]
    let memory = allocations::finish(memory_start, 1);
    #[cfg(not(feature = "allocations"))]
    let memory = Value::Null;
    (prepared, nanos, memory)
}
/// Warms up, times and counts one mode's operation. Each mode passes its own
/// closure, so each mode compiles to a separate instance of this function whose
/// timed loop contains only that mode's operation. Instances are never inlined
/// into the dispatch: changing one mode's operation cannot change the
/// instructions of another mode's loop, although it can still move them.
#[inline(never)]
fn measure(r: &Request, observed: Value, operation: impl Fn() -> &'static str) -> Value {
    let mut outcomes = BTreeMap::<&str, usize>::new();
    let expected_status = observed["status"].as_str().unwrap();
    for _ in 0..3 {
        black_box(operation());
    }
    // Preallocate outcome storage outside both measured regions.
    for key in [
        "baseline",
        "parsed",
        "prepared",
        "satisfies",
        "fails",
        "undefined",
        "no-verdict",
    ] {
        outcomes.insert(key, 0);
    }
    if matches!(r.mode.as_str(), "document" | "read") {
        for key in ["conformant", "non-conformant", "read"] {
            outcomes.insert(key, 0);
        }
    }
    #[cfg(feature = "allocations")]
    let memory_start = allocations::start();
    let start = Instant::now();
    for _ in 0..r.iterations {
        *outcomes.get_mut(operation()).unwrap() += 1;
    }
    let nanos = start.elapsed().as_nanos();
    #[cfg(feature = "allocations")]
    let memory = allocations::finish(memory_start, r.iterations);
    #[cfg(not(feature = "allocations"))]
    let memory = Value::Null;
    outcomes.retain(|_, count| *count > 0);
    if matches!(r.mode.as_str(), "eval" | "full" | "explain" | "document") {
        assert_eq!(outcomes.get(expected_status), Some(&r.iterations));
    }
    json!({"nanoseconds":nanos,"iterations":r.iterations,"observation":observed,"outcomes":outcomes,"memory":memory})
}
fn main() {
    // Source digests embedded by build.rs, so the build driver can confirm
    // which instrument and SDK trees this binary was compiled from.
    if std::env::args().nth(1).as_deref() == Some("--provenance") {
        println!(
            "{}",
            json!({"instrument":env!("EVALUATOR_BENCH_INSTRUMENT_DIGEST"),
                "sdk":env!("EVALUATOR_BENCH_SDK_DIGEST")})
        );
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--empty") {
        #[cfg(feature = "allocations")]
        let memory = allocations::finish(allocations::start(), 1);
        #[cfg(not(feature = "allocations"))]
        let memory = Value::Null;
        println!(
            "{}",
            json!({"iterations":1,"observation":{"status":"empty-process"},"outcomes":{"empty-process":1},"memory":memory})
        );
        return;
    }
    let mut text = String::new();
    io::stdin().read_to_string(&mut text).unwrap();
    let r: Request = serde_json::from_str(&text).unwrap();
    println!("{}", bench(&r));
}

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
fn context(r: &Request) -> sdk::Context {
    sdk::context(
        &r.schema,
        &r.resources,
        &r.limits,
        r.upstream || r.mode == "explain",
    )
}
fn prepare(r: &Request) -> sdk::Prepared {
    context(r).prepare_input()
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
    // Cold first use is timed before any preparation. Its result is checked
    // immediately after the timer; all other modes check outcomes before timing.
    if r.mode == "cold" {
        assert_eq!(r.iterations, 1);
        #[cfg(feature = "allocations")]
        let memory_start = allocations::start();
        let start = Instant::now();
        let (sdk, rs) = if r.engine == "sdk" {
            (Some(prepare(r)), None)
        } else {
            (None, Some(peer(r)))
        };
        let nanos = start.elapsed().as_nanos();
        #[cfg(feature = "allocations")]
        let memory = allocations::finish(memory_start, 1);
        #[cfg(not(feature = "allocations"))]
        let memory = Value::Null;
        let observed = if let Some(p) = sdk {
            outcome(&p, &sdk::parse_value(&r.value), r.cancelled).observation()
        } else {
            json!({"status":if rs.unwrap().is_valid(&parse_peer(&r.value)) {"satisfies"} else {"fails"}})
        };
        assert_expected(&observed, &r.expected);
        return json!({"nanoseconds":nanos,"iterations":1,"observation":observed,"outcomes":{"prepared":1},"memory":memory});
    }
    if matches!(r.mode.as_str(), "document" | "read") {
        assert_eq!(r.engine, "sdk");
        let document = sdk::validate_document_bytes(r.document.as_bytes());
        let mut observed = document.observation();
        if r.mode == "read" {
            observed["reads"] = serde_json::to_value(document.read()).unwrap();
        }
        assert_expected(&observed, &r.expected);
        let operation = || {
            if r.mode == "document" {
                black_box(sdk::validate_document_bytes(black_box(
                    r.document.as_bytes(),
                )))
                .status()
            } else {
                black_box(document.read());
                "read"
            }
        };
        return measure(r, observed, operation);
    }
    let context = (r.engine == "sdk").then(|| context(r));
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
    let operation = || -> &'static str {
        if r.engine == "sdk" {
            match r.mode.as_str() {
                "baseline" => {
                    black_box(&r.value);
                    "baseline"
                }
                "parse" => {
                    drop(black_box(sdk::parse_value(black_box(&r.value))));
                    "parsed"
                }
                "compile" => {
                    assert!(black_box(prepare(black_box(r))).is_ready());
                    "prepared"
                }
                "context" => {
                    assert!(black_box(context.as_ref().unwrap().prepare_input()).is_ready());
                    "prepared"
                }
                "explain" => black_box(
                    prepared
                        .as_ref()
                        .unwrap()
                        .explain(black_box(value.as_ref().unwrap()), r.cancelled),
                )
                .status(),
                "eval" => black_box(outcome(
                    prepared.as_ref().unwrap(),
                    black_box(value.as_ref().unwrap()),
                    r.cancelled,
                ))
                .status(),
                "full" => black_box(outcome(
                    &prepare(black_box(r)),
                    &sdk::parse_value(black_box(&r.value)),
                    r.cancelled,
                ))
                .status(),
                _ => panic!("unknown mode"),
            }
        } else {
            match r.mode.as_str() {
                "baseline" => {
                    black_box(&r.value);
                    "baseline"
                }
                "parse" => {
                    drop(black_box(parse_peer(black_box(&r.value))));
                    "parsed"
                }
                "compile" => {
                    drop(black_box(peer(black_box(r))));
                    "prepared"
                }
                "eval" => {
                    if black_box(
                        rs.as_ref()
                            .unwrap()
                            .is_valid(black_box(pv.as_ref().unwrap())),
                    ) {
                        "satisfies"
                    } else {
                        "fails"
                    }
                }
                "full" => {
                    if black_box(peer(black_box(r)).is_valid(&parse_peer(black_box(&r.value)))) {
                        "satisfies"
                    } else {
                        "fails"
                    }
                }
                _ => panic!("unsupported peer mode"),
            }
        }
    };
    measure(r, observed, operation)
}
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

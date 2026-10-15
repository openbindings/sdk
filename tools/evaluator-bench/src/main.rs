//! One JSON request per fresh child; one JSON-line observation on stdout.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Diagnostics, Limits};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    hint::black_box,
    io::{self, Read},
    sync::Arc,
    time::Instant,
};
#[cfg(feature = "allocations")]
mod allocations;
#[cfg(feature = "allocations")]
#[global_allocator]
static ALLOCATOR: allocations::Counter = allocations::Counter;
const ROOT: &str = "https://benchmark.invalid/root";
#[derive(Deserialize)]
struct Request {
    schema: String,
    value: String,
    expected: Value,
    #[serde(default)]
    resources: BTreeMap<String, String>,
    #[serde(default)]
    upstream: bool,
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    limits: Limits,
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
fn context(r: &Request) -> ValueContracts {
    let document = ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{ROOT}"}}}}}}}}"#
    ))
    .unwrap();
    let validation = document.validate().unwrap();
    let mut resources: Vec<_> = r
        .resources
        .iter()
        .map(|(uri, text)| SchemaResource {
            uri: uri.clone(),
            document: JsonValue::parse(text).unwrap(),
        })
        .collect();
    resources.push(SchemaResource {
        uri: ROOT.into(),
        document: JsonValue::parse(&r.schema).unwrap(),
    });
    validation.validated().unwrap().value_contracts(
        Arc::new(
            DefaultEvaluator::with_limits(r.limits.clone()).with_diagnostics(if r.upstream {
                Diagnostics::Upstream
            } else {
                Diagnostics::Coarse
            }),
        ),
        ResourceSet::new(resources).unwrap(),
    )
}
fn prepare(r: &Request) -> ValueContractPreparation {
    context(r).prepare("op", Direction::Input)
}
fn outcome(p: &ValueContractPreparation, value: &JsonValue, cancelled: bool) -> ValueOutcome {
    match p {
        ValueContractPreparation::Ready(contract) => {
            contract.validate_with_control(value, &std::sync::atomic::AtomicBool::new(cancelled))
        }
        other => panic!("fixture did not prepare: {other:?}"),
    }
}
fn status(result: &ValueOutcome) -> &'static str {
    match result {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}
fn observation(result: &ValueOutcome) -> Value {
    let mut value = json!({"status":status(result)});
    match result {
        ValueOutcome::NoVerdict { detail } => {
            value["code"] = json!(detail.code);
            value["detail"] = serde_json::to_value(detail).unwrap();
        }
        ValueOutcome::Undefined { detail } => {
            value["code"] = json!(detail.code);
            value["detail"] = serde_json::to_value(detail).unwrap();
        }
        ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } => {
            value["problems"] = serde_json::to_value(problems).unwrap();
            value["problems_complete"] = json!(problems_complete);
        }
        _ => {}
    }
    value
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
            observation(&outcome(
                &p,
                &JsonValue::parse(&r.value).unwrap(),
                r.cancelled,
            ))
        } else {
            json!({"status":if rs.unwrap().is_valid(&parse_peer(&r.value)) {"satisfies"} else {"fails"}})
        };
        assert_expected(&observed, &r.expected);
        return json!({"nanoseconds":nanos,"iterations":1,"observation":observed,"outcomes":{"prepared":1},"memory":memory});
    }
    let context = (r.engine == "sdk").then(|| context(r));
    let prepared = context.as_ref().map(|c| c.prepare("op", Direction::Input));
    let value = (r.engine == "sdk").then(|| JsonValue::parse(&r.value).unwrap());
    let rs = (r.engine == "jsonschema-rs").then(|| peer(r));
    let pv = (r.engine == "jsonschema-rs").then(|| parse_peer(&r.value));
    let observed = if let Some(p) = &prepared {
        observation(&outcome(p, value.as_ref().unwrap(), r.cancelled))
    } else {
        json!({"status":if rs.as_ref().unwrap().is_valid(pv.as_ref().unwrap()) {"satisfies"} else {"fails"}})
    };
    assert_expected(&observed, &r.expected);
    let mut outcomes = BTreeMap::<&str, usize>::new();
    let expected_status = observed["status"].as_str().unwrap();
    let operation = || -> &'static str {
        if r.engine == "sdk" {
            match r.mode.as_str() {
                "baseline" => {
                    black_box(&r.value);
                    "baseline"
                }
                "parse" => {
                    drop(black_box(JsonValue::parse(black_box(&r.value)).unwrap()));
                    "parsed"
                }
                "compile" => {
                    assert!(matches!(
                        black_box(prepare(black_box(r))),
                        ValueContractPreparation::Ready(_)
                    ));
                    "prepared"
                }
                "context" => {
                    assert!(matches!(
                        black_box(context.as_ref().unwrap().prepare("op", Direction::Input)),
                        ValueContractPreparation::Ready(_)
                    ));
                    "prepared"
                }
                "eval" => status(&black_box(outcome(
                    prepared.as_ref().unwrap(),
                    black_box(value.as_ref().unwrap()),
                    r.cancelled,
                ))),
                "full" => status(&black_box(outcome(
                    &prepare(black_box(r)),
                    &JsonValue::parse(black_box(&r.value)).unwrap(),
                    r.cancelled,
                ))),
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
    if matches!(r.mode.as_str(), "eval" | "full") {
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

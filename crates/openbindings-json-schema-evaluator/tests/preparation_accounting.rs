//! Preparation charges against a counting allocator. Its own test executable owns the
//! global allocator, so no other test binary or measurement copy of the crate meets it.
use openbindings::*;
use openbindings_allocation_test_counter::{Meter, measure};
use openbindings_internal_schema::preparation::audit;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;
#[global_allocator]
static METER: Meter = Meter;
fn document(n: usize, pattern: bool) -> ConformantDocument {
    let properties:serde_json::Map<_,_>=(0..n).map(|i|{
        let mut value=json!({"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","maxLength":200},"count":{"type":"integer","minimum":0},"ratio":{"type":"number","minimum":0,"maximum":1},"tags":{"type":"array","items":{"type":"string"},"maxItems":20},"state":{"enum":["open","closed","pending"]},"created":{"type":"string","format":"date-time"},"active":{"type":"boolean"}},"required":["id","name"],"additionalProperties":false});
        if pattern {value["properties"]["name"]["pattern"]=json!(format!("^(record-{i}|item-{i})[A-Za-z0-9]{{1,128}}(end|stop)$"));}
        (format!("record{i}"),value)
    }).collect();
    Document::parse(json!({"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"type":"object","properties":properties}}}).to_string()).unwrap().validate().unwrap().conformant().unwrap()
}
#[test]
fn ledger_matches_preparation_requested_heap() {
    let warm = document(1, false);
    drop(
        warm.schema("entry")
            .unwrap()
            .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default()),
    );
    let mut failed = false;
    for n in [1, 30, 300, 1000] {
        for (pattern, original) in [(false, false), (false, true), (true, false)] {
            let proof = document(n, pattern);
            let schema = proof.schema("entry").unwrap();
            let evaluator = DefaultEvaluator::new();
            let resources = ExternalSchemas::default();
            let limits = PreparationLimits {
                max_sdk_work_units: original.then_some(usize::MAX),
                ..Default::default()
            };
            for repeat in 0..2 {
                audit::LAST_PEAK.set(0);
                let (result, sample) = measure(|| {
                    schema.prepare_with_control(
                        &evaluator,
                        &resources,
                        &limits,
                        &std::sync::atomic::AtomicBool::new(false),
                    )
                });
                assert!(result.is_ok());
                let ledger = audit::LAST_PEAK.get();
                eprintln!(
                    "preparation n={n} pattern={pattern} original={original} repeat={repeat} ledger={ledger} heap={} ratio={:.6}",
                    sample.peak,
                    ledger as f64 / sample.peak as f64
                );
                failed |= ledger < sample.peak || ledger > sample.peak * 2;
                drop(result);
            }
        }
    }
    assert!(
        !failed,
        "preparation ledger must be between peak requested heap and twice that peak"
    );
}

#[test]
fn external_and_supplied_ledgers_match_requested_heap() {
    let warm = document(1, false);
    drop(
        warm.schema("entry")
            .unwrap()
            .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default()),
    );
    let mut failed = false;
    for n in [1, 30, 300, 1000] {
        let proof = document(n, false);
        let value = proof.schema("entry").unwrap().json().clone();
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: "https://perf.test/entry".into(),
            document: value,
        }])
        .unwrap();
        let referring=Document::parse(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://perf.test/entry"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
        let schema = referring.schema("entry").unwrap();
        let evaluator = DefaultEvaluator::new();
        let limits = PreparationLimits::default();
        for supplied in [true, false] {
            for repeat in 0..2 {
                audit::LAST_PEAK.set(0);
                let (result, sample) = measure(|| {
                    if supplied {
                        resources.prepare_with_control(
                            "https://perf.test/entry",
                            &evaluator,
                            &limits,
                            &std::sync::atomic::AtomicBool::new(false),
                        )
                    } else {
                        schema.prepare_with_control(
                            &evaluator,
                            &resources,
                            &limits,
                            &std::sync::atomic::AtomicBool::new(false),
                        )
                    }
                });
                assert!(result.is_ok());
                let ledger = audit::LAST_PEAK.get();
                eprintln!(
                    "preparation-external n={n} supplied={supplied} repeat={repeat} ledger={ledger} heap={} ratio={:.6}",
                    sample.peak,
                    ledger as f64 / sample.peak as f64
                );
                failed |= ledger < sample.peak || ledger > sample.peak * 2;
                drop(result);
            }
        }
    }
    assert!(
        !failed,
        "external preparation ledger must be between peak heap and twice that peak"
    );
}

#[test]
fn large_const_preparation_reports_held_storage() {
    let warm = document(1, false);
    drop(
        warm.schema("entry")
            .unwrap()
            .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default()),
    );
    let source = json!({"const":"x".repeat(4096*64)}).to_string();
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://memory.test/entry".into(),
        document: JsonValue::parse(&source).unwrap(),
    }])
    .unwrap();
    audit::LAST_PEAK.set(0);
    let (result, sample) = measure(|| {
        resources.prepare_with_control(
            "https://memory.test/entry",
            &DefaultEvaluator::new(),
            &PreparationLimits {
                max_sdk_reservation_bytes: 8 * 1024 * 1024,
                ..Default::default()
            },
            &std::sync::atomic::AtomicBool::new(false),
        )
    });
    eprintln!(
        "large const source={} admitted={} ledger={} heap={}",
        source.len(),
        result.is_ok(),
        audit::LAST_PEAK.get(),
        sample.peak
    );
    assert!(result.is_ok());
    let ledger = audit::LAST_PEAK.get();
    assert!(ledger >= sample.peak && ledger <= sample.peak * 2);
    assert!(
        resources
            .prepare_with_control(
                "https://memory.test/entry",
                &DefaultEvaluator::new(),
                &PreparationLimits {
                    max_sdk_reservation_bytes: 0,
                    ..Default::default()
                },
                &std::sync::atomic::AtomicBool::new(false),
            )
            .is_err()
    );
}

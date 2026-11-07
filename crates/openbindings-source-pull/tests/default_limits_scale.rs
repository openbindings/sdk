use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_source_pull::{PullLimits, PullRequest, PullResult, PullValidationError};
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn validate(document: Value, result: Value) -> Result<ConformantDocument, PullValidationError> {
    let document = Document::parse(document.to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let request = PullRequest::new(document, "s");
    PullResult::from_json(JsonValue::parse(result.to_string()).unwrap())
        .unwrap()
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
}
fn input() -> Value {
    json!({"type":"object","properties":{
        "a":{"$ref":"#/schemas/A"},"b":{"$ref":"#/schemas/B"},"c":{"$ref":"#/schemas/C"}
    }})
}
fn first(count: usize) -> (Value, Value) {
    let operations: Map<_, _> = (0..count)
        .map(|i| (format!("o{i}"), json!({"input":input()})))
        .collect();
    let bindings: Map<_, _> = (0..count)
        .map(|i| {
            (
                format!("b{i}"),
                json!({"operation":format!("o{i}"),"source":"s"}),
            )
        })
        .collect();
    let added: Vec<_> = (0..count)
        .map(|i| json!({"binding":format!("b{i}"),"operation":format!("o{i}")}))
        .collect();
    (
        json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}}}),
        json!({"additions":{"operations":operations,"bindings":bindings,"schemas":{"A":true,"B":true,"C":true}},"report":{"exhaustive":true,"added":added}}),
    )
}
#[test]
fn first_pull_of_5000_operations_with_three_shared_references() {
    let (document, result) = first(5000);
    validate(document, result).unwrap();
}
fn repull(count: usize) -> (Value, Value) {
    let operations: Map<_, _> = (0..count)
        .map(|i| (format!("o{i}"), json!({"input":input()})))
        .collect();
    let bindings: Map<_, _> = (0..count)
        .map(|i| {
            (
                format!("b{i}"),
                json!({"operation":format!("o{i}"),"source":"s"}),
            )
        })
        .collect();
    let existing: Vec<_> = (0..count).map(|i| json!({"binding":format!("b{i}"),"status":"stale","change":"non-breaking","proposal":{"input":{"$ref":"#/schemas/New"}}})).collect();
    (
        json!({"openbindings":"0.2.0","operations":operations,"bindings":bindings,"sources":{"s":{"kind":"k"}},"schemas":{"A":true,"B":true,"C":true}}),
        json!({"additions":{"schemas":{"New":{"type":"string"}}},"report":{"exhaustive":true,"added":[],"existing":existing}}),
    )
}
#[test]
fn repull_with_5000_proposals_preserves_original_operations() {
    let (document, result) = repull(5000);
    let original = document.clone();
    let adopted = validate(document, result).unwrap();
    assert_eq!(
        adopted.document().value().get("operations").unwrap().text(),
        original["operations"].to_string()
    );
}
#[test]
fn a_schema_cycle_reached_by_5000_entries_terminates() {
    let (document, mut result) = first(5000);
    result["additions"]["schemas"] =
        json!({"A":{"$ref":"#/schemas/B"},"B":{"$ref":"#/schemas/C"},"C":{"$ref":"#/schemas/A"}});
    validate(document, result).unwrap();
}

#[test]
fn validation_cancellation_allows_an_independent_retry() {
    struct Polls {
        calls: AtomicUsize,
        cancel_at: usize,
    }
    impl openbindings::WorkControl for Polls {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.cancel_at
        }
    }
    let (document, result) = repull(3);
    let document = Document::parse(document.to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let request = PullRequest::new(document, "s");
    let result = PullResult::from_json(JsonValue::parse(result.to_string()).unwrap()).unwrap();
    let completed = Polls {
        calls: AtomicUsize::new(0),
        cancel_at: usize::MAX,
    };
    result
        .validate_for(&request, &PullLimits::default(), &completed)
        .unwrap();
    for cancel_at in 0..completed.calls.load(Ordering::Relaxed) {
        let control = Polls {
            calls: AtomicUsize::new(0),
            cancel_at,
        };
        assert!(
            matches!(
                result.validate_for(&request, &PullLimits::default(), &control),
                Err(PullValidationError::Cancelled)
            ),
            "checkpoint {cancel_at}"
        );
    }
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

// The fixed core passes must not make the relationship ledger a second, smaller
// size limit: a result the 32 MiB JSON limits admit is judged, not refused.
#[test]
fn a_result_the_size_limits_admit_is_not_refused_for_its_bytes() {
    let filler = "x".repeat(30 * 1024 * 1024);
    let document = json!({
        "openbindings":"0.2.0",
        "operations":{"o":{}},
        "sources":{"s":{"kind":"k"}},
        "bindings":{"b":{"operation":"o","source":"s"}}
    });
    let result = json!({
        "additions":{"schemas":{"New":{"type":"string","description":filler}}},
        "report":{"exhaustive":true,"added":[],"existing":[{
            "binding":"b",
            "status":"stale",
            "change":"non-breaking",
            "proposal":{"input":{"$ref":"#/schemas/New"}}
        }]}
    });
    validate(document, result).unwrap();
}

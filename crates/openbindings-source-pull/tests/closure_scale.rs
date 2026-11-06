use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_source_pull::{
    PullLimits, PullRequest, PullResult, PullValidationError, RelationshipError,
};
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
fn closed(answer: Result<ConformantDocument, PullValidationError>, accepted: bool) {
    if accepted {
        assert!(answer.is_ok(), "{answer:?}");
    } else {
        assert!(
            matches!(
                answer,
                Err(PullValidationError::Relationship(
                    RelationshipError::UsesClosure
                ))
            ),
            "{answer:?}"
        );
    }
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
    let added: Vec<_> = (0..count).map(|i| json!({"binding":format!("b{i}"),"operation":format!("o{i}"),"uses":["schemas/A","schemas/B","schemas/C"]})).collect();
    (
        json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}}}),
        json!({"additions":{"operations":operations,"bindings":bindings,"schemas":{"A":true,"B":true,"C":true}},"report":{"exhaustive":true,"added":added}}),
    )
}
#[test]
fn first_pull_of_5000_operations_with_three_shared_references() {
    let (document, result) = first(5000);
    closed(validate(document, result), true);
}
#[test]
fn first_pull_of_5000_operations_rejects_one_missing_use() {
    let (document, mut result) = first(5000);
    result["report"]["added"][4999]["uses"]
        .as_array_mut()
        .unwrap()
        .pop();
    closed(validate(document, result), false);
}
fn repull(operations: usize, proposals: usize, collide: bool) -> (Value, Value) {
    let operations: Map<_, _> = (0..operations)
        .map(|i| (format!("o{i}"), json!({"input":input()})))
        .collect();
    let bindings: Map<_, _> = (0..proposals)
        .map(|i| {
            (
                format!("b{i}"),
                json!({"operation":format!("o{}", if collide {0} else {i}),"source":"s"}),
            )
        })
        .collect();
    let existing: Vec<_> = (0..proposals).map(|i| json!({"binding":format!("b{i}"),"status":"stale","change":"non-breaking","proposal":{"input":{"$ref":"#/schemas/New"},"uses":["schemas/New"]}})).collect();
    (
        json!({"openbindings":"0.2.0","operations":operations,"bindings":bindings,"sources":{"s":{"kind":"k"}},"schemas":{"A":true,"B":true,"C":true}}),
        json!({"additions":{"schemas":{"New":{"type":"string"}}},"report":{"exhaustive":true,"added":[],"existing":existing}}),
    )
}
#[test]
fn repull_of_5000_bindings_on_5000_operations() {
    let (document, result) = repull(5000, 5000, false);
    let original = document.clone();
    let adopted = validate(document, result).unwrap();
    assert_eq!(
        adopted.document().value().get("operations").unwrap().text(),
        original["operations"].to_string()
    );
}
#[test]
fn repull_of_5000_bindings_rejects_one_missing_use() {
    let (document, mut result) = repull(5000, 5000, false);
    result["report"]["existing"][4999]["proposal"]["uses"] = json!([]);
    closed(validate(document, result), false);
}
#[test]
fn colliding_bindings_are_each_judged_against_their_own_uses() {
    for missing in [None, Some(0), Some(1)] {
        let (document, mut result) = repull(1, 2, true);
        result["additions"]["schemas"]["Other"] = json!({"type":"number"});
        result["report"]["existing"][1]["proposal"] =
            json!({"input":{"$ref":"#/schemas/Other"},"uses":["schemas/Other"]});
        if let Some(index) = missing {
            result["report"]["existing"][index]["proposal"]["uses"] = json!([]);
        }
        closed(validate(document, result), missing.is_none());
    }
}
#[test]
fn overlapping_input_and_output_proposals_use_consistent_batches() {
    let (document, mut result) = repull(1, 3, true);
    let existing = result["report"]["existing"].as_array_mut().unwrap();
    existing[0]["proposal"] = json!({"output":{"$ref":"#/schemas/New"},"uses":["schemas/New"]});
    existing[1]["proposal"]["output"] = json!({"$ref":"#/schemas/New"});
    closed(validate(document, result), true);
}
#[test]
fn traversal_into_another_proposal_uses_existing_content_in_both_directions() {
    for old_depends in [false, true] {
        for declared in [false, true] {
            let document = json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}},"schemas":{
                "Start":true,"Other":if old_depends { json!({"$ref":"urn:example:old"}) } else { json!(true) }
            }});
            let result = json!({"additions":{"schemas":{"Old":{"$id":"urn:example:old"},"New":true}},"report":{"exhaustive":true,"added":[],"existing":[
                {"schema":"Start","status":"stale","change":"dangerous","proposal":{"schema":{"$ref":"#/schemas/Other"},"uses":if declared {json!(["schemas/Old"])} else if old_depends {json!(["schemas/New"])} else {json!([])}}},
                {"schema":"Other","status":"stale","change":"dangerous","proposal":{"schema":{"$ref":"#/schemas/New"},"uses":["schemas/New"]}}
            ]}});
            closed(validate(document, result), !old_depends || declared);
        }
    }
}
#[test]
fn traversal_back_to_own_slot_uses_proposed_content() {
    let document = json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}},"schemas":{
        "Start":{"$ref":"urn:example:old"},"Back":{"$ref":"#/schemas/Start"}
    }});
    let result = json!({"additions":{"schemas":{"Old":{"$id":"urn:example:old"}}},"report":{"exhaustive":true,"added":[],"existing":[
        {"schema":"Start","status":"stale","change":"dangerous","proposal":{"schema":{"$ref":"#/schemas/Back"}}}
    ]}});
    closed(validate(document, result), true);
}
#[test]
fn hostile_collisions_on_a_large_document_exhaust_the_ledger() {
    let (document, result) = repull(5000, 1000, true);
    let answer = validate(document, result);
    assert!(
        matches!(answer, Err(PullValidationError::LimitExceeded(_))),
        "{answer:?}"
    );
}
#[test]
fn a_schema_cycle_reached_by_5000_entries_terminates() {
    let (document, mut result) = first(5000);
    result["additions"]["schemas"] =
        json!({"A":{"$ref":"#/schemas/B"},"B":{"$ref":"#/schemas/C"},"C":{"$ref":"#/schemas/A"}});
    closed(validate(document, result), true);
}

#[test]
fn cancellation_during_collision_batches_allows_an_independent_retry() {
    struct Polls {
        calls: AtomicUsize,
        cancel_at: usize,
    }
    impl openbindings::WorkControl for Polls {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.cancel_at
        }
    }
    let (document, result) = repull(1, 3, true);
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
            "proposal":{"input":{"$ref":"#/schemas/New"},"uses":["schemas/New"]}
        }]}
    });
    closed(validate(document, result), true);
}

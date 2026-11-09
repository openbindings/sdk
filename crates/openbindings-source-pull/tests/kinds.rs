//! Callers can adopt a subset of additions after checking its references.
use openbindings::{ConformantDocument, Document, Edit, JsonValue};
use openbindings_source_pull::*;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}

fn object(fields: BTreeMap<String, JsonValue>) -> JsonValue {
    json(&format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{}:{}", quote(&k), v.text()))
            .collect::<Vec<_>>()
            .join(",")
    ))
}

fn document(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}

#[test]
fn a_caller_can_keep_part_of_a_pull_by_following_schema_references() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example.catalog@1"}}}"#,
    );
    let request = PullRequest::new(original.clone(), "s");
    let result = PullResult::from_json(json(r##"{"additions":{"operations":{"a":{"input":{"$ref":"#/schemas/S"}},"b":{"input":{"$ref":"#/schemas/T"}}},"schemas":{"S":{"type":"string"},"T":{"type":"integer"}},"bindings":{"a":{"source":"s","operation":"a"},"b":{"source":"s","operation":"b"}}},"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a"},{"binding":"b","operation":"b"}]}}"##)).unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    // Inspecting or dropping a retained result changes nothing in the original.
    drop(result.clone());
    assert!(original.value().get("schemas").is_none());
    assert_eq!(original.value().get("operations").unwrap().len(), Some(0));
    let additions = result.additions();
    let operation = additions
        .operations()
        .unwrap()
        .value()
        .get("a")
        .unwrap()
        .to_owned();
    // Follow a's input reference to the schema addition before inserting it.
    assert_eq!(
        operation
            .get("input")
            .unwrap()
            .get("$ref")
            .unwrap()
            .as_str(),
        Some("#/schemas/S")
    );
    let schema = additions
        .schemas()
        .unwrap()
        .value()
        .get("S")
        .unwrap()
        .to_owned();
    let binding = additions
        .bindings()
        .unwrap()
        .value()
        .get("a")
        .unwrap()
        .to_owned();
    let selected = original
        .document()
        .edit(&[
            Edit::set(&["operations", "a"], operation.clone()),
            Edit::set(&["schemas"], object(BTreeMap::from([("S".into(), schema)]))),
            Edit::set(
                &["bindings"],
                object(BTreeMap::from([("a".into(), binding)])),
            ),
        ])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    assert!(
        selected
            .value()
            .get("operations")
            .unwrap()
            .get("b")
            .is_none()
    );
    assert!(selected.value().get("schemas").unwrap().get("T").is_none());
    // Missing the schema reference target is caught by revalidation of this chosen
    // edit; validation of the all-additions snapshot did not authorize any subset.
    let incomplete = original
        .document()
        .edit(&[Edit::set(&["operations", "a"], operation)])
        .unwrap()
        .validate()
        .unwrap();
    assert!(incomplete.conformant().is_none());
}

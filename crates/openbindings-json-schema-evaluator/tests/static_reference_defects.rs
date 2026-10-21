use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;

fn observe(case: &Value) -> Value {
    let document = case.get("document").cloned().unwrap_or_else(|| {
        json!({
            "openbindings":"0.2.0", "operations":{"op":{"input":case["schema"]}}
        })
    });
    let parsed = Document::parse(document.to_string()).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.conformant().expect("conformant fixture");
    let supplied = ResourceSet::new(case["resources"].as_array().into_iter().flatten().map(|r| {
        SchemaResource {
            uri: r["uri"].as_str().unwrap().into(),
            document: JsonValue::parse(r["schema"].to_string()).unwrap(),
        }
    }))
    .unwrap();
    let limits: Limits =
        serde_json::from_value(case.get("limits").cloned().unwrap_or(json!({}))).unwrap();
    let operation = proof.resolve_operation("op").expect("declared operation");
    let contract = operation.input().expect("declared input");
    let (stage, result) =
        match contract.prepare_with(&DefaultEvaluator::with_limits(limits), &supplied) {
            Ok(contract) => (
                "ready",
                contract.validate(&JsonValue::parse(case["value"].to_string()).unwrap()),
            ),
            Err(detail) => ("preparation", ValueOutcome::NoVerdict { detail }),
        };
    match result {
        ValueOutcome::Satisfies => json!({"stage":stage,"outcome":"S"}),
        ValueOutcome::Fails { .. } => json!({"stage":stage,"outcome":"F"}),
        ValueOutcome::Undefined { detail } => {
            json!({"stage":stage,"outcome":"undefined", "detail":{
            "code":detail.code,"location":detail.location}})
        }
        ValueOutcome::NoVerdict { detail } => json!({"stage":stage,"outcome":"N", "detail":{
            "reason":detail.reason,"code":detail.code,"location":detail.location}}),
    }
}
fn assert_subset(expected: &Value, actual: &Value, id: &str) {
    if let Some(fields) = expected.as_object() {
        for (key, value) in fields {
            assert_subset(value, &actual[key], id);
        }
    } else {
        assert_eq!(expected, actual, "{id}");
    }
}
#[test]
fn matrix_and_concrete_reference_completions() {
    let mut receipt = Vec::new();
    for text in [
        include_str!("fixtures/reference-matrix.json"),
        include_str!("fixtures/reference-completions.json"),
    ] {
        let cases: Vec<Value> = serde_json::from_str(text).unwrap();
        for case in cases {
            let actual = observe(&case);
            let mut expected = case["expected"].clone();
            if expected["outcome"] == "N" && expected["detail"]["reason"] == "undefined" {
                expected["outcome"] = json!("undefined");
                expected["detail"].as_object_mut().unwrap().remove("reason");
            }
            assert_subset(&expected, &actual, case["id"].as_str().unwrap());
            receipt.push(json!({"id":case["id"],"actual":actual}));
        }
    }
    if let Ok(path) = std::env::var("REFERENCE_RECEIPT") {
        std::fs::write(path, serde_json::to_string(&receipt).unwrap()).unwrap();
    }
}
#[test]
fn preparation_and_validation_honor_cancellation() {
    let parsed = Document::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$id":"https://controls.test/root","$ref":"#missing"}},"ok":{"input":{"type":"integer"}}}}"##).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.conformant().unwrap();
    let evaluator = DefaultEvaluator::new();
    let cancelled = AtomicBool::new(true);
    let defective = proof.resolve_operation("op").unwrap();
    let unprepared = defective.input().unwrap();
    assert!(
        matches!(unprepared.prepare_with_control(&evaluator, &ResourceSet::default(), &cancelled), Err(detail) if detail.reason == NoVerdictReason::Cancelled)
    );
    assert!(
        matches!(unprepared.prepare(&evaluator), Err(detail) if detail.code == "anchor-missing")
    );
    let operation = proof.resolve_operation("ok").unwrap();
    let Ok(contract) = operation.input().unwrap().prepare(&evaluator) else {
        panic!("prepared")
    };
    assert!(
        matches!(contract.validate_with_control(&JsonValue::parse("1").unwrap(), &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
}

// The evaluator establishes no verdict that a static reference defect leaves
// open, even one that would not depend on it: it refuses preparation, naming
// the schema whose reference it could not follow.
fn refusal_at(document: String) -> NoVerdict {
    let parsed = Document::parse(document).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.conformant().unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    let Err(detail) = operation.input().unwrap().prepare(&DefaultEvaluator::new()) else {
        panic!("a defective reference prepared")
    };
    detail
}
#[test]
fn defective_references_refuse_preparation_naming_the_reference() {
    let defective = |properties: &str| {
        refusal_at(format!(
            r#"{{"openbindings":"0.2.0","schemas":{{"T":{{"$id":"https://defects.test/r","properties":{properties}}}}},"operations":{{"op":{{"input":{{"$ref":"https://defects.test/r"}}}}}}}}"#
        ))
    };
    let detail = defective(r##"{"b":{"$ref":"#nope"}}"##);
    assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
    assert_eq!(detail.code, "anchor-missing");
    assert_eq!(
        detail.location,
        Some(SchemaLocation::new(
            None,
            "/schemas/T/properties/b".parse().unwrap()
        ))
    );
    let detail = defective(r##"{"a":{"$ref":"#/$defs/missing"},"b":{"$ref":"#nope"}}"##);
    assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
    assert!(detail.location.is_some());
}
#[test]
fn values_reaching_a_missing_resource_give_no_verdict_naming_it() {
    let document = |properties: &str| {
        format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"properties":{properties}}}}}}}}}"#
        )
    };
    let parsed = Document::parse(document(r#"{"b":{"$ref":"https://missing.test/b"}}"#)).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.conformant().unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    let contract = operation
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new())
        .unwrap();
    assert!(matches!(
        contract.validate(&JsonValue::parse(r#"{"a":1}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
    let ValueOutcome::NoVerdict { detail } =
        contract.validate(&JsonValue::parse(r#"{"b":1}"#).unwrap())
    else {
        panic!("a value reaching the missing resource was decided")
    };
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    assert_eq!(detail.code, "resource-unavailable");
    assert_eq!(
        detail.location,
        Some(SchemaLocation::new(
            Some("https://missing.test/b".into()),
            "".parse().unwrap()
        ))
    );
    // A fragment of a missing resource has no stand-in: preparation refuses,
    // naming the reference.
    let detail = refusal_at(document(
        r#"{"b":{"$ref":"https://missing.test/b#/$defs/x"}}"#,
    ));
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    assert_eq!(
        detail.location,
        Some(SchemaLocation::new(
            None,
            "/operations/op/input/properties/b".parse().unwrap()
        ))
    );
}

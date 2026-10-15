use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

fn observe(case: &Value) -> Value {
    let document = case.get("document").cloned().unwrap_or_else(|| {
        json!({
            "openbindings":"0.2.0", "operations":{"op":{"input":case["schema"]}}
        })
    });
    let parsed = ParsedDocument::parse(document.to_string()).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.validated().expect("conformant fixture");
    let supplied = ResourceSet::new(case["resources"].as_array().into_iter().flatten().map(|r| {
        SchemaResource {
            uri: r["uri"].as_str().unwrap().into(),
            document: JsonValue::parse(r["schema"].to_string()).unwrap(),
        }
    }))
    .unwrap();
    let limits: Limits =
        serde_json::from_value(case.get("limits").cloned().unwrap_or(json!({}))).unwrap();
    let contracts =
        proof.value_contracts(Arc::new(DefaultEvaluator::with_limits(limits)), supplied);
    let (stage, result) = match contracts.prepare("op", Direction::Input) {
        ValueContractPreparation::Ready(contract) => (
            "ready",
            contract.validate(&JsonValue::parse(case["value"].to_string()).unwrap()),
        ),
        ValueContractPreparation::NoVerdict { detail } => {
            ("preparation", ValueOutcome::NoVerdict { detail })
        }
        other => panic!("{other:?}"),
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
fn semantic_preparation_and_validation_honor_cancellation() {
    let parsed = ParsedDocument::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$id":"https://controls.test/root","$ref":"#missing"}}}}"##).unwrap();
    let validation = parsed.validate().unwrap();
    let contracts = validation
        .validated()
        .unwrap()
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
    let cancelled = AtomicBool::new(true);
    assert!(
        matches!(contracts.prepare_with_control("op", Direction::Input, &cancelled), ValueContractPreparation::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
    let ValueContractPreparation::Ready(contract) = contracts.prepare("op", Direction::Input)
    else {
        panic!("prepared")
    };
    assert!(
        matches!(contract.validate_with_control(&JsonValue::parse("1").unwrap(), &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
}

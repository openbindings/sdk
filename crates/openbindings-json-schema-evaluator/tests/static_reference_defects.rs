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
    let Some(proof) = validation.conformant() else {
        return json!({"stage":"admission","outcome":validation.report().conclusion,"evidence":validation.report().evidence});
    };
    let supplied =
        ExternalSchemas::new(case["resources"].as_array().into_iter().flatten().map(|r| {
            ExternalSchema {
                uri: r["uri"].as_str().unwrap().into(),
                document: JsonValue::parse(r["schema"].to_string()).unwrap(),
            }
        }))
        .unwrap();
    let limits: Limits =
        serde_json::from_value(case.get("limits").cloned().unwrap_or(json!({}))).unwrap();
    let operation = proof.resolve_operation("op").expect("declared operation");
    let contract = operation.input().expect("declared input");
    let (stage, result) = match contract.prepare(
        &DefaultEvaluator::with_limits(limits),
        &supplied,
        &openbindings::PreparationLimits::default(),
    ) {
        Ok(contract) => (
            "ready",
            contract.evaluate(&JsonValue::parse(case["value"].to_string()).unwrap()),
        ),
        Err(failure) => {
            return json!({"stage":"preparation", "outcome":"refused", "failure":failure});
        }
    };
    match result {
        ValueOutcome::Satisfies => json!({"stage":stage,"outcome":"S"}),
        ValueOutcome::Fails { .. } => json!({"stage":stage,"outcome":"F"}),
        ValueOutcome::Undefined { detail } => {
            json!({"stage":stage,"outcome":"undefined", "detail":{
            "code":detail.code,"location":detail.location.map(|at| json!({"resource":at.source_uri,"pointer":at.pointer}))}})
        }
        ValueOutcome::NoVerdict { detail } => json!({"stage":stage,"outcome":"N", "detail":{
            "reason":detail.reason,"code":detail.code,"location":detail.location.map(|at| json!({"resource":at.source_uri,"pointer":at.pointer}))}}),
    }
}
#[test]
fn matrix_and_concrete_reference_completions() {
    let mut receipt = Vec::new();
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/reference-current-oracle.json")).unwrap();
    let oracle: std::collections::BTreeMap<_, _> = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| (case["id"].as_str().unwrap(), case))
        .collect();
    assert_eq!(oracle.len(), 2348);
    let mut failures = Vec::new();
    for text in [
        include_str!("fixtures/reference-matrix.json"),
        include_str!("fixtures/reference-completions.json"),
    ] {
        let cases: Vec<Value> = serde_json::from_str(text).unwrap();
        for case in cases {
            let actual = observe(&case);
            let id = case["id"].as_str().unwrap();
            let required = oracle[id]["required"].as_str().unwrap();
            let matched = match required {
                "S" | "F" => actual["stage"] == "ready" && actual["outcome"] == required,
                "U" => actual["stage"] == "ready" && actual["outcome"] == "undefined",
                "external" => {
                    actual["outcome"] == "N" && actual["detail"]["reason"] == "resource-unavailable"
                }
                "limit" => {
                    (actual["outcome"] == "N" && actual["detail"]["reason"] == "limit-exceeded")
                        || (actual["outcome"] == "refused" && actual["failure"]["kind"] == "limit")
                }
                "admission" => {
                    actual["stage"] == "admission"
                        && actual["outcome"] == "non-conformant"
                        && oracle[id]["rules"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|rule| actual["evidence"][rule.as_str().unwrap()] == "violated")
                }
                other => panic!("unknown oracle assertion {other}"),
            };
            if !matched {
                failures.push(format!("{id}: required {required}, observed {actual}"));
            }
            receipt.push(json!({"id":case["id"],"actual":actual}));
        }
    }
    if let Ok(path) = std::env::var("REFERENCE_RECEIPT") {
        std::fs::write(path, serde_json::to_string(&receipt).unwrap()).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn preparation_and_validation_honor_cancellation() {
    let parsed = Document::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://controls.test/missing#anchor"}},"ok":{"input":{"type":"integer"}}}}"##).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation.conformant().unwrap();
    let evaluator = DefaultEvaluator::new();
    let cancelled = AtomicBool::new(true);
    let defective = proof.resolve_operation("op").unwrap();
    let unprepared = defective.input().unwrap();
    assert!(
        matches!(unprepared.prepare_with_control(&evaluator, &ExternalSchemas::default(), &openbindings::PreparationLimits::default(), &cancelled), Err(detail) if detail.kind == PreparationFailureKind::Cancelled)
    );
    let prepared = unprepared
        .prepare(
            &evaluator,
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(prepared.evaluate(&JsonValue::null()), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable)
    );
    let operation = proof.resolve_operation("ok").unwrap();
    let Ok(contract) = operation.input().unwrap().prepare(
        &evaluator,
        &openbindings::ExternalSchemas::default(),
        &openbindings::PreparationLimits::default(),
    ) else {
        panic!("prepared")
    };
    assert!(
        matches!(contract.evaluate_with_control(&JsonValue::parse("1").unwrap(), &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
}

#[test]
fn defective_contained_references_are_admission_negatives_before_preparation() {
    for properties in [
        r##"{"b":{"$ref":"#nope"}}"##,
        r##"{"a":{"$ref":"#/$defs/missing"},"b":{"$ref":"#nope"}}"##,
    ] {
        let source = format!(
            r#"{{"openbindings":"0.2.0","schemas":{{"T":{{"$id":"https://defects.test/r","properties":{properties}}}}},"operations":{{"op":{{"input":{{"$ref":"https://defects.test/r"}}}}}}}}"#
        );
        let document = Document::parse(source).unwrap();
        let checked = document.validate().unwrap();
        assert_eq!(checked.report().evidence["OBI-12"], Evidence::Violated);
        assert!(checked.conformant().is_none());
    }
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
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        contract.evaluate(&JsonValue::parse(r#"{"a":1}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
    let ValueOutcome::NoVerdict { detail } =
        contract.evaluate(&JsonValue::parse(r#"{"b":1}"#).unwrap())
    else {
        panic!("a value reaching the missing resource was decided")
    };
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    assert_eq!(detail.code, "resource-unavailable");
    assert_eq!(
        detail.location,
        Some(SchemaOrigin::new(
            None,
            "/operations/op/input/properties/b/$ref".parse().unwrap(),
        ))
    );
    // A missing fragment is retained as a per-value external dependency too.
    let parsed = Document::parse(document(
        r#"{"b":{"$ref":"https://missing.test/b#/$defs/x"}}"#,
    ))
    .unwrap();
    let proof = parsed.validate().unwrap().conformant().unwrap();
    let prepared = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        prepared.evaluate(&JsonValue::parse(r#"{"a":1}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
    let ValueOutcome::NoVerdict { detail } =
        prepared.evaluate(&JsonValue::parse(r#"{"b":1}"#).unwrap())
    else {
        panic!("external fragment is unavailable");
    };
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    assert_eq!(
        detail.location,
        Some(SchemaOrigin::new(
            None,
            "/operations/op/input/properties/b/$ref".parse().unwrap(),
        ))
    );
}

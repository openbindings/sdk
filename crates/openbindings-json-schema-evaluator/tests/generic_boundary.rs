use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator};
use std::sync::atomic::AtomicBool;
fn schema(text: &str) -> Schema {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"run":{{"input":{text}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
    .resolve_operation("run")
    .unwrap()
    .input()
    .unwrap()
}
#[test]
fn direct_default_validator_refuses_ambiguous_instances() {
    let selected = schema(r#"{"type":"object"}"#);
    let external = ExternalSchemas::default();
    let limits = PreparationLimits::default();
    let control = AtomicBool::new(false);
    let validator: DefaultValidator = DefaultEvaluator::new()
        .prepare(
            EvaluationRequest {
                root: SchemaRoot::Contained(&selected),
                external_schemas: &external,
            },
            &limits,
            &control,
        )
        .unwrap();
    let duplicate = JsonValue::parse(r#"{"x":1,"x":2}"#).unwrap();
    for detail in [Detail::Outcome, Detail::Explain] {
        assert!(
            matches!(validator.evaluate(&duplicate, detail, &control), ValueOutcome::NoVerdict { detail } if detail.code == "duplicate-instance-members")
        );
    }
}
#[test]
fn unavailable_annotation_source_retains_its_resolution_cause() {
    let prepared = schema(r#"{"$id":"https://entry.test/","$ref":"https://missing.test/schema","unevaluatedProperties":false}"#)
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default()).unwrap();
    let ValueOutcome::NoVerdict { detail } =
        prepared.evaluate(&JsonValue::parse(r#"{"x":1}"#).unwrap())
    else {
        panic!("missing content can change validity and annotations");
    };
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    // The diagnostic names the original reference, not the missing content.
    assert_eq!(
        detail.location,
        Some(SchemaOrigin::new(
            None,
            "/operations/run/input/$ref".parse().unwrap(),
        ))
    );
}

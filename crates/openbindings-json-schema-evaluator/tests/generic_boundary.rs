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
    let direct: DefaultValidator = DefaultEvaluator::new()
        .prepare(
            EvaluationRequest {
                schema: &selected,
                external_schemas: &external,
            },
            &limits,
            &control,
        )
        .unwrap();
    let extracted = selected
        .prepare(&DefaultEvaluator::new(), &external, &limits)
        .unwrap()
        .into_validator();
    let duplicate = JsonValue::parse(r#"{"x":1,"x":2}"#).unwrap();
    for validator in [direct, extracted] {
        for detail in [Detail::Outcome, Detail::Explain] {
            assert!(
                matches!(validator.evaluate(&duplicate, detail, &control), ValueOutcome::NoVerdict { detail } if detail.code == "duplicate-instance-members")
            );
        }
    }
}
#[test]
fn unavailable_annotation_source_retains_its_resolution_cause() {
    let prepared = schema(r#"{"$id":"https://entry.test/","$ref":"https://missing.test/schema","unevaluatedProperties":false}"#)
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default(), &PreparationLimits::default()).unwrap();
    let ValueOutcome::NoVerdict { detail } =
        prepared.evaluate(&JsonValue::parse(r#"{"x":1}"#).unwrap())
    else {
        panic!("missing content can change validity and annotations");
    };
    assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
    // No invented source coordinate for a resource that was never supplied.
    assert!(detail.location.is_none());
}

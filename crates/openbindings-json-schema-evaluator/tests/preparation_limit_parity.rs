//! The TypeScript suite replays these same inputs, limits and refusal codes.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::atomic::AtomicBool;

fn refuses(case: usize, limits: PreparationLimits) {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/preparation-limit-parity.json")).unwrap();
    let document = Document::parse(fixture["document"].as_str().unwrap()).unwrap();
    let proof = document.validate().unwrap().conformant().unwrap();
    let schema = proof.resolve_operation("run").unwrap().input().unwrap();
    let uri = fixture["uri"].as_str().unwrap();
    let external = ExternalSchema {
        uri: uri.into(),
        document: JsonValue::parse(fixture["schema"].as_str().unwrap()).unwrap(),
    };
    let resources = ExternalSchemas::new([external.clone()]).unwrap();
    let evaluator = DefaultEvaluator::new();
    let control = AtomicBool::new(false);
    for result in [
        schema.prepare_with_control(&evaluator, &ExternalSchemas::default(), &limits, &control),
        schema.prepare_with_control(&evaluator, &resources, &limits, &control),
        resources.prepare_with_control(uri, &evaluator, &limits, &control),
    ] {
        let failure = result.unwrap_err();
        let PreparationFailureKind::Limit { resource } = failure.kind else {
            panic!("expected preparation limit, got {failure:?}")
        };
        assert_eq!(resource, fixture["cases"][case]["code"].as_str().unwrap());
    }
    let failure = ExternalSchemas::new_with_limits([external], &limits, &control).unwrap_err();
    let ExternalSchemaErrorKind::Limit { resource } = failure.kind else {
        panic!("expected registration limit, got {failure:?}")
    };
    assert_eq!(resource, fixture["cases"][case]["code"].as_str().unwrap());
    let retry = resources.prepare(uri, &evaluator).unwrap();
    assert!(matches!(
        retry.evaluate(&JsonValue::parse("7").unwrap()),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn schema_node_limit_matches_typescript() {
    refuses(
        0,
        PreparationLimits {
            max_schema_nodes: 0,
            ..PreparationLimits::default()
        },
    );
}
#[test]
fn identity_byte_limit_matches_typescript() {
    refuses(
        1,
        PreparationLimits {
            max_identity_bytes: 0,
            ..PreparationLimits::default()
        },
    );
}
#[test]
fn preparation_work_limit_matches_typescript() {
    refuses(
        2,
        PreparationLimits {
            max_sdk_work_units: Some(0),
            ..PreparationLimits::default()
        },
    );
}
#[test]
fn preparation_reservation_limit_matches_typescript() {
    refuses(
        3,
        PreparationLimits {
            max_sdk_reservation_bytes: 0,
            ..PreparationLimits::default()
        },
    );
}

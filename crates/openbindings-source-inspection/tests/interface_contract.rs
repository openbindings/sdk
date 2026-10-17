//! Independent wire-shape oracle: the pinned published interface document, not a
//! schema recreated from this implementation's native types.
use openbindings::{Document, JsonValue, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_source_inspection::*;

#[test]
fn inventories_validate_against_pinned_interface_output_contract() {
    let document = Document::parse(include_bytes!("fixtures/source-inspector-0.1.json")).unwrap();
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("openbindings.source-inspector.inspectSource")
        .and_then(|operation| operation.output()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("interface output contract did not prepare")
    };
    let target = BindableTarget::new(
        "空.🚲",
        BindingFragment {
            content: Some(JsonValue::parse("9007199254740993").unwrap()),
            idempotent: Some(false),
            ..Default::default()
        },
    );
    for inventory in [
        SourceInspection::complete(vec![]),
        SourceInspection::complete(vec![target]),
        SourceInspection::partial(
            vec![],
            InspectionLimitation::new("example.unavailable", "admitted records could not be read"),
        ),
    ] {
        assert!(matches!(
            contract.validate(&inventory.to_json().unwrap()),
            ValueOutcome::Satisfies
        ));
    }
    for invalid in [
        r#"{"targets":[],"exhaustive":false}"#,
        r#"{"targets":[{"sourceRef":"","binding":{}}],"exhaustive":true}"#,
        r#"{"targets":[],"exhaustive":false,"limitation":{"code":"","message":"missing"}}"#,
    ] {
        assert!(matches!(
            contract.validate(&JsonValue::parse(invalid).unwrap()),
            ValueOutcome::Fails { .. }
        ));
    }
}

#[test]
fn duplicate_recovery_is_sdk_policy_not_an_interface_requirement() {
    let document = Document::parse(include_bytes!("fixtures/source-inspector-0.1.json")).unwrap();
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("openbindings.source-inspector.inspectSource")
        .and_then(|operation| operation.output()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("interface output contract did not prepare")
    };
    // A consumer can retain repeated evidence or deduplicate it. Neither result
    // shape prescribes the consumer's recovery policy.
    for wire in [
        r#"{"targets":[{"sourceRef":"a","binding":{}},{"sourceRef":"a","binding":{}}],"exhaustive":true}"#,
        r#"{"targets":[{"sourceRef":"a","binding":{}}],"exhaustive":true}"#,
    ] {
        assert!(matches!(
            contract.validate(&JsonValue::parse(wire).unwrap()),
            ValueOutcome::Satisfies
        ));
    }
    let target = BindableTarget::new("a", BindingFragment::default());
    assert!(
        SourceInspection::complete(vec![target.clone(), target.clone()])
            .validate()
            .is_err()
    );
    assert!(SourceInspection::complete(vec![target]).validate().is_ok());
}

// the pinned InspectionLimitation schema omits additionalProperties,
// which permits additional evidence. Both serializers must preserve it exactly.
#[test]
fn limitation_additional_evidence_matches_the_published_interface() {
    let document = Document::parse(include_bytes!("fixtures/source-inspector-0.1.json")).unwrap();
    assert!(
        document
            .value()
            .view()
            .at("/schemas/InspectionLimitation")
            .unwrap()
            .get("additionalProperties")
            .is_none()
    );
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("openbindings.source-inspector.inspectSource")
        .and_then(|operation| operation.output()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("interface contract failed")
    };
    let mut limitation = InspectionLimitation::new(
        "example.remaining",
        "Some admitted targets could not be inspected",
    )
    .with_additional_fields(std::collections::BTreeMap::from([(
        "evidence".into(),
        JsonValue::parse(r#"{"count":9007199254740993,"state":null}"#).unwrap(),
    )]));
    let inventory = SourceInspection::partial(vec![], limitation.clone());
    let wire = inventory.to_json().unwrap();
    assert_eq!(wire.text(), serde_json::to_string(&inventory).unwrap());
    assert_eq!(
        wire.view()
            .at("/limitation/evidence/count")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    assert!(matches!(contract.validate(&wire), ValueOutcome::Satisfies));
    for key in ["code", "message", "details"] {
        limitation
            .additional_fields
            .insert(key.into(), JsonValue::null());
        assert!(serde_json::to_string(&limitation).is_err());
        assert!(
            SourceInspection::partial(vec![], limitation.clone())
                .to_json()
                .is_err()
        );
        limitation.additional_fields.remove(key);
    }
}

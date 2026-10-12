//! Independent wire-shape oracle: the pinned published role document, not a
//! schema recreated from this implementation's native types.
use openbindings::{
    ContractPreparation, JsonValue, ParsedDocument, ResourceSet, Side, ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_source_inspection::*;
use std::sync::Arc;

#[test]
fn inventories_validate_against_pinned_role_output_contract() {
    let document =
        ParsedDocument::parse(include_bytes!("fixtures/source-inspector-0.1.json")).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) =
        context.prepare("openbindings.source-inspector.inspectSource", Side::Output)
    else {
        panic!("role output contract did not prepare")
    };
    let target = BindableTarget {
        source_ref: "空.🚲".into(),
        binding: BindingFragment {
            content: Some(JsonValue::parse("9007199254740993").unwrap()),
            idempotent: Some(false),
            ..Default::default()
        },
        operation_key: None,
        operation: None,
    };
    for inventory in [
        SourceInspection::complete(vec![]),
        SourceInspection::complete(vec![target]),
        SourceInspection::partial(
            vec![],
            InspectionLimitation {
                code: "example.unavailable".into(),
                message: "admitted records could not be read".into(),
                details: None,
            },
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
    let document =
        ParsedDocument::parse(include_bytes!("fixtures/source-inspector-0.1.json")).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) =
        context.prepare("openbindings.source-inspector.inspectSource", Side::Output)
    else {
        panic!("role output contract did not prepare")
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
    let target = BindableTarget {
        source_ref: "a".into(),
        binding: BindingFragment::default(),
        operation_key: None,
        operation: None,
    };
    assert!(
        SourceInspection::complete(vec![target.clone(), target.clone()])
            .validate()
            .is_err()
    );
    assert!(SourceInspection::complete(vec![target]).validate().is_ok());
}

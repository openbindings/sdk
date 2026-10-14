use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};
use std::sync::Arc;

fn prepared(schema: Value, limits: Limits, resources: ResourceSet) -> ValueContract {
    let doc = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let context = doc
        .validate()
        .unwrap()
        .validated()
        .expect("conformant document")
        .value_contracts(Arc::new(DefaultEvaluator::with_limits(limits)), resources);
    match context.prepare("op", Direction::Input) {
        ValueContractPreparation::Ready(contract) => contract,
        other => panic!("{other:?}"),
    }
}
#[test]
fn coarse_is_value_free_and_upstream_is_one_first_error() {
    let schema = json!({"allOf":[{"type":"string"},false]});
    let value = JsonValue::parse("42").unwrap();
    let contract = prepared(schema.clone(), Limits::default(), ResourceSet::default());
    let outcome = contract.validate(&value);
    let ValueOutcome::Fails {
        message,
        problems,
        problems_complete,
    } = outcome
    else {
        panic!()
    };
    assert_eq!(message, "value does not satisfy the contract");
    assert!(problems.is_empty());
    assert!(!problems_complete);
    let doc = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let context = doc
        .validate()
        .unwrap()
        .validated()
        .unwrap()
        .value_contracts(
            Arc::new(
                DefaultEvaluator::new()
                    .with_diagnostics(openbindings_json_schema_evaluator::Diagnostics::Upstream),
            ),
            ResourceSet::default(),
        );
    let ValueContractPreparation::Ready(contract) = context.prepare("op", Direction::Input) else {
        panic!()
    };
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = contract.validate(&value)
    else {
        panic!()
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "type");
    assert_eq!(
        problems[0].schema_location.as_ref().unwrap().pointer,
        "/operations/op/input/allOf/0/type"
    );
    assert!(!problems_complete);
}
#[test]
fn removed_limits_are_rejected_and_modes_are_typed() {
    for field in ["diagnostic_bytes", "max_problems"] {
        assert!(serde_json::from_str::<Limits>(&format!("{{\"{field}\":1}}")).is_err());
    }
    use openbindings_json_schema_evaluator::Diagnostics;
    assert_eq!(Diagnostics::default(), Diagnostics::Coarse);
    assert!(serde_json::from_str::<Diagnostics>("true").is_err());
}

#[test]
fn interrupted_upstream_detail_cannot_weaken_an_established_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfterVerdict(AtomicUsize);
    impl WorkControl for CancelAfterVerdict {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 3
        }
    }
    let doc =
        ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":false}}}"#)
            .unwrap();
    let context = doc
        .validate()
        .unwrap()
        .validated()
        .unwrap()
        .value_contracts(
            Arc::new(
                DefaultEvaluator::new()
                    .with_diagnostics(openbindings_json_schema_evaluator::Diagnostics::Upstream),
            ),
            ResourceSet::default(),
        );
    let ValueContractPreparation::Ready(contract) = context.prepare("op", Direction::Input) else {
        panic!()
    };
    let result = contract
        .validate_with_control(&JsonValue::null(), &CancelAfterVerdict(AtomicUsize::new(0)));
    assert!(
        matches!(result, ValueOutcome::Fails { problems, problems_complete:false, .. } if problems.is_empty())
    );
}

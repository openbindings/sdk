//! Baseline-compatible regression witness: copy this test unchanged to a checkout
//! predating diagnostic_bytes. It fails on retained default-output amplification,
//! without depending on the new limit field or historical artifact paths.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;
use std::sync::Arc;

#[test]
fn default_diagnostics_do_not_amplify_long_locations_beyond_one_mib() {
    let key = "k".repeat(32768);
    let document = ParsedDocument::parse(json!({"openbindings":"0.2.0","operations":{"check":{"input":{"type":"object","properties":{&key:{"type":"object","additionalProperties":false}}}}}}).to_string()).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let fields: serde_json::Map<_, _> = (0..257).map(|i| (format!("f{i}"), json!(0))).collect();
    let input = JsonValue::parse(json!({&key:fields}).to_string()).unwrap();
    let ContractPreparation::Ready(contract) = context.prepare("check", Side::Input) else {
        panic!("expected ready contract");
    };
    let result = contract.validate(&input);
    let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = result
    else {
        panic!("expected failure: {result:?}")
    };
    assert!(!problems.is_empty());
    assert!(!problems_complete);
    let bytes: usize = problems
        .iter()
        .map(|problem| {
            assert_eq!(problem.code, "falseSchema");
            assert!(input.at(&problem.instance_pointer).is_some());
            let at = problem.schema_location.as_ref().unwrap();
            assert!(document.value().at(&at.pointer).is_some());
            problem.instance_pointer.len()
                + problem.code.len()
                + problem.message.len()
                + at.pointer.len()
                + at.resource.as_ref().map_or(0, String::len)
        })
        .sum();
    assert!(
        bytes <= 1_048_576,
        "default retained diagnostics: {bytes} bytes"
    );
}

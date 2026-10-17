//! Cheap-check result-shape checks. Allocation is independently enforced by
//! tools/diagnostic-memory-probe, including ordinary and both partial verdict paths.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;

#[test]
fn default_diagnostics_omit_problems_with_long_source_locations() {
    let key = "k".repeat(32768);
    let document = Document::parse(json!({"openbindings":"0.2.0","operations":{"check":{"input":{"type":"object","properties":{&key:{"type":"object","additionalProperties":false}}}}}}).to_string()).unwrap();
    let document = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = document
        .resolve_operation("check")
        .expect("declared operation");
    let fields: serde_json::Map<_, _> = (0..257).map(|i| (format!("f{i}"), json!(0))).collect();
    let input = JsonValue::parse(json!({&key:fields}).to_string()).unwrap();
    let Ok(contract) = operation
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
    else {
        panic!("expected ready contract");
    };
    let result = contract.validate(&input);
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = result
    else {
        panic!("expected failure: {result:?}")
    };
    assert!(problems.is_empty());
    assert!(!problems_complete);
}

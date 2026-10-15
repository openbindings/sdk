//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Diagnostics};
use std::fmt::Debug;
use std::sync::Arc;
// Keep the independent review's exact credential and pointer probe value.
const SECRET: &str = "REVIEW-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let parsed = ParsedDocument::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
    let validated = parsed.validate().unwrap().validated().unwrap();
    let mut rows = Vec::new();
    for diagnostics in [Diagnostics::Coarse, Diagnostics::Upstream] {
        let contracts = validated.value_contracts(
            Arc::new(DefaultEvaluator::new().with_diagnostics(diagnostics)),
            ResourceSet::default(),
        );
        rows.push(row("ValueContracts", &contracts));
        let prepared = contracts.prepare("op", Direction::Input);
        rows.push(row("ValueContractPreparation::Ready", &prepared));
        let ValueContractPreparation::Ready(contract) = prepared else {
            panic!("not ready")
        };
        rows.push(row("ValueContract", &contract));
        let value = JsonValue::parse(serde_json::json!({SECRET:7}).to_string()).unwrap();
        let result = contract.validate(&value);
        assert!(matches!(result, ValueOutcome::Fails { .. }));
        rows.push(row("actual evaluator outcome", &result));
    }

    check_rows(rows, 8);
}

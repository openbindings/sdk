//! Default value failures must not construct eager library diagnostics.
use super::{JsonValue, measured};
use openbindings::{
    Direction, ParsedDocument, ResourceSet, ValueContractPreparation, ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;
use std::sync::Arc;

pub fn run() {
    let types = json!({"anyOf":[{"type":"string"},{"type":"boolean"}]});
    let missing = json!({"$ref":"https://unavailable.invalid/value"});
    for (path, schema, expected) in [
        ("ordinary", types.clone(), "fails"),
        (
            "partial-upper",
            json!({"allOf":[types.clone(),missing.clone()]}),
            "fails",
        ),
        (
            "partial-lower",
            json!({"anyOf":[types,missing]}),
            "no-verdict",
        ),
    ] {
        let text =
            json!({"openbindings":"0.2.0","operations":{"check":{"input":schema}}}).to_string();
        let document = ParsedDocument::parse(text).unwrap();
        let validation = document.validate().unwrap();
        let context = validation
            .validated()
            .expect("conformant fixture")
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
        let ValueContractPreparation::Ready(contract) = context.prepare("check", Direction::Input)
        else {
            panic!("expected prepared {path} contract");
        };
        for count in [50_000, 1_000] {
            let text = format!("[{}0]", "0,".repeat(count - 1));
            let value = JsonValue::parse(&text).unwrap();
            for repetition in 0..3 {
                let (outcome, bytes, nanos) = measured(|| contract.validate(&value));
                match &outcome {
                    ValueOutcome::Fails {
                        problems,
                        problems_complete,
                        ..
                    } => {
                        assert_eq!(expected, "fails");
                        assert!(problems.is_empty() && !problems_complete);
                    }
                    ValueOutcome::NoVerdict { .. } => assert_eq!(expected, "no-verdict"),
                    other => panic!("unexpected {path} outcome: {other:?}"),
                }
                println!(
                    "{}",
                    json!({"phase":"default-value","path":path,"elements":count,
                    "source_bytes":text.len(),"repetition":repetition,"peak_bytes":bytes,"nanos":nanos,"outcome":outcome,"ceiling_bytes":65536})
                );
                assert!(
                    bytes <= 65536,
                    "coarse allocation: {path}: {bytes} exceeds 65536 bytes"
                );
            }
        }
    }
}

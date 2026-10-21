//! Default value failures must not construct eager library diagnostics.
use super::{JsonValue, measured};
use openbindings::{Document, ValueOutcome};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;

pub fn run(only: Option<&str>) {
    // A failing branch over every element: the library builds an error for each.
    let types = json!({"anyOf":[{"type":"string"},{"items":{"type":"string"}}]});
    let capped = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: Some(1_000),
        ..Limits::default()
    });
    let default = DefaultEvaluator::new();
    for (path, schema, evaluator, expected) in [
        ("ordinary", types.clone(), &default, "fails"),
        (
            "referenced",
            json!({"$defs":{"types":types.clone()},"allOf":[{"$ref":"#/operations/check/input/$defs/types"}]}),
            &default,
            "fails",
        ),
        // Reading every element exhausts the cap.
        (
            "work-capped",
            json!({"type":"array","items":{"type":"integer"}}),
            &capped,
            "no-verdict",
        ),
    ] {
        if only.is_some_and(|only| only != path) {
            continue;
        }
        let text =
            json!({"openbindings":"0.2.0","operations":{"check":{"input":schema}}}).to_string();
        let document = Document::parse(text).unwrap();
        let validation = document.validate().unwrap();
        let proof = validation.conformant().expect("conformant fixture");
        let Some(contract) = proof
            .resolve_operation("check")
            .and_then(|operation| operation.input()?.prepare(evaluator).ok())
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

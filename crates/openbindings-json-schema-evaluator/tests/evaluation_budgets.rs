//! The default work allowance grows with the value checked, and exhausted depth
//! is reported as such.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(schema: &str, limits: Limits) -> PreparedContract {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::with_limits(limits))
        .unwrap()
}
fn code(outcome: &ValueOutcome) -> Option<&str> {
    match outcome {
        ValueOutcome::NoVerdict { detail } => Some(&detail.code),
        _ => None,
    }
}

#[test]
fn work_allowance_grows_with_the_value() {
    let schema = r#"{"type":"array","items":{"type":"integer","minimum":0}}"#;
    // About three units per element: more than the 2,000,000 a small value gets.
    let items = 700_000;
    let large = JsonValue::parse(format!("[{}0]", "1,".repeat(items - 1))).unwrap();
    let defaults = prepared(schema, Limits::default());
    assert!(matches!(defaults.validate(&large), ValueOutcome::Satisfies));
    let failing = JsonValue::parse(format!("[{}-1]", "1,".repeat(items - 1))).unwrap();
    assert!(matches!(
        defaults.explain(&failing),
        ValueOutcome::Fails { problems, .. } if problems.len() == 1
    ));
    // An explicit allowance still bounds the work: it grows in proportion.
    let small = Limits {
        evaluation_steps: 1_000,
        ..Limits::default()
    };
    let bounded = prepared(schema, small);
    assert_eq!(
        code(&bounded.validate(&large)),
        Some("evaluation-work-limit")
    );
    assert!(matches!(
        bounded.validate(&JsonValue::parse("[1,2]").unwrap()),
        ValueOutcome::Satisfies
    ));
    let none = prepared(
        schema,
        Limits {
            evaluation_steps: 0,
            ..Limits::default()
        },
    );
    assert_eq!(code(&none.validate(&large)), Some("evaluation-work-limit"));
}

#[test]
fn deep_values_report_the_depth_limit() {
    let schema = r##"{"$defs":{"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/operations/op/input/$defs/node"}}]}},"$ref":"#/operations/op/input/$defs/node"}"##;
    let deep = JsonValue::parse(format!("{}0{}", "[".repeat(300), "]".repeat(300))).unwrap();
    let defaults = prepared(schema, Limits::default());
    // Ample work, but each level nests several evaluations.
    assert_eq!(
        code(&defaults.validate(&deep)),
        Some("evaluation-depth-limit")
    );
    let deeper = prepared(
        schema,
        Limits {
            evaluation_depth: 4096,
            ..Limits::default()
        },
    );
    assert!(matches!(deeper.validate(&deep), ValueOutcome::Satisfies));
}

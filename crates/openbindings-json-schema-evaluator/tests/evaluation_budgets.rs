//! The default work allowance grows with the value checked, an explicit one caps
//! every pass, and exhausted depth is reported as such.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(schema: &str, limits: Limits) -> PreparedValueContract {
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

const SCHEMA: &str = r#"{"type":"array","items":{"type":"integer","minimum":0}}"#;
// About three units per element: more than the 2,000,000 a small value gets.
const ITEMS: usize = 700_000;
fn large() -> JsonValue {
    JsonValue::parse(format!("[{}0]", "1,".repeat(ITEMS - 1))).unwrap()
}
fn explicit(steps: usize) -> PreparedValueContract {
    prepared(
        SCHEMA,
        Limits {
            evaluation_steps: Some(steps),
            ..Limits::default()
        },
    )
}

#[test]
fn work_allowance_grows_with_the_value() {
    let defaults = prepared(SCHEMA, Limits::default());
    assert!(matches!(
        defaults.validate(&large()),
        ValueOutcome::Satisfies
    ));
    let failing = JsonValue::parse(format!("[{}-1]", "1,".repeat(ITEMS - 1))).unwrap();
    assert!(matches!(
        defaults.explain(&failing),
        ValueOutcome::Fails { problems, .. } if problems.len() == 1
    ));
}

#[test]
fn an_explicit_work_allowance_caps_every_pass_whatever_the_value_size() {
    let large = large();
    // The default's allowance for a small value, stated explicitly, does not
    // grow: the large value the default decides exhausts it.
    assert_eq!(
        code(&explicit(2_000_000).validate(&large)),
        Some("evaluation-work-limit")
    );
    assert_eq!(
        code(&explicit(1_000).validate(&large)),
        Some("evaluation-work-limit")
    );
    assert_eq!(
        code(&explicit(0).validate(&large)),
        Some("evaluation-work-limit")
    );
    assert!(matches!(
        explicit(1_000).validate(&JsonValue::parse("[1,2]").unwrap()),
        ValueOutcome::Satisfies
    ));
    // A cap above what the value needs decides it.
    assert!(matches!(
        explicit(10_000_000).validate(&large),
        ValueOutcome::Satisfies
    ));
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

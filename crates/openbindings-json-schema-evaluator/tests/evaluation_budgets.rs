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
fn the_default_allowance_grows_in_proportion_not_without_bound() {
    // Each `allOf` branch costs a few units per element, so these two checks
    // need about 4,000,000 and 16,000,000 units for a value of four times the
    // node count the 2,000,000-unit default is stated for.
    fn all_of(branches: usize) -> String {
        let branches = vec![r#"{"minimum":0}"#; branches].join(",");
        format!(r#"{{"type":"array","items":{{"allOf":[{branches}]}}}}"#)
    }
    let (cheaper, costlier) = (all_of(6), all_of(29));
    // The array and its elements: 4 x 65,536 nodes.
    let value = JsonValue::parse(format!("[{}0]", "1,".repeat(4 * 65_536 - 2))).unwrap();
    let outcome = |schema: &str, limits| prepared(schema, limits).validate(&value);
    let capped = |steps| Limits {
        evaluation_steps: Some(steps),
        ..Limits::default()
    };
    // Bracket each check's need with explicit allowances.
    for (schema, enough, too_little) in [
        (&cheaper, 8_000_000, 2_000_000),
        (&costlier, 32_000_000, 8_000_000),
    ] {
        let decided = outcome(schema, capped(enough));
        assert!(matches!(decided, ValueOutcome::Satisfies));
        let refused = outcome(schema, capped(too_little));
        assert_eq!(code(&refused), Some("evaluation-work-limit"));
    }
    // The default gives this value four times 2,000,000 units, and no more.
    let decided = outcome(&cheaper, Limits::default());
    assert!(matches!(decided, ValueOutcome::Satisfies));
    let refused = outcome(&costlier, Limits::default());
    assert_eq!(code(&refused), Some("evaluation-work-limit"));
}

#[test]
fn an_evaluator_reports_the_budgets_it_was_given() {
    let limits = Limits {
        evaluation_steps: Some(7),
        evaluation_depth: 11,
        regex_steps: 13,
        compile_json_depth: 17,
        pattern_bytes: 19,
        pattern_depth: 23,
    };
    let evaluator = DefaultEvaluator::with_limits(limits);
    let reported = evaluator.limits();
    assert_eq!(reported.evaluation_steps, Some(7));
    assert_eq!(
        (
            reported.evaluation_depth,
            reported.regex_steps,
            reported.compile_json_depth,
            reported.pattern_bytes,
            reported.pattern_depth
        ),
        (11, 13, 17, 19, 23)
    );
    assert_eq!(DefaultEvaluator::new().limits().evaluation_steps, None);
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

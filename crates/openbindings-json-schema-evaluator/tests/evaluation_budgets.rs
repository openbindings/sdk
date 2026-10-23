//! Evaluation work is uncapped unless the application sets a cap, which then
//! applies to every pass whatever the value's size; exhausted depth is
//! reported as such.
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
// Under a cap, two units per element: reading it and entering its schema.
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
fn uncapped_evaluation_decides_large_values() {
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
fn explicit_caps_bracket_a_checks_work_and_the_default_sets_none() {
    // Each `allOf` branch costs at least a unit per element for the reference
    // it enters, so the costlier check needs several times the cheaper one's
    // units; each pair of caps brackets a check's need.
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
    // Without a cap both are decided.
    for schema in [&cheaper, &costlier] {
        assert!(matches!(
            outcome(schema, Limits::default()),
            ValueOutcome::Satisfies
        ));
    }
}

#[test]
fn an_evaluator_reports_the_budgets_it_was_given() {
    let limits = Limits {
        evaluation_steps: Some(7),
        stack_bytes: 11,
        compile_json_depth: 17,
        pattern_bytes: 19,
        pattern_depth: 23,
        total_pattern_bytes: 31,
        total_pattern_memory: 37,
        program_bytes: 41,
        schema_resources: Some(29),
    };
    let evaluator = DefaultEvaluator::with_limits(limits);
    let reported = evaluator.limits();
    assert_eq!(reported.evaluation_steps, Some(7));
    assert_eq!(reported.schema_resources, Some(29));
    assert_eq!(reported.total_pattern_bytes, 31);
    assert_eq!(reported.total_pattern_memory, 37);
    assert_eq!(
        DefaultEvaluator::new().limits().total_pattern_bytes,
        4 << 20
    );
    assert_eq!(
        DefaultEvaluator::new().limits().total_pattern_memory,
        64 << 20
    );
    assert_eq!(reported.program_bytes, 41);
    assert_eq!(DefaultEvaluator::new().limits().stack_bytes, 256 << 10);
    assert_eq!(DefaultEvaluator::new().limits().program_bytes, 48 << 20);
    assert_eq!(
        (
            reported.stack_bytes,
            reported.compile_json_depth,
            reported.pattern_bytes,
            reported.pattern_depth
        ),
        (11, 17, 19, 23)
    );
    assert_eq!(DefaultEvaluator::new().limits().evaluation_steps, None);
    assert_eq!(DefaultEvaluator::new().limits().schema_resources, None);
}

#[test]
fn an_explicit_work_allowance_caps_every_pass_whatever_the_value_size() {
    let large = large();
    for steps in [1_000_000, 1_000, 0] {
        assert_eq!(
            code(&explicit(steps).validate(&large)),
            Some("evaluation-work-limit")
        );
    }
    assert!(matches!(
        explicit(1_000).validate(&JsonValue::parse("[1,2]").unwrap()),
        ValueOutcome::Satisfies
    ));
    // A cap above what the value needs decides it.
    assert!(matches!(
        explicit(10_000_000).validate(&large),
        ValueOutcome::Satisfies
    ));
    // An exhausted explanation pass keeps the failure the verdict pass
    // established.
    let failing = JsonValue::parse(format!("[{}-1]", "1,".repeat(ITEMS - 1))).unwrap();
    let capped = explicit(1_500_000);
    assert!(matches!(
        capped.explain(&failing),
        ValueOutcome::Fails { problems, .. } if problems.len() == 1
    ));
}

#[test]
fn deep_values_report_the_depth_limit() {
    let schema = r##"{"$defs":{"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/operations/op/input/$defs/node"}}]}},"$ref":"#/operations/op/input/$defs/node"}"##;
    let deep = JsonValue::parse(format!("{}0{}", "[".repeat(300), "]".repeat(300))).unwrap();
    let defaults = prepared(schema, Limits::default());
    // Ample work, but each level nests several evaluations: 1,024 nested
    // evaluations fit the default stack, so values up to 203 levels deep are
    // evaluated, and the refusal says why and what to raise.
    let refused = defaults.validate(&deep);
    let ValueOutcome::NoVerdict { detail } = &refused else {
        panic!("{refused:?}")
    };
    assert_eq!(detail.code, "evaluation-depth-limit");
    assert_eq!(
        detail.message,
        "this value nests more than 203 levels, the deepest this value contract evaluates within the \
         1024 schema evaluations its 256 KiB stack_bytes holds at 256 bytes each; raise \
         stack_bytes (stackBytes in TypeScript)"
    );
    // The bound grows with the stack the host gives.
    let deeper = prepared(
        schema,
        Limits {
            stack_bytes: 1024 * 1024,
            ..Limits::default()
        },
    );
    assert!(matches!(deeper.validate(&deep), ValueOutcome::Satisfies));
}

#[test]
fn a_contract_too_deep_for_the_stack_refuses_every_value() {
    // A chain of 300 references, each nesting one evaluation deeper, holds
    // more evaluations than a 64 KiB stack does before any value is read.
    let links: Vec<_> = (0..300)
        .map(|i| {
            format!(
                r##""d{i}":{{"$ref":"#/operations/op/input/$defs/d{}"}}"##,
                i + 1
            )
        })
        .collect();
    let schema = format!(
        r##"{{"$ref":"#/operations/op/input/$defs/d0","$defs":{{{},"d300":{{"type":"integer"}}}}}}"##,
        links.join(",")
    );
    let small = prepared(
        &schema,
        Limits {
            stack_bytes: 64 * 1024,
            ..Limits::default()
        },
    );
    let ValueOutcome::NoVerdict { detail } = small.validate(&JsonValue::parse("1").unwrap()) else {
        panic!("a 64 KiB stack evaluates no value through 300 references")
    };
    assert_eq!(
        detail.message,
        "evaluating any value against this value contract could nest more than the 256 schema \
         evaluations its 64 KiB stack_bytes holds at 256 bytes each; raise stack_bytes \
         (stackBytes in TypeScript)"
    );
    // The default stack holds them.
    let defaults = prepared(&schema, Limits::default());
    assert!(matches!(
        defaults.validate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Satisfies
    ));
}

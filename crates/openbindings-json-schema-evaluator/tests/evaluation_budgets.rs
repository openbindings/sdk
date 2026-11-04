//! Evaluation work has no default cap. A caller-selected cap covers every
//! pass; storage admission and cooperative cancellation remain separate.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(
    schema: &str,
    limits: Limits,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
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
        .prepare(
            &DefaultEvaluator::with_limits(limits),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap()
}
fn code(outcome: &ValueOutcome) -> Option<&str> {
    match outcome {
        ValueOutcome::NoVerdict { detail } => Some(&detail.code),
        _ => None,
    }
}

const SCHEMA: &str = r#"{"type":"array","items":{"type":"integer","minimum":0}}"#;
// Large values exercise traversal and primitive work under one allowance.
const ITEMS: usize = 700_000;
fn large() -> JsonValue {
    JsonValue::parse(format!("[{}0]", "1,".repeat(ITEMS - 1))).unwrap()
}
fn explicit(steps: usize) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    prepared(
        SCHEMA,
        Limits {
            evaluation_sdk_work_units: Some(steps),
            ..Limits::default()
        },
    )
}

#[test]
fn repeated_root_proofs_fit_a_small_reservation_without_copying_empty_storage() {
    let references = vec![r##"{"$ref":"#/$defs/leaf"}"##; 5000].join(",");
    let schema = format!(
        r#"{{"$id":"https://budget.example/root","$defs":{{"leaf":{{"type":"integer"}}}},"allOf":[{references}]}}"#
    );
    let contract = prepared(
        &schema,
        Limits {
            evaluation_sdk_reservation_bytes: 64 * 1024,
            ..Limits::default()
        },
    );
    for (text, satisfies) in [("1", true), (r#""wrong""#, false)] {
        let value = JsonValue::parse(text).unwrap();
        for outcome in [contract.evaluate(&value), contract.explain(&value)] {
            assert!(
                matches!(
                    outcome,
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ),
                "{text}: {outcome:?}"
            );
            assert_eq!(matches!(outcome, ValueOutcome::Satisfies), satisfies);
        }
    }
}

#[test]
fn uncapped_evaluation_decides_large_values() {
    let defaults = prepared(SCHEMA, Limits::default());
    assert!(matches!(
        defaults.evaluate(&large()),
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
    let outcome = |schema: &str, limits| prepared(schema, limits).evaluate(&value);
    let capped = |steps| Limits {
        evaluation_sdk_work_units: Some(steps),
        ..Limits::default()
    };
    // Original-engine work includes exact primitive and frame dispatch work.
    // The values and expected semantics are unchanged; these generous caps
    // bracket the current units (exact/one-below is covered separately).
    for (schema, enough, too_little) in [
        (&cheaper, 64_000_000, 2_000_000),
        (&costlier, 256_000_000, 8_000_000),
    ] {
        let decided = outcome(schema, capped(enough));
        assert!(
            matches!(decided, ValueOutcome::Satisfies),
            "{enough}: {decided:?}"
        );
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
fn evaluation_allowance_serialization_uses_only_current_names() {
    use serde_json::json;
    let defaults = serde_json::to_value(Limits::default()).unwrap();
    assert_eq!(defaults["evaluation_sdk_work_units"], json!(null));
    assert_eq!(
        defaults["evaluation_sdk_reservation_bytes"],
        json!(48 << 20)
    );
    assert!(defaults.get("evaluation_steps").is_none());
    assert!(defaults.get("program_bytes").is_none());
    for work in [json!(null), json!(0), json!(17)] {
        for bytes in [0, 41] {
            let supplied = json!({
                "evaluation_sdk_work_units": work,
                "evaluation_sdk_reservation_bytes": bytes,
            });
            let limits: Limits = serde_json::from_value(supplied.clone()).unwrap();
            let encoded = serde_json::to_value(limits).unwrap();
            for (key, value) in supplied.as_object().unwrap() {
                assert_eq!(&encoded[key], value);
            }
        }
    }
    for retired in ["evaluation_steps", "program_bytes"] {
        for value in [json!(null), json!(0), json!(17)] {
            let mut keys = json!({});
            keys[retired] = value;
            assert!(serde_json::from_value::<Limits>(keys.clone()).is_err());
            keys["evaluation_sdk_work_units"] = json!(null);
            keys["evaluation_sdk_reservation_bytes"] = json!(0);
            assert!(serde_json::from_value::<Limits>(keys).is_err());
        }
    }
    for invalid in [json!(null), json!(-1), json!(1.5), json!("41")] {
        assert!(
            serde_json::from_value::<Limits>(json!({
                "evaluation_sdk_reservation_bytes": invalid,
            }))
            .is_err()
        );
    }
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        serde_json::from_value::<Limits>(json!({"evaluation_sdk_work_units": 4_294_967_296u64}))
            .unwrap()
            .evaluation_sdk_work_units,
        Some(4_294_967_296)
    );
}

#[test]
fn an_evaluator_reports_the_budgets_it_was_given() {
    let limits = Limits {
        evaluation_sdk_work_units: Some(7),
        pattern_bytes: 19,
        pattern_depth: 23,
        total_pattern_bytes: 31,
        total_pattern_memory: 37,
        evaluation_sdk_reservation_bytes: 41,
        schema_resources: Some(29),
    };
    let evaluator = DefaultEvaluator::with_limits(limits);
    let reported = evaluator.limits();
    assert_eq!(reported.evaluation_sdk_work_units, Some(7));
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
    assert_eq!(reported.evaluation_sdk_reservation_bytes, 41);
    assert_eq!(
        DefaultEvaluator::new()
            .limits()
            .evaluation_sdk_reservation_bytes,
        48 << 20
    );
    assert_eq!((reported.pattern_bytes, reported.pattern_depth), (19, 23));
    assert_eq!(
        DefaultEvaluator::new().limits().evaluation_sdk_work_units,
        None
    );
    assert_eq!(DefaultEvaluator::new().limits().schema_resources, None);
}

#[test]
fn an_explicit_work_allowance_caps_every_pass_whatever_the_value_size() {
    let large = large();
    for steps in [1_000_000, 1_000, 0] {
        assert_eq!(
            code(&explicit(steps).evaluate(&large)),
            Some("evaluation-work-limit")
        );
    }
    assert!(matches!(
        explicit(1_000).evaluate(&JsonValue::parse("[1,2]").unwrap()),
        ValueOutcome::Satisfies
    ));
    // A cap above what the value needs decides it.
    assert!(matches!(
        explicit(64_000_000).evaluate(&large),
        ValueOutcome::Satisfies
    ));
    // Preserve the former1.5M-unit request: the current evaluator now exhausts
    // it before completing this large verdict. Funded evaluation still fails.
    let failing = JsonValue::parse(format!("[{}-1]", "1,".repeat(ITEMS - 1))).unwrap();
    assert_eq!(
        code(&explicit(1_500_000).explain(&failing)),
        Some("evaluation-work-limit")
    );
    assert!(
        matches!(explicit(64_000_000).explain(&failing), ValueOutcome::Fails { problems, .. } if problems.len() == 1)
    );
}

#[test]
fn deep_concrete_values_do_not_spend_symbolic_effects() {
    let schema = r##"{"$defs":{"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/operations/op/input/$defs/node"}}]}},"$ref":"#/operations/op/input/$defs/node"}"##;
    let defaults = prepared(schema, Limits::default());
    for (leaf, satisfies) in [("0", true), ("false", false)] {
        let deep =
            JsonValue::parse(format!("{}{leaf}{}", "[".repeat(300), "]".repeat(300))).unwrap();
        for outcome in [defaults.evaluate(&deep), defaults.explain(&deep)] {
            assert!(
                matches!(outcome, ValueOutcome::Satisfies) && satisfies
                    || matches!(outcome, ValueOutcome::Fails { .. }) && !satisfies,
                "{outcome:?}"
            );
        }
    }
}

#[test]
fn long_alias_chains_do_not_inherit_the_retired_compiler_stack_bound() {
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
    // The same300-link input now evaluates through explicit original frames.
    let defaults = prepared(&schema, Limits::default());
    assert!(matches!(
        defaults.evaluate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Satisfies
    ));
}

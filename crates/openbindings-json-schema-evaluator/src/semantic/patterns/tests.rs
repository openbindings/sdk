use super::*;
use openbindings_internal_schema::preparation::Attempt;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

fn plan(schema: Value, limits: Limits) -> Plan {
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://patterns.test/root"}}}).to_string(),
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let preparation = PreparationLimits::default();
    let external = ExternalSchemas::new([ExternalSchema {
        uri: "https://patterns.test/root".into(),
        document: JsonValue::parse(schema.to_string()).unwrap(),
    }])
    .unwrap();
    let environment = document
        .schema("entry")
        .unwrap()
        .environment(&external, &preparation, &AtomicBool::new(false))
        .unwrap();
    Attempt::run(
        preparation.max_sdk_work_units,
        preparation.max_sdk_reservation_bytes,
        &|| false,
        |a| Plan::new(environment, limits, preparation, a),
    )
    .unwrap()
    .unwrap()
}
fn outcome(plan: &Plan, value: Value) -> ValueOutcome {
    plan.evaluate(
        &JsonValue::parse(value.to_string()).unwrap(),
        Detail::Outcome,
        &AtomicBool::new(false),
    )
}

#[test]
fn values_names_and_original_occurrences_share_only_immutable_programs() {
    let before = compiler::build_count();
    let plan = plan(
        json!({"pattern":"a|b","patternProperties":{"\\u0061|b":true},"properties":{"x":{"pattern":"a|b"}}}),
        Limits::default(),
    );
    let programs: Vec<_> = plan.patterns.values().collect();
    assert_eq!(programs.len(), 3);
    assert!(programs.iter().all(|p| Arc::ptr_eq(p, programs[0])));
    assert_eq!(compiler::build_count(), before + 1);
    for _ in 0..5 {
        assert!(matches!(
            outcome(&plan, json!("a")),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            outcome(&plan, json!({"a":1,"x":"b"})),
            ValueOutcome::Satisfies
        ));
    }
    assert_eq!(compiler::build_count(), before + 1);
}

#[test]
fn sharing_never_merges_distinct_undefined_effects() {
    let separate = plan(
        json!({"allOf":[{"pattern":"["},{"not":{"pattern":"["}}]}),
        Limits::default(),
    );
    assert!(matches!(
        outcome(&separate, json!("x")),
        ValueOutcome::Undefined { .. }
    ));
    let same = plan(
        json!({"$defs":{"A":{"pattern":"["}},"allOf":[{"$ref":"#/$defs/A"},{"not":{"$ref":"#/$defs/A"}}]}),
        Limits::default(),
    );
    assert!(matches!(
        outcome(&same, json!("x")),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn limited_programs_are_terminal_only_when_reached() {
    for limits in [
        Limits {
            pattern_bytes: 1,
            ..Limits::default()
        },
        Limits {
            pattern_depth: 0,
            ..Limits::default()
        },
        Limits::default(),
    ] {
        let source = if limits.pattern_bytes == 1 {
            "ab"
        } else if limits.pattern_depth == 0 {
            "(a)"
        } else {
            "a{65535}"
        };
        let p = plan(
            json!({"properties":{"x":{"pattern":source}}}),
            limits.clone(),
        );
        for value in [json!(null), json!({}), json!({"x":17})] {
            assert!(matches!(outcome(&p, value), ValueOutcome::Satisfies));
        }
        assert!(
            matches!(outcome(&p,json!({"x":"x"})),ValueOutcome::NoVerdict{detail}if detail.reason==NoVerdictReason::LimitExceeded)
        );
        let names = plan(json!({"patternProperties":{source:true}}), limits);
        for value in [json!(null), json!({})] {
            assert!(matches!(outcome(&names, value), ValueOutcome::Satisfies));
        }
        assert!(
            matches!(outcome(&names,json!({"x":17})),ValueOutcome::NoVerdict{detail}if detail.reason==NoVerdictReason::LimitExceeded)
        );
    }
}

#[test]
fn fixed_tiny_program_jobs_prepare_once_under_defaults() {
    for count in [100, 300, 400, 1024] {
        let members: serde_json::Map<String, Value> = (0..count)
            .map(|n| {
                (
                    format!("p{n}"),
                    json!({"pattern":format!("a{n:04}|b{n:04}")}),
                )
            })
            .collect();
        let before = compiler::build_count();
        let p = plan(json!({"properties":members}), Limits::default());
        assert_eq!(compiler::build_count(), before + count);
        for _ in 0..3 {
            assert!(matches!(outcome(&p, json!({})), ValueOutcome::Satisfies));
        }
        assert!(matches!(
            outcome(&p, json!({"p0":"a0000"})),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            outcome(&p, json!({"p0":"x"})),
            ValueOutcome::Fails { .. }
        ));
        assert_eq!(compiler::build_count(), before + count);
    }
}

#[test]
fn cancellation_and_concurrent_searches_do_not_poison_shared_programs() {
    let p = plan(json!({"pattern":"^(ab|é😀){1,2}$"}), Limits::default());
    let stop = AtomicBool::new(true);
    let good = JsonValue::parse(r#""é😀ab""#).unwrap();
    assert!(
        matches!(p.evaluate(&good,Detail::Outcome,&stop),ValueOutcome::NoVerdict{detail}if detail.reason==NoVerdictReason::Cancelled)
    );
    stop.store(false, Ordering::Relaxed);
    let before = compiler::build_count();
    std::thread::scope(|scope| {
        for text in ["ab", "é😀ab", "x", "ab\n"] {
            let p = &p;
            scope.spawn(move || {
                for _ in 0..20 {
                    let result = outcome(p, json!(text));
                    assert_eq!(
                        matches!(result, ValueOutcome::Satisfies),
                        matches!(text, "ab" | "é😀ab")
                    );
                }
            });
        }
    });
    assert!(matches!(
        p.evaluate(&good, Detail::Outcome, &stop),
        ValueOutcome::Satisfies
    ));
    assert_eq!(compiler::build_count(), before);
}

#[test]
fn unused_wide_pattern_keeps_its_own_sibling_reason() {
    let wide = r"\S".repeat(6000);
    for (first, second) in [("(?!a)", wide.as_str()), (wide.as_str(), "(?!a)")] {
        let p = plan(
            json!({"properties":{"a":{"pattern":first},"b":{"pattern":second},"c":{"pattern":"\\Bx"}}}),
            Limits::default(),
        );
        for (name, source) in [("a", first), ("b", second), ("c", r"\Bx")] {
            let ValueOutcome::NoVerdict { detail } = outcome(&p, json!({name:"x"})) else {
                panic!("{name}")
            };
            let expected = if source.starts_with("(?") {
                "pattern-backtracking"
            } else if source.starts_with(r"\B") {
                "pattern-non-word-boundary"
            } else {
                "pattern-size-limit"
            };
            assert_eq!(detail.code, expected);
        }
        assert!(matches!(outcome(&p, json!({})), ValueOutcome::Satisfies));
    }
}

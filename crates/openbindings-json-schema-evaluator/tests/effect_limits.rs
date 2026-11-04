//! Symbolic effects have a fixed logical ceiling independent of physical stack
//! size and of concrete schema/instance nesting.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};

fn schema(input: Value) -> Schema {
    Document::parse(json!({"openbindings":"0.2.0","operations":{"op":{"input":input}}}).to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .operation("op")
        .unwrap()
        .input()
        .unwrap()
}
fn prepare(
    input: Value,
    limits: Limits,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    schema(input)
        .prepare(
            &DefaultEvaluator::with_limits(limits),
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap()
}
fn effects(count: usize) -> Value {
    json!({"allOf":(0..count).map(|_| json!({"pattern":"["})).collect::<Vec<_>>()})
}
#[test]
fn distinct_effects_have_one_63_64_65_boundary_in_both_detail_modes() {
    let value = JsonValue::parse(r#""x""#).unwrap();
    for count in [63, 64, 65] {
        let prepared = prepare(effects(count), Limits::default());
        for outcome in [prepared.evaluate(&value), prepared.explain(&value)] {
            if count <= 64 {
                assert!(
                    matches!(outcome, ValueOutcome::Undefined { .. }),
                    "{count}: {outcome:?}"
                );
            } else {
                assert!(matches!(outcome, ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "semantic-effect-limit"));
            }
        }
        assert!(
            matches!(prepared.evaluate_with_control(&value, &std::sync::atomic::AtomicBool::new(true)),
            ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
        );
    }
}
#[test]
fn same_original_effect_and_equal_subjects_reuse_one_variable() {
    let repeated = json!({"$defs":{"p":{"pattern":"["}},"allOf":(0..128).map(|_|json!({"$ref":"#/operations/op/input/$defs/p"})).collect::<Vec<_>>()});
    let prepared = prepare(repeated, Limits::default());
    assert!(matches!(
        prepared.evaluate_json(r#""x""#).unwrap(),
        ValueOutcome::Undefined { .. }
    ));
    let items = prepare(
        json!({"items":{"allOf":[{"pattern":"["}]}}),
        Limits::default(),
    );
    assert!(matches!(
        items
            .evaluate_json(json!(vec!["x"; 128]).to_string())
            .unwrap(),
        ValueOutcome::Undefined { .. }
    ));
    let distinct = (0..65).map(|n| format!("x{n}")).collect::<Vec<_>>();
    assert!(
        matches!(items.evaluate_json(json!(distinct).to_string()).unwrap(), ValueOutcome::NoVerdict { detail } if detail.code == "semantic-effect-limit")
    );
}
#[test]
fn actual_work_and_storage_exhaustion_precede_the_effect_ceiling() {
    for (limits, code) in [
        (
            Limits {
                evaluation_steps: Some(0),
                ..Limits::default()
            },
            "evaluation-work-limit",
        ),
        (
            Limits {
                program_bytes: 0,
                ..Limits::default()
            },
            "semantic-analysis-memory-limit",
        ),
    ] {
        let prepared = prepare(effects(65), limits);
        assert!(
            matches!(prepared.evaluate_json(r#""x""#).unwrap(), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded && detail.code == code)
        );
    }
    assert!(matches!(
        prepare(effects(64), Limits::default())
            .evaluate_json(r#""x""#)
            .unwrap(),
        ValueOutcome::Undefined { .. }
    ));
}
#[test]
fn selected_meta_evaluation_uses_the_same_effect_profile() {
    const META: &str = "https://effects.test/meta";
    const RESOURCE: &str = "https://effects.test/resource";
    for count in [63, 64, 65] {
        let resources = ExternalSchemas::new([
            ExternalSchema { uri:META.into(), document:JsonValue::parse(json!({
                "$id":META, "$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/validation":true},
                "properties":{"title":effects(count)}
            }).to_string()).unwrap() },
            ExternalSchema { uri:RESOURCE.into(), document:JsonValue::parse(json!({"$schema":META,"title":"x"}).to_string()).unwrap() },
        ]).unwrap();
        let failure = schema(json!({"$ref":RESOURCE}))
            .prepare(
                &DefaultEvaluator::new(),
                &resources,
                &PreparationLimits::default(),
            )
            .unwrap_err();
        let expected = if count <= 64 {
            PreparationFailureKind::UnsupportedCapability {
                capability: "custom-meta-undefined".into(),
            }
        } else {
            PreparationFailureKind::Limit {
                resource: "semantic-effect-limit".into(),
            }
        };
        assert_eq!(failure.kind, expected, "{count}");
    }
}
#[test]
fn removed_compiler_and_stack_options_are_rejected() {
    for key in ["stack_bytes", "compile_json_depth"] {
        assert!(
            serde_json::from_value::<Limits>(json!({key:0})).is_err(),
            "{key}"
        );
        assert!(
            serde_json::to_value(Limits::default())
                .unwrap()
                .get(key)
                .is_none()
        );
    }
}

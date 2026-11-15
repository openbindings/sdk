use openbindings::{Document, ExternalSchemas, JsonValue, NoVerdictReason, ValueOutcome};
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};
use serde_json::json;

fn prepare(
    schema: serde_json::Value,
    limits: Limits,
) -> openbindings::PreparedSchema<DefaultValidator> {
    Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
    .operation("op")
    .unwrap()
    .input()
    .unwrap()
    .prepare(
        &DefaultEvaluator::with_limits(limits),
        &ExternalSchemas::default(),
    )
    .unwrap()
}

fn refused(outcome: ValueOutcome) {
    let ValueOutcome::NoVerdict { detail } = outcome else {
        panic!("expected a search refusal")
    };
    assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(detail.code, "pattern-search-work-limit");
    assert!(!format!("{detail:?}").contains("PRIVATE_PATTERN_VALUE"));
    assert!(!detail.message.contains("PRIVATE_PATTERN_VALUE"));
}

#[test]
fn hostile_searches_are_refused_by_default_and_run_with_the_ceiling_removed() {
    for pattern in ["^(?:(?:a{1,200}){1,200}|a)*$", "^(?:a{1,200}){1,200}$"] {
        let schema = json!({"pattern":pattern});
        let bounded = prepare(schema.clone(), Limits::default());
        for length in [2_000, 20_000, 50_000] {
            refused(
                bounded.evaluate(&JsonValue::string(&format!("{}b", "a".repeat(length))).unwrap()),
            );
        }
        let unbounded = prepare(
            schema,
            Limits {
                pattern_search_work_units: None,
                ..Limits::default()
            },
        );
        let value = JsonValue::string(&format!("{}b", "a".repeat(2_000))).unwrap();
        assert!(matches!(
            unbounded.evaluate(&value),
            ValueOutcome::Fails { .. }
        ));
    }
}

#[test]
fn search_ceiling_is_inclusive_per_search_and_can_be_raised() {
    // The normalized literal has six units per input byte plus the end position.
    let schema = json!({"items":{"pattern":"^abc$"}});
    let value = JsonValue::parse(r#"["abc","abc"]"#).unwrap();
    for cap in [
        None,
        Some(24),
        Some(25),
        Limits::default().pattern_search_work_units,
    ] {
        assert!(matches!(
            prepare(
                schema.clone(),
                Limits {
                    pattern_search_work_units: cap,
                    ..Limits::default()
                }
            )
            .evaluate(&value),
            ValueOutcome::Satisfies
        ));
    }
    refused(
        prepare(
            schema,
            Limits {
                pattern_search_work_units: Some(23),
                ..Limits::default()
            },
        )
        .evaluate(&value),
    );
    for schema in [
        json!({"propertyNames":{"pattern":"^abc$"}}),
        json!({"patternProperties":{"abc":true}}),
    ] {
        let prepared = prepare(
            schema,
            Limits {
                pattern_search_work_units: Some(0),
                ..Limits::default()
            },
        );
        let value = JsonValue::parse(r#"{"PRIVATE_PATTERN_VALUE":0}"#).unwrap();
        refused(prepared.evaluate(&value));
        assert!(matches!(
            prepared.evaluate(&JsonValue::parse("1").unwrap()),
            ValueOutcome::Satisfies
        ));
    }
}

#[test]
fn search_limit_serde_uses_the_optional_integer_contract() {
    assert_eq!(
        Limits::default().pattern_search_work_units,
        Some(250_000_000)
    );
    for value in [json!(null), json!(0), json!(u32::MAX)] {
        let parsed: Limits =
            serde_json::from_value(json!({"pattern_search_work_units":value})).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["pattern_search_work_units"],
            value
        );
    }
    for value in [json!(-1), json!(0.5), json!("1")] {
        assert!(
            serde_json::from_value::<Limits>(json!({"pattern_search_work_units":value})).is_err()
        );
    }
}

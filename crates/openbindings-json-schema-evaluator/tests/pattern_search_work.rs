use openbindings::{
    Document, ExternalSchema, ExternalSchemas, JsonValue, NoVerdictReason, ValueOutcome,
};
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

fn supplied(
    schema: serde_json::Value,
    limits: Limits,
) -> openbindings::PreparedSchema<DefaultValidator> {
    ExternalSchemas::new([ExternalSchema {
        uri: "https://patterns.test/root".into(),
        document: JsonValue::parse(schema.to_string()).unwrap(),
    }])
    .unwrap()
    .prepare(
        "https://patterns.test/root",
        &DefaultEvaluator::with_limits(limits),
    )
    .unwrap()
}

fn class(outcome: &ValueOutcome) -> (u8, &str) {
    match outcome {
        ValueOutcome::Satisfies => (0, ""),
        ValueOutcome::Fails { .. } => (1, ""),
        ValueOutcome::Undefined { detail } => (2, &detail.code),
        ValueOutcome::NoVerdict { detail } => (3, &detail.code),
    }
}

#[test]
fn ordinary_corpus_decides_at_defaults_on_both_routes_and_details() {
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/pattern-work-ordinary.json")).unwrap();
    let mut false_no_verdicts = 0;
    for row in corpus {
        let unit = row["unit"].as_str().unwrap();
        let length = row["bytes"].as_u64().unwrap() as usize;
        let mut text = unit.repeat(length.div_ceil(unit.len()));
        text.truncate(length);
        let value = JsonValue::string(&text).unwrap();
        let schema = json!({"type":"string", "pattern":row["pattern"]});
        assert_eq!(
            row["ceiling"],
            json!(Limits::default().pattern_search_work_units)
        );
        for p in [
            prepare(schema.clone(), Limits::default()),
            supplied(schema, Limits::default()),
        ] {
            let expected = if row["satisfies"] == true { 0 } else { 1 };
            let evaluated = p.evaluate(&value);
            let explained = p.explain(&value);
            for outcome in [&evaluated, &explained] {
                false_no_verdicts += usize::from(matches!(outcome, ValueOutcome::NoVerdict { .. }));
                assert_eq!(class(outcome).0, expected, "{}: {outcome:?}", row["name"]);
            }
            assert_eq!(class(&evaluated), class(&explained));
        }
    }
    assert_eq!(false_no_verdicts, 0);
}

#[test]
#[ignore = "release measurement of default admission at the largest JSON strings"]
fn ordinary_corpus_default_admission_boundaries() {
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/pattern-work-ordinary.json")).unwrap();
    for row in corpus {
        let p = supplied(
            json!({"type":"string", "pattern":row["pattern"]}),
            Limits::default(),
        );
        let length = row["largest_bytes"].as_u64().unwrap() as usize;
        let unit = row["unit"].as_str().unwrap();
        for n in [length, length + 1] {
            let mut text = unit.repeat(n.div_ceil(unit.len()));
            text.truncate(n);
            let value = JsonValue::string(&text);
            if n > 32 * 1024 * 1024 - 2 {
                assert!(value.is_err());
            } else {
                let outcome = p.evaluate(&value.unwrap());
                if n == length {
                    assert!(
                        matches!(
                            outcome,
                            ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                        ),
                        "{}",
                        row["name"]
                    );
                } else {
                    refused(outcome);
                }
            }
        }
    }
}

#[test]
fn hostile_corpus_refuses_before_an_anchored_search_exceeds_its_work_ceiling() {
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/pattern-work-hostile.json")).unwrap();
    for row in corpus {
        let text = format!(
            "{}b",
            "a".repeat(row["bytes"].as_u64().unwrap() as usize - 1)
        );
        let value = JsonValue::string(&text).unwrap();
        assert_eq!(
            row["ceiling"],
            json!(Limits::default().pattern_search_work_units)
        );
        let schema = json!({"pattern":row["pattern"]});
        for p in [
            prepare(schema.clone(), Limits::default()),
            supplied(schema, Limits::default()),
        ] {
            refused(p.evaluate(&value));
            refused(p.explain(&value));
        }
    }
}

#[test]
fn explanation_preserves_the_selected_verdict_at_each_search_allowance() {
    // Finite Original inspection now admits the review's default-limit witness.
    // The unbounded class also exercises an incomplete Original explanation
    // after the compiled lane has established failure.
    for pattern in ["^.{0,1000}$", r"^[\p{L}\p{N} ]+$"] {
        let value = JsonValue::string(&format!("{}!", "a".repeat(2_941_176))).unwrap();
        for cap in [
            None,
            Some(0),
            Some(23),
            Some(249_999_999),
            Some(250_000_000),
            Some(250_000_001),
        ] {
            let p = prepare(
                json!({"type":"string", "pattern":pattern}),
                Limits {
                    pattern_search_work_units: cap,
                    ..Limits::default()
                },
            );
            let evaluated = p.evaluate(&value);
            let explained = p.explain(&value);
            assert_eq!(class(&evaluated), class(&explained), "{pattern} {cap:?}");
            if cap == Limits::default().pattern_search_work_units && pattern.contains("p{L}") {
                assert!(
                    matches!(explained, ValueOutcome::Fails { problems, problems_complete: false, .. } if problems.is_empty())
                );
            }
        }
    }
}

#[test]
fn bounded_prefix_matches_use_the_complete_haystack_for_assertions() {
    for (pattern, text, matches) in [
        ("a", "ab", true),
        ("a$", "ab", false),
        (r"a\b", "ab", false),
        (r"a\b", "a!", true),
        ("é$", "éx", false),
        ("é", "éx", true),
    ] {
        let schema = json!({"pattern":pattern});
        let value = JsonValue::string(text).unwrap();
        // Find the least cap that can prove a prefix match through the public
        // API. A false boundary match must never become Satisfies at any cap.
        for cap in [0, 8, 32, 64, 128, 256, 512] {
            let p = supplied(
                schema.clone(),
                Limits {
                    pattern_search_work_units: Some(cap),
                    ..Limits::default()
                },
            );
            let outcome = p.evaluate(&value);
            if matches!(outcome, ValueOutcome::Satisfies) {
                assert!(matches);
            }
            if matches!(outcome, ValueOutcome::Fails { .. }) {
                assert!(!matches);
            }
        }
    }
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

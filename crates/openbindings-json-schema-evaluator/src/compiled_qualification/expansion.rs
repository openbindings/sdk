use super::*;
use std::time::Duration;

const FORMS: [&str; 5] = ["allOf", "anyOf", "oneOf", "not", "if"];

fn leaves() -> [(serde_json::Value, &'static str, &'static str); 3] {
    [
        (json!({"type":"object"}), "{}", "1"),
        (json!({"type":"integer","minimum":0}), "1", "-1"),
        (json!({"enum":["a","b"]}), "\"a\"", "\"c\""),
    ]
}

fn diamond(levels: usize, form: &str, leaf: serde_json::Value) -> String {
    let mut schemas = serde_json::Map::new();
    for i in 0..levels {
        let next = json!({"$ref":format!("#/schemas/d{}", i + 1)});
        let body = match form {
            // Both occurrences are evaluated even when the condition fails.
            "if" => json!({"if":next,"then":next,"else":next}),
            // Negation alone has one edge. Put a diamond beneath each not.
            "not" => json!({"not":{"allOf":[next,next]}}),
            _ => json!({form:[next,next]}),
        };
        schemas.insert(format!("d{i}"), body);
    }
    schemas.insert(format!("d{levels}"), leaf);
    json!({"openbindings":"0.2.0","schemas":schemas,
        "operations":{"op":{"input":{"$ref":"#/schemas/d0"}}}})
    .to_string()
}

fn expected(form: &str, levels: usize, leaf: bool) -> bool {
    match form {
        "oneOf" if levels > 0 => false,
        "not" => leaf != (levels % 2 == 1),
        _ => leaf,
    }
}

fn timely(work: impl FnOnce() + Send + 'static) {
    timely_with_deadline(work, Duration::from_secs(20));
}

fn timely_with_deadline(work: impl FnOnce() + Send + 'static, timeout: Duration) {
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        work();
        send.send(()).unwrap();
    });
    // Do not join on timeout: a removed guard must fail the test rather than
    // wait for an exponential synchronous evaluation to return.
    receive
        .recv_timeout(timeout)
        .expect("schema expansion did not finish within its deadline");
    worker.join().unwrap();
}

fn assert_verdict(validator: &DefaultValidator, value: &str, valid: bool) {
    let outcome = validator.evaluate(&JsonValue::parse(value).unwrap(), Detail::Outcome, &HEALTHY);
    assert!(
        matches!(
            (&outcome, valid),
            (ValueOutcome::Satisfies, true) | (ValueOutcome::Fails { .. }, false)
        ),
        "expected {valid}, got {outcome:?}"
    );
}

#[test]
fn original_diamonds_are_bounded_at_default_and_explicit_limits() {
    timely(|| {
        for work in [None, Some(1_000_000)] {
            for form in FORMS {
                for (leaf, yes, no) in leaves() {
                    for levels in [3, 4, 40, 64] {
                        let limits = Limits {
                            evaluation_sdk_work_units: work,
                            ..Limits::default()
                        };
                        let validator = prepare_document(
                            &diamond(levels, form, leaf.clone()),
                            &ExternalSchemas::default(),
                            &Original(limits),
                            &PreparationLimits::default(),
                        )
                        .unwrap();
                        assert_verdict(&validator, yes, expected(form, levels, true));
                        assert_verdict(&validator, no, expected(form, levels, false));
                    }
                }
            }
        }
    });
}

#[test]
fn default_diamonds_decline_before_construction_and_terminate() {
    timely(|| {
        for form in FORMS {
            for (leaf, yes, no) in leaves() {
                // The independent Boolean recurrence is also checked at small
                // odd/even depths, where both compiled and Original can run.
                for levels in [3, 4, 40, 64] {
                    compiled::entries::take();
                    let validator = prepare_document(
                        &diamond(levels, form, leaf.clone()),
                        &ExternalSchemas::default(),
                        &DefaultEvaluator::new(),
                        &PreparationLimits::default(),
                    )
                    .unwrap();
                    assert_verdict(&validator, yes, expected(form, levels, true));
                    assert_verdict(&validator, no, expected(form, levels, false));
                }
            }
        }
    });
}

// A compact graph with exactly `applications` expanded schema occurrences,
// including the operation input. One ref costs 1 + child; a duplicate pair
// costs 1 + 2 * (1 + child). This pins individual counts, not powers of two.
fn exact_expansion(applications: usize, leaf: serde_json::Value) -> String {
    fn add(
        count: usize,
        leaf: &serde_json::Value,
        schemas: &mut serde_json::Map<String, serde_json::Value>,
    ) -> String {
        let name = format!("n{count}");
        let body = if count == 1 {
            leaf.clone()
        } else {
            let double = count >= 5 && count % 2 == 1;
            let child = if double { (count - 3) / 2 } else { count - 1 };
            let next = json!({"$ref":format!("#/schemas/{}", add(child, leaf, schemas))});
            if double {
                json!({"allOf":[next,next]})
            } else {
                next
            }
        };
        schemas.insert(name.clone(), body);
        name
    }
    let mut schemas = serde_json::Map::new();
    let root = add(applications - 1, &leaf, &mut schemas);
    json!({"openbindings":"0.2.0","schemas":schemas,
        "operations":{"op":{"input":{"$ref":format!("#/schemas/{root}")}}}})
    .to_string()
}

#[test]
fn exact_expansion_boundary_and_lane_equivalence() {
    timely(|| {
        for applications in [99_999, 100_000, 100_001] {
            for (leaf, yes, no) in leaves() {
                let document = exact_expansion(applications, leaf);
                compiled::entries::take_expansion();
                let selected = prepare_document(
                    &document,
                    &ExternalSchemas::default(),
                    &DefaultEvaluator::new(),
                    &PreparationLimits::default(),
                )
                .unwrap();
                assert_eq!(compiled::entries::take_expansion(), Some(applications));
                let original = prepare_document(
                    &document,
                    &ExternalSchemas::default(),
                    &Original(Limits::default()),
                    &PreparationLimits::default(),
                )
                .unwrap();
                let lane = if applications <= 100_000 {
                    "compiled"
                } else {
                    "original"
                };
                if applications > 100_000 {
                    assert!(matches!(
                        selected.compiled,
                        compiled::Lane::Original("acyclic-finite-expansion-profile")
                    ));
                }
                for value in [yes, no] {
                    assert_eq!(
                        compare(&selected, &original, &JsonValue::parse(value).unwrap()).0,
                        lane
                    );
                }
            }
        }
    });
}

#[test]
fn wide_shallow_shared_definition_stays_compiled() {
    let properties: serde_json::Map<_, _> = (0..4096)
        .map(|i| (format!("p{i}"), json!({"$ref":"#/schemas/shared"})))
        .collect();
    let document = json!({"openbindings":"0.2.0","schemas":{"shared":{"type":"integer"}},
        "operations":{"op":{"input":{"properties":properties}}}})
    .to_string();
    compiled::entries::take_expansion();
    let selected = prepare_document(
        &document,
        &ExternalSchemas::default(),
        &DefaultEvaluator::new(),
        &PreparationLimits::default(),
    )
    .unwrap();
    let original = prepare_document(
        &document,
        &ExternalSchemas::default(),
        &Original(Limits::default()),
        &PreparationLimits::default(),
    )
    .unwrap();
    for value in ["{}", "{\"p0\":1,\"p4095\":2}", "{\"p4095\":false}"] {
        compare(&selected, &original, &JsonValue::parse(value).unwrap());
    }
}

#[test]
fn dependent_and_overlapping_child_paths_cannot_reset_expansion() {
    timely(|| {
        for advances in [false, true] {
            let mut schemas = serde_json::Map::new();
            for i in 0..40 {
                let next = json!({"$ref":format!("#/schemas/d{}", i + 1)});
                let body = if advances {
                    json!({"allOf":[{"properties":{"x":next}}, {"properties":{"x":next}}]})
                } else {
                    json!({"dependentSchemas":{"a":next,"b":next}})
                };
                schemas.insert(format!("d{i}"), body);
            }
            schemas.insert("d40".into(), json!({"type":"object"}));
            let document = json!({"openbindings":"0.2.0","schemas":schemas,
                "operations":{"op":{"input":{"$ref":"#/schemas/d0"}}}})
            .to_string();
            let selected = prepare_document(
                &document,
                &ExternalSchemas::default(),
                &DefaultEvaluator::new(),
                &PreparationLimits::default(),
            )
            .unwrap();
            let value = if advances {
                format!("{}{{}}{}", "{\"x\":".repeat(40), "}".repeat(40))
            } else {
                "{\"a\":0,\"b\":0}".into()
            };
            assert_verdict(&selected, &value, true);
        }
    });
}

fn array_diamond(leaf: serde_json::Value) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(&diamond(10, "allOf", leaf)).unwrap();
    let input = &mut document["operations"]["op"]["input"];
    *input = json!({"type":"array", "items":input.take()});
    document.to_string()
}

fn array_value(item: &str, count: usize) -> JsonValue {
    JsonValue::parse(format!("[{}]", vec![item; count].join(","))).unwrap()
}

fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Undefined { .. } => panic!("unexpected undefined: {outcome:?}"),
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        ValueOutcome::NoVerdict { detail } => {
            assert_eq!(detail.reason, NoVerdictReason::LimitExceeded, "{detail:?}");
            None
        }
    }
}

#[test]
fn product_boundary_preserves_cancellation_and_both_verdicts() {
    timely(|| {
        // The boundary test isolates the product decision with a cheap root
        // predicate. Advancing hostile work is exercised by the array diamonds.
        let source = exact_expansion(100, json!({"type":"array"}));
        compiled::entries::take_applications();
        let selected = prepare_document(
            &source,
            &ExternalSchemas::default(),
            &DefaultEvaluator::new(),
            &PreparationLimits::default(),
        )
        .unwrap();
        assert_eq!(compiled::entries::take_applications(), Some(100));
        let original = prepare_document(
            &source,
            &ExternalSchemas::default(),
            &Original(Limits::default()),
            &PreparationLimits::default(),
        )
        .unwrap();
        let nodes = compiled::MAXIMUM_EVALUATION_APPLICATIONS / 100;
        for count in [10, nodes - 1, nodes] {
            for valid in [true, false] {
                let value = if valid {
                    array_value("1", count)
                } else {
                    JsonValue::parse(format!(r#"{{"x":{}}}"#, array_value("1", count - 2).text()))
                        .unwrap()
                };
                assert_eq!(
                    openbindings_internal_json::backend::node_count(value.view()),
                    count + 1
                );
                compiled::entries::take();
                let actual = selected.evaluate(&value, Detail::Outcome, &HEALTHY);
                assert_eq!(compiled::entries::take()[2], usize::from(count < nodes));
                let expected = original.evaluate(&value, Detail::Outcome, &HEALTHY);
                if let Some(result) = verdict(&actual) {
                    assert_eq!(result, valid);
                }
                if let Some(result) = verdict(&expected) {
                    assert_eq!(result, valid);
                }
                if count < nodes {
                    assert_eq!(verdict(&actual), Some(valid));
                }
                if let (Some(a), Some(b)) = (verdict(&actual), verdict(&expected)) {
                    assert_eq!(a, b);
                }
                let cancelled = AtomicBool::new(true);
                assert!(
                    matches!(selected.evaluate(&value, Detail::Outcome, &cancelled),
                    ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
                );
            }
        }
    });
}

#[test]
fn product_default_large_diamonds_finish_without_backend_entry() {
    // Each case gets its own deadline. A removed check enters the uncancellable
    // backend and fails hard, including if it eventually returns a true verdict.
    // Debug builds run the Original fallback unoptimized, so they use a value
    // just over the ceiling for both leaves (4,095 applications times 20,001
    // nodes) and a smaller enum. Release builds, which tools/verify.py and the
    // package CI job run, check the measured sizes and the 1,024-choice enum.
    let (counts, choices): (&[usize], usize) = if cfg!(debug_assertions) {
        (&[20_000], 64)
    } else {
        (&[100_000, 500_000], 1024)
    };
    let last = format!("\"v{}\"", choices - 1);
    for enumeration in [false, true] {
        for &count in counts {
            let last = last.clone();
            timely_with_deadline(
                move || {
                    let leaf = if enumeration {
                        json!({"enum":(0..choices).map(|i| format!("v{i}")).collect::<Vec<_>>()})
                    } else {
                        json!({"type":"integer","minimum":0})
                    };
                    let source = array_diamond(leaf);
                    compiled::entries::take_applications();
                    let selected = prepare_document(
                        &source,
                        &ExternalSchemas::default(),
                        &DefaultEvaluator::new(),
                        &PreparationLimits::default(),
                    )
                    .unwrap();
                    let original = prepare_document(
                        &source,
                        &ExternalSchemas::default(),
                        &Original(Limits::default()),
                        &PreparationLimits::default(),
                    )
                    .unwrap();
                    let value = array_value(if enumeration { &last } else { "1" }, count);
                    compiled::entries::take();
                    let actual = selected.evaluate(&value, Detail::Outcome, &HEALTHY);
                    assert!(verdict(&actual).is_none_or(|v| v));
                    for (item, valid) in [
                        (if enumeration { last.as_str() } else { "1" }, true),
                        (if enumeration { "\"absent\"" } else { "-1" }, false),
                    ] {
                        let small = array_value(item, 2);
                        compare(&selected, &original, &small);
                        assert_verdict(&selected, small.text(), valid);
                    }
                },
                Duration::from_secs(90),
            );
        }
    }
}

#[test]
fn product_host_work_caps_stop_enum_fallback() {
    timely(|| {
        let source =
            array_diamond(json!({"enum":(0..1024).map(|i| format!("v{i}")).collect::<Vec<_>>()}));
        let value = array_value("\"v1023\"", 500_000);
        for cap in [1_000_000, 10_000_000] {
            let selected = prepare_document(
                &source,
                &ExternalSchemas::default(),
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_work_units: Some(cap),
                    ..Limits::default()
                }),
                &PreparationLimits::default(),
            )
            .unwrap();
            compiled::entries::take();
            let outcome = selected.evaluate(&value, Detail::Outcome, &HEALTHY);
            assert!(
                matches!(&outcome, ValueOutcome::NoVerdict { detail }
                if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "evaluation-work-limit"),
                "{outcome:?}"
            );
        }
    });
}

#[test]
fn product_disjoint_selectors_tighten_but_overlaps_and_recursive_literals_add() {
    let choices: Vec<_> = (0..64).map(|i| format!("v{i}")).collect();
    let recursive = json!({"properties":{"kind":{"enum":choices},"next":{"$ref":"#"}}});
    let (selected, original) = owners(
        &recursive.to_string(),
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    // Both productive entries retain the finite terminal enum contribution.
    for text in [
        r#"{"kind":"v0","next":{"kind":"v63"}}"#,
        r#"{"next":{"kind":"no"}}"#,
    ] {
        compare(&selected, &original, &JsonValue::parse(text).unwrap());
    }
    let value = JsonValue::parse(format!(
        r#"{{"padding":{}}}"#,
        array_value("0", 900_000).text()
    ))
    .unwrap();
    compare(&selected, &original, &value);
}

#[test]
fn product_enum_weight_selects_at_its_value_boundary() {
    let source =
        array_diamond(json!({"enum":(0..1024).map(|i| format!("v{i}")).collect::<Vec<_>>()}));
    let selected = prepare_document(
        &source,
        &ExternalSchemas::default(),
        &DefaultEvaluator::new(),
        &PreparationLimits::default(),
    )
    .unwrap();
    let original = prepare_document(
        &source,
        &ExternalSchemas::default(),
        &Original(Limits::default()),
        &PreparationLimits::default(),
    )
    .unwrap();
    let nodes = compiled::MAXIMUM_EVALUATION_APPLICATIONS / 1_052_671;
    for count in [nodes - 1, nodes] {
        assert_eq!(
            compare(&selected, &original, &array_value("\"v1023\"", count)).0,
            if count < nodes {
                "compiled"
            } else {
                "original"
            }
        );
    }
}

#[test]
fn product_real_source_puller_keeps_its_verdict_up_to_parser_node_limit() {
    timely(|| {
        // The corpus's largest tightened coefficient, with annotations removed.
        // At 10 million, a populated report fell back to a memory NoVerdict.
        let source = include_str!("../../tests/fixtures/product-source-puller.json");
        compiled::entries::take_applications();
        let selected = prepare_document(
            source,
            &ExternalSchemas::default(),
            &DefaultEvaluator::new(),
            &PreparationLimits::default(),
        )
        .unwrap();
        let mut value = json!({"additions":{},"report":{"exhaustive":true,"added":[],
            "existing":vec![json!({"binding":"x","status":"current"});190_000]},"padding":[]});
        let base = JsonValue::parse(value.to_string()).unwrap();
        let count = 1_000_000 - openbindings_internal_json::backend::node_count(base.view());
        value["padding"] = json!(vec![0; count]);
        let value = JsonValue::parse(value.to_string()).unwrap();
        assert_eq!(
            openbindings_internal_json::backend::node_count(value.view()),
            1_000_000
        );
        compiled::entries::take();
        assert_verdict(&selected, value.text(), true);
    });
}

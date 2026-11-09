use super::*;
use std::sync::atomic::AtomicBool;

const ROOT: &str = "https://explain.invalid/root";
fn prepare(schema: JsonValue, extras: Option<JsonRef<'_>>) -> (Plan, ExternalSchemas) {
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{ROOT}"}}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let mut schemas = vec![ExternalSchema {
        uri: ROOT.into(),
        document: schema,
    }];
    if let Some(extras) = extras {
        for member in extras.members().unwrap() {
            schemas.push(ExternalSchema {
                uri: member.name.as_str().unwrap().into(),
                document: JsonValue::parse(member.value.text()).unwrap(),
            });
        }
    }
    let schemas = ExternalSchemas::new(schemas).unwrap();
    let preparation = PreparationLimits::default();
    let control = AtomicBool::new(false);
    let environment = document
        .operation("op")
        .unwrap()
        .input()
        .unwrap()
        .environment(&schemas, &preparation, &control)
        .unwrap();
    let plan = Plan::new(environment, Limits::default(), preparation, &|| false).unwrap();
    (plan, schemas)
}

fn category(outcome: &ValueOutcome) -> &'static str {
    match outcome {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}

#[test]
fn required_property_and_additional_property_failures_keep_original_coordinates() {
    let (plan, _) = prepare(JsonValue::parse(r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a","b","c"],"additionalProperties":false}"#).unwrap(), None);
    let value = JsonValue::parse(r#"{"a":1,"b":"x","z":true}"#).unwrap();
    let result = plan.evaluate(&value, Detail::Explain, &AtomicBool::new(false));
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = result
    else {
        panic!("expected failure")
    };
    assert!(problems_complete);
    assert_eq!(
        problems
            .iter()
            .map(|p| (
                p.instance.as_str().unwrap(),
                p.schema.pointer.as_str().unwrap(),
                p.code.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("", "/required", "required"),
            ("/a", "/properties/a/type", "type"),
            ("/b", "/properties/b/type", "type"),
            ("/z", "/additionalProperties", "falseSchema"),
        ]
    );
}

fn explain(schema: &str, value: &str) -> (Vec<ValueProblem>, bool) {
    let (plan, _) = prepare(JsonValue::parse(schema).unwrap(), None);
    let value = JsonValue::parse(value).unwrap();
    let ordinary = plan.evaluate(&value, Detail::Outcome, &AtomicBool::new(false));
    let explained = plan.evaluate(&value, Detail::Explain, &AtomicBool::new(false));
    assert_eq!(category(&ordinary), category(&explained));
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = explained
    else {
        panic!("expected failure")
    };
    (problems, problems_complete)
}

#[test]
fn multiple_local_assertions_and_missing_members_are_all_retained() {
    let (problems, complete) = explain(
        r#"{"minimum":10,"maximum":0,"multipleOf":2,"enum":[42],"const":6}"#,
        "5",
    );
    assert!(complete);
    assert_eq!(
        problems.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(),
        ["const", "enum", "maximum", "minimum", "multipleOf"]
    );
    let (problems, complete) = explain(
        r#"{"required":["title","a","b"],"dependentRequired":{"title":["c","d"]}}"#,
        r#"{"title":"present"}"#,
    );
    assert!(complete);
    assert_eq!(
        problems
            .iter()
            .map(|p| p.schema.pointer.as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "/dependentRequired",
            "/dependentRequired",
            "/required",
            "/required"
        ]
    );
    assert_eq!(
        problems
            .iter()
            .map(|p| p.details.as_ref())
            .collect::<Vec<_>>(),
        ["c", "d", "a", "b"]
            .map(|member| Some(ValueProblemDetails::Required {
                member: member.into()
            }))
            .iter()
            .map(Option::as_ref)
            .collect::<Vec<_>>()
    );
}

#[test]
fn facts_reserve_exact_backing_before_copying_and_never_return_partial_lists() {
    let control = AtomicBool::new(false);
    for (schema, keyword, bytes) in [
        (r#"{"required":["é"]}"#, "required", 2),
        (
            r#"{"type":["null","integer"]}"#,
            "type",
            2 * size_of::<String>() + 11,
        ),
        (r#"{"minimum":1E+400}"#, "minimum", 6),
        (r#"{"minItems":2.00e+0}"#, "minItems", 7),
        (
            r#"{"enum":[1E+400,"é"]}"#,
            "enum",
            2 * size_of::<String>() + 10,
        ),
    ] {
        let source = JsonValue::parse(schema).unwrap();
        let operand = source.get(keyword).unwrap();
        let member = (keyword == "required").then(|| operand.element(0).unwrap());
        let mut graph = Graph::new(bytes - 1, None, 0, &control);
        let (facts, complete) = schema_facts(&mut graph, keyword, operand, member).unwrap();
        assert!(facts.is_none(), "{schema}");
        assert!(!complete);
        assert_eq!(graph.remaining().bytes, bytes - 1, "refusal copies nothing");
        let mut graph = Graph::new(bytes, None, 0, &control);
        let (facts, complete) = schema_facts(&mut graph, keyword, operand, member).unwrap();
        assert!(facts.is_some(), "{schema}");
        assert!(complete);
        assert_eq!(graph.remaining().bytes, 0);
    }
}

#[test]
fn nested_occurrences_sort_by_instance_then_original_schema() {
    let schema = r#"{"properties":{"rows":{"items":{"properties":{"~/":{"allOf":[{"minimum":5},{"const":8}]}}}}}}"#;
    let (problems, complete) = explain(schema, r#"{"rows":[{"~/":1},{"~/":2}]}"#);
    assert!(complete);
    let coordinates = problems
        .iter()
        .map(|p| {
            (
                p.instance.as_str().unwrap(),
                p.schema.pointer.as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        coordinates,
        [
            (
                "/rows/0/~0~1",
                "/properties/rows/items/properties/~0~1/allOf/0/minimum"
            ),
            (
                "/rows/0/~0~1",
                "/properties/rows/items/properties/~0~1/allOf/1/const"
            ),
            (
                "/rows/1/~0~1",
                "/properties/rows/items/properties/~0~1/allOf/0/minimum"
            ),
            (
                "/rows/1/~0~1",
                "/properties/rows/items/properties/~0~1/allOf/1/const"
            ),
        ]
    );
    let schema = r#"{"additionalProperties":{"allOf":[{"type":"string"},{"const":null}]}}"#;
    let (expected, _) = explain(schema, r#"{"z":1,"a":2}"#);
    for _ in 0..5 {
        assert_eq!(explain(schema, r#"{"a":2,"z":1}"#).0, expected);
    }
}

#[test]
fn applicators_report_only_the_failures_that_apply() {
    for (schema, value, expected) in [
        (
            r#"{"allOf":[{"type":"string"},false]}"#,
            "1",
            vec![
                ("", "/allOf/0/type", "type"),
                ("", "/allOf/1", "falseSchema"),
            ],
        ),
        (
            r#"{"anyOf":[{"type":"string"},{"type":"boolean"}]}"#,
            "1",
            vec![("", "/anyOf", "anyOf")],
        ),
        (
            r#"{"oneOf":[{"type":"string"},false]}"#,
            "1",
            vec![("", "/oneOf", "oneOf")],
        ),
        (
            r#"{"oneOf":[true,true,false]}"#,
            "1",
            vec![("", "/oneOf", "oneOf")],
        ),
        (
            r#"{"not":{"type":"integer"}}"#,
            "1",
            vec![("", "/not", "not")],
        ),
        (
            r#"{"anyOf":[false,true],"not":false,"const":2}"#,
            "1",
            vec![("", "/const", "const")],
        ),
        (
            r#"{"if":true,"then":{"minimum":2,"const":3},"else":false}"#,
            "1",
            vec![
                ("", "/then/const", "const"),
                ("", "/then/minimum", "minimum"),
            ],
        ),
        (
            r#"{"if":false,"then":false,"else":{"minimum":2,"const":3}}"#,
            "1",
            vec![
                ("", "/else/const", "const"),
                ("", "/else/minimum", "minimum"),
            ],
        ),
        (
            r#"{"contains":{"type":"string"},"minContains":2}"#,
            "[1]",
            vec![("", "/contains", "contains")],
        ),
        (
            r#"{"propertyNames":{"pattern":"^x$"}}"#,
            r#"{"y":1,"z":2}"#,
            vec![("", "/propertyNames", "propertyNames")],
        ),
        (
            r#"{"properties":{"a":true},"additionalProperties":false}"#,
            r#"{"z":1,"a":2,"b":3}"#,
            vec![
                ("/b", "/additionalProperties", "falseSchema"),
                ("/z", "/additionalProperties", "falseSchema"),
            ],
        ),
        (
            r#"{"allOf":[{"properties":{"a":true}}],"unevaluatedProperties":false}"#,
            r#"{"z":1,"a":2,"b":3}"#,
            vec![
                ("/b", "/unevaluatedProperties", "falseSchema"),
                ("/z", "/unevaluatedProperties", "falseSchema"),
            ],
        ),
        (
            r#"{"prefixItems":[true],"unevaluatedItems":false}"#,
            "[1,2,3]",
            vec![
                ("/1", "/unevaluatedItems", "falseSchema"),
                ("/2", "/unevaluatedItems", "falseSchema"),
            ],
        ),
    ] {
        let (problems, complete) = explain(schema, value);
        assert!(complete, "{schema}");
        assert_eq!(
            problems
                .iter()
                .map(|p| (
                    p.instance.as_str().unwrap(),
                    p.schema.pointer.as_str().unwrap(),
                    p.code.as_str()
                ))
                .collect::<Vec<_>>(),
            expected,
            "{schema}"
        );
    }
}

#[test]
fn problem_limit_distinguishes_exactly_full_from_truncated() {
    for count in [
        MAX_EXPLANATION_PROBLEMS - 1,
        MAX_EXPLANATION_PROBLEMS,
        MAX_EXPLANATION_PROBLEMS + 1,
    ] {
        let required = (0..count).map(|n| format!("field{n}")).collect::<Vec<_>>();
        let (problems, complete) =
            explain(&serde_json::json!({"required":required}).to_string(), "{}");
        assert_eq!(problems.len(), count.min(MAX_EXPLANATION_PROBLEMS));
        assert_eq!(complete, count <= MAX_EXPLANATION_PROBLEMS);
        let (problems, complete) = explain(
            r#"{"items":{"type":"string"}}"#,
            &serde_json::to_string(&vec![1; count]).unwrap(),
        );
        assert_eq!(problems.len(), count.min(MAX_EXPLANATION_PROBLEMS));
        assert_eq!(complete, count <= MAX_EXPLANATION_PROBLEMS);
        assert!(problems.windows(2).all(|p| {
            p[0].instance
                .utf16_units()
                .cmp(p[1].instance.utf16_units())
                .is_le()
        }));
    }
}

#[test]
fn unresolved_assertions_or_guards_never_claim_completeness() {
    for schema in [
        r#"{"required":["missing"],"$ref":"https://example.test/missing"}"#,
        r#"{"required":["missing"],"patternProperties":{"(?i)x":false},"additionalProperties":false}"#,
        r#"{"required":["missing"],"if":{"$ref":"https://example.test/missing"},"then":false,"else":false}"#,
    ] {
        let (problems, complete) = explain(schema, r#"{"a":1}"#);
        assert!(!complete, "{schema}");
        assert_eq!(problems.len(), 1, "{schema}");
        assert_eq!(problems[0].code, "required");
    }
}

#[test]
fn required_format_assertion_is_reported_alongside_pattern() {
    let extras = JsonValue::parse(r#"{"https://explain.invalid/format-meta":{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/validation":true,"https://json-schema.org/draft/2020-12/vocab/format-assertion":true},"type":["object","boolean"]}}"#).unwrap();
    let (plan, _) = prepare(
        JsonValue::parse(
            r#"{"$schema":"https://explain.invalid/format-meta","format":"email","pattern":"^z"}"#,
        )
        .unwrap(),
        Some(extras.view()),
    );
    let result = plan.evaluate(
        &JsonValue::parse(r#""bad""#).unwrap(),
        Detail::Explain,
        &AtomicBool::new(false),
    );
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = result
    else {
        panic!("expected failure")
    };
    assert!(problems_complete);
    assert_eq!(
        problems.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(),
        ["format", "pattern"]
    );
}

#[test]
fn repeated_references_share_evidence_without_duplicating_problems() {
    let (problems, complete) = explain(
        r##"{"$id":"https://explain.invalid/root","$defs":{"bad":{"minimum":5,"const":6}},"allOf":[{"$ref":"#/$defs/bad"},{"$ref":"#/$defs/bad"}]}"##,
        "1",
    );
    assert!(complete);
    assert_eq!(
        problems
            .iter()
            .map(|p| p.schema.pointer.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["/$defs/bad/const", "/$defs/bad/minimum"]
    );
}

#[test]
fn independent_original_explanation_controls_preserve_proof_and_exact_locations() {
    // The control file itself contains exact lone-unit pointer expectations.
    let controls = JsonValue::parse(include_str!("controls.json")).unwrap();
    let cases = controls.get("cases").unwrap();
    let mut count = 0;
    for case in cases.elements().unwrap() {
        count += 1;
        let id = case.get("id").unwrap().as_str().unwrap();
        let schema = case
            .get("schema_text")
            .map(|v| JsonValue::parse(v.as_str().unwrap()).unwrap())
            .unwrap_or_else(|| JsonValue::parse(case.get("schema").unwrap().text()).unwrap());
        let (plan, resources) = prepare(schema, case.get("resources"));
        let mut value = case
            .get("value_text")
            .map(|v| JsonValue::parse(v.as_str().unwrap()).unwrap())
            .unwrap_or_else(|| case.get("value").unwrap().to_owned());
        if let Some(pointer) = case.get("evaluate_subtree") {
            value = value.at(pointer.as_str().unwrap()).unwrap().to_owned();
        }
        let control = AtomicBool::new(false);
        let ordinary = plan.evaluate(&value, Detail::Outcome, &control);
        let explained = plan.evaluate(&value, Detail::Explain, &control);
        let required = case.get("required_outcome").unwrap().as_str().unwrap();
        assert_eq!(category(&ordinary), required, "{id}: {ordinary:?}");
        assert_eq!(category(&explained), required, "{id}: {explained:?}");
        if required == "fails" {
            let first =
                plan.evaluate_pass(&value, &control, None, &mut None, plan.allowance(), false);
            let second =
                plan.evaluate_pass(&value, &control, None, &mut None, first.remaining, true);
            assert_eq!(
                category(&second.outcome),
                "fails",
                "{id}: healthy diagnostic pass diverged"
            );
        }
        if let ValueOutcome::Fails { problems, .. } = ordinary {
            assert!(problems.is_empty(), "{id}");
        }
        let must_report = case
            .get("must_report_problem_when_funded")
            .unwrap()
            .as_bool()
            .unwrap();
        if let ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } = explained
        {
            assert_eq!(
                problems_complete,
                case.get("problems_complete").unwrap().as_bool().unwrap(),
                "{id}"
            );
            assert_eq!(problems.len(), usize::from(must_report), "{id}");
            if must_report {
                assert_eq!(problems.len(), 1, "{id}");
            }
            for problem in problems {
                for forbidden in case.get("forbidden_keywords").unwrap().elements().unwrap() {
                    assert_ne!(problem.code, forbidden.as_str().unwrap(), "{id}");
                }
                if let Some(keyword) = case.get("keyword") {
                    let keyword = keyword.as_str().unwrap();
                    assert_eq!(
                        problem.code,
                        if keyword == "$false" {
                            "falseSchema"
                        } else {
                            keyword
                        },
                        "{id}"
                    );
                }
                if let Some(expected) = case.get("instance_pointer") {
                    assert!(
                        problem
                            .instance
                            .utf16_units()
                            .eq(expected.as_string().unwrap().utf16_units()),
                        "{id}: instance pointer"
                    );
                }
                if let Some(expected) = case.get("schema_pointer") {
                    assert!(
                        problem
                            .schema
                            .pointer
                            .utf16_units()
                            .eq(expected.as_string().unwrap().utf16_units()),
                        "{id}: schema pointer"
                    );
                }
                let source_uri = case
                    .get("schema_source_uri")
                    .map(|v| v.as_str().unwrap())
                    .unwrap_or(ROOT);
                assert_eq!(
                    problem.schema.source_uri.as_deref(),
                    Some(source_uri),
                    "{id}"
                );
                assert!(
                    value.at_pointer(&problem.instance).is_some(),
                    "{id}: instance provenance"
                );
                assert!(
                    resources.locate(&problem.schema).is_some(),
                    "{id}: schema provenance"
                );
                assert_eq!(problem.message, "value fails this schema assertion");
            }
        } else {
            assert!(!must_report, "{id}");
        }
    }
    assert_eq!(count, 42);
}

fn has_problem(outcome: &ValueOutcome) -> bool {
    matches!(outcome, ValueOutcome::Fails { problems, .. } if problems.len() == 1)
}

#[test]
fn two_pass_storage_and_work_are_cumulative_at_exact_boundaries() {
    let (mut plan, _) = prepare(
        JsonValue::parse(r#"{"properties":{"a":{"type":"integer"}}}"#).unwrap(),
        None,
    );
    let value = JsonValue::parse(r#"{"a":"x"}"#).unwrap();
    let control = AtomicBool::new(false);
    plan.limits.evaluation_sdk_work_units = Some(1_000_000);
    let allowance = plan.allowance();
    let first = plan.evaluate_pass(&value, &control, None, &mut None, allowance, false);
    let first_bytes = allowance.bytes - first.remaining.bytes;
    let second = plan.evaluate_pass(&value, &control, None, &mut None, first.remaining, true);
    assert!(has_problem(&second.outcome));
    let bytes = allowance.bytes - second.remaining.bytes;
    let work = allowance.work.unwrap() - second.remaining.work.unwrap();
    assert!(bytes > first_bytes);

    plan.limits.evaluation_sdk_reservation_bytes = bytes;
    plan.limits.evaluation_sdk_work_units = Some(work);
    assert!(has_problem(&plan.evaluate(
        &value,
        Detail::Explain,
        &control
    )));
    plan.limits.evaluation_sdk_reservation_bytes = bytes - 1;
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));
    plan.limits.evaluation_sdk_reservation_bytes = bytes;
    plan.limits.evaluation_sdk_work_units = Some(work - 1);
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));

    plan.limits.evaluation_sdk_work_units = allowance.work;
    plan.limits.evaluation_sdk_reservation_bytes = first_bytes;
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));
    plan.limits.evaluation_sdk_reservation_bytes = first_bytes - 1;
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Explain, &control)),
        "no-verdict"
    );
    plan.limits.evaluation_sdk_reservation_bytes = allowance.bytes;
    assert!(has_problem(&plan.evaluate(
        &value,
        Detail::Explain,
        &control
    )));
}

#[test]
fn every_cancellation_boundary_distinguishes_before_and_after_the_verdict() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfter {
        after: usize,
        checks: AtomicUsize,
    }
    impl WorkControl for CancelAfter {
        fn is_cancelled(&self) -> bool {
            self.checks.fetch_add(1, Ordering::Relaxed) >= self.after
        }
    }
    let (plan, _) = prepare(
        JsonValue::parse(r#"{"properties":{"a":{"type":"integer"}}}"#).unwrap(),
        None,
    );
    let value = JsonValue::parse(r#"{"a":"x"}"#).unwrap();
    let count = CancelAfter {
        after: usize::MAX,
        checks: AtomicUsize::new(0),
    };
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Outcome, &count)),
        "fails"
    );
    let verdict_checks = count.checks.swap(0, Ordering::Relaxed);
    assert!(has_problem(&plan.evaluate(&value, Detail::Explain, &count)));
    let all_checks = count.checks.load(Ordering::Relaxed);
    assert!(all_checks > verdict_checks);
    for after in 0..all_checks {
        let control = CancelAfter {
            after,
            checks: AtomicUsize::new(0),
        };
        let result = plan.evaluate(&value, Detail::Explain, &control);
        assert_eq!(
            category(&result),
            if after < verdict_checks {
                "no-verdict"
            } else {
                "fails"
            },
            "checkpoint {after}/{all_checks}, verdict {verdict_checks}"
        );
        assert!(!has_problem(&result), "checkpoint {after}");
        assert!(has_problem(&plan.evaluate(
            &value,
            Detail::Explain,
            &AtomicBool::new(false)
        )));
    }
}

#[test]
fn enclosing_attempt_exhaustion_is_not_hidden_by_the_established_verdict() {
    use openbindings_internal_schema::preparation::Attempt;
    let (plan, _) = prepare(JsonValue::parse(r#"{"type":"integer"}"#).unwrap(), None);
    let value = JsonValue::parse(r#""x""#).unwrap();
    let control = AtomicBool::new(false);
    let run = |detail, work, bytes| {
        Attempt::run(work, bytes, &|| false, |attempt| {
            let outcome = plan.evaluate_with_preparation(&value, detail, &control, Some(attempt));
            (outcome, attempt.allocated())
        })
    };
    let (_, first_bytes) = run(Detail::Outcome, usize::MAX, usize::MAX).unwrap();
    let (explained, all_bytes) = run(Detail::Explain, usize::MAX, usize::MAX).unwrap();
    assert!(has_problem(&explained));
    assert!(all_bytes > first_bytes);
    assert!(has_problem(
        &run(Detail::Explain, usize::MAX, all_bytes).unwrap().0
    ));
    for bytes in [first_bytes, all_bytes - 1] {
        let refusal = Attempt::run(usize::MAX, bytes, &|| false, |attempt| {
            let outcome =
                plan.evaluate_with_preparation(&value, Detail::Explain, &control, Some(attempt));
            assert_eq!(category(&outcome), "fails");
            assert!(!has_problem(&outcome));
            // Ignoring the diagnostic refusal cannot hide enclosing exhaustion.
            true
        })
        .unwrap_err();
        assert_eq!(refusal.code, "preparation-sdk-reservation-bytes");
    }

    // Locate the enclosing work boundary itself, including backend work and
    // Attempt's checks, instead of assuming it equals the Graph's work count.
    let minimum = |detail| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(detail, high, usize::MAX).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if run(detail, middle, usize::MAX).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let first_work = minimum(Detail::Outcome);
    let all_work = minimum(Detail::Explain);
    assert!(all_work > first_work);
    assert!(has_problem(
        &run(Detail::Explain, all_work, usize::MAX).unwrap().0
    ));
    for work in [first_work, all_work - 1] {
        let refusal = Attempt::run(work, usize::MAX, &|| false, |attempt| {
            let outcome =
                plan.evaluate_with_preparation(&value, Detail::Explain, &control, Some(attempt));
            assert_eq!(category(&outcome), "fails");
            assert!(!has_problem(&outcome));
        })
        .unwrap_err();
        assert_eq!(refusal.code, "preparation-sdk-work-units");
    }
    assert!(has_problem(
        &run(Detail::Explain, all_work, all_bytes).unwrap().0
    ));
}

#[test]
fn a_failed_primitive_does_not_skip_later_work_or_its_refusal() {
    let (mut plain, _) = prepare(JsonValue::parse(r#"{"type":"null"}"#).unwrap(), None);
    let (mut more, _) = prepare(
        JsonValue::parse(r#"{"type":"null","minLength":1}"#).unwrap(),
        None,
    );
    let value = JsonValue::parse(format!("\"{}\"", "x".repeat(2048))).unwrap();
    let control = AtomicBool::new(false);
    plain.limits.evaluation_sdk_work_units = Some(1_000_000);
    let first = plain.evaluate_pass(&value, &control, None, &mut None, plain.allowance(), false);
    assert_eq!(category(&first.outcome), "fails");
    more.limits.evaluation_sdk_work_units = Some(1_000_000 - first.remaining.work.unwrap());
    for detail in [Detail::Outcome, Detail::Explain] {
        assert_eq!(
            category(&more.evaluate(&value, detail, &control)),
            "no-verdict"
        );
    }
    more.limits.evaluation_sdk_work_units = None;
    let ValueOutcome::Fails { problems, .. } = more.evaluate(&value, Detail::Explain, &control)
    else {
        panic!("the fully funded assertion fails");
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "type");
}

#[test]
fn zero_unlimited_and_overflow_allowances_do_not_renew_work() {
    let (mut plan, _) = prepare(JsonValue::parse("false").unwrap(), None);
    let value = JsonValue::parse("null").unwrap();
    let control = AtomicBool::new(false);
    for work in [None, Some(usize::MAX)] {
        plan.limits.evaluation_sdk_work_units = work;
        assert!(has_problem(&plan.evaluate(
            &value,
            Detail::Explain,
            &control
        )));
    }
    plan.limits.evaluation_sdk_work_units = Some(0);
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Explain, &control)),
        "no-verdict"
    );
    plan.limits.evaluation_sdk_work_units = None;
    plan.limits.evaluation_sdk_reservation_bytes = 0;
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Explain, &control)),
        "no-verdict"
    );

    let mut graph = Graph::new(usize::MAX, Some(usize::MAX), 0, &control);
    graph.charge(usize::MAX).unwrap();
    assert!(graph.charge(1).is_err());
    assert_eq!(graph.remaining().bytes, 0);
    graph.work(usize::MAX).unwrap();
    assert!(graph.work(1).is_err());
    assert_eq!(graph.remaining().work, Some(0));
}

#[test]
fn concurrent_calls_keep_exact_occurrences_and_independent_controls() {
    let (plan, _) = prepare(
        JsonValue::parse(r#"{"properties":{"a":{"type":"integer"},"b":{"type":"integer"}}}"#)
            .unwrap(),
        None,
    );
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let plan = &plan;
            scope.spawn(move || {
                for step in 0..25 {
                    let (text, expected) = if (worker + step) % 2 == 0 {
                        (r#"{"a":"same","b":1}"#, "/a")
                    } else {
                        (r#"{"a":1,"b":"same"}"#, "/b")
                    };
                    let value = JsonValue::parse(text).unwrap();
                    assert_eq!(
                        category(&plan.evaluate(&value, Detail::Explain, &AtomicBool::new(true))),
                        "no-verdict"
                    );
                    let ValueOutcome::Fails { problems, .. } =
                        plan.evaluate(&value, Detail::Explain, &AtomicBool::new(false))
                    else {
                        panic!("a healthy concurrent call fails");
                    };
                    assert_eq!(problems.len(), 1);
                    assert_eq!(problems[0].instance.as_str(), Some(expected));
                    assert!(value.at_pointer(&problems[0].instance).is_some());
                }
            });
        }
    });
}

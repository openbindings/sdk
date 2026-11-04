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
            assert!(!problems_complete, "{id}");
            assert!(problems.len() <= 1, "{id}");
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
    plan.limits.evaluation_steps = Some(1_000_000);
    let allowance = plan.allowance();
    let first = plan.evaluate_pass(&value, &control, None, &mut None, allowance, false);
    let first_bytes = allowance.bytes - first.remaining.bytes;
    let second = plan.evaluate_pass(&value, &control, None, &mut None, first.remaining, true);
    assert!(has_problem(&second.outcome));
    let bytes = allowance.bytes - second.remaining.bytes;
    let work = allowance.work.unwrap() - second.remaining.work.unwrap();
    assert!(bytes > first_bytes);

    plan.limits.program_bytes = bytes;
    plan.limits.evaluation_steps = Some(work);
    assert!(has_problem(&plan.evaluate(
        &value,
        Detail::Explain,
        &control
    )));
    plan.limits.program_bytes = bytes - 1;
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));
    plan.limits.program_bytes = bytes;
    plan.limits.evaluation_steps = Some(work - 1);
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));

    plan.limits.evaluation_steps = allowance.work;
    plan.limits.program_bytes = first_bytes;
    let result = plan.evaluate(&value, Detail::Explain, &control);
    assert_eq!(category(&result), "fails");
    assert!(!has_problem(&result));
    plan.limits.program_bytes = first_bytes - 1;
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Explain, &control)),
        "no-verdict"
    );
    plan.limits.program_bytes = allowance.bytes;
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
        assert_eq!(refusal.code, "preparation-allocation-bytes");
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
        assert_eq!(refusal.code, "preparation-work-units");
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
    plain.limits.evaluation_steps = Some(1_000_000);
    let first = plain.evaluate_pass(&value, &control, None, &mut None, plain.allowance(), false);
    assert_eq!(category(&first.outcome), "fails");
    more.limits.evaluation_steps = Some(1_000_000 - first.remaining.work.unwrap());
    for detail in [Detail::Outcome, Detail::Explain] {
        assert_eq!(
            category(&more.evaluate(&value, detail, &control)),
            "no-verdict"
        );
    }
    more.limits.evaluation_steps = None;
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
        plan.limits.evaluation_steps = work;
        assert!(has_problem(&plan.evaluate(
            &value,
            Detail::Explain,
            &control
        )));
    }
    plan.limits.evaluation_steps = Some(0);
    assert_eq!(
        category(&plan.evaluate(&value, Detail::Explain, &control)),
        "no-verdict"
    );
    plan.limits.evaluation_steps = None;
    plan.limits.program_bytes = 0;
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

#[test]
fn sdk_control_refusals_keep_static_text_through_mapping_and_cloning() {
    use openbindings_internal_schema::preparation::Attempt;
    use std::borrow::Cow;
    let healthy = AtomicBool::new(false);
    for (work, bytes) in [(0, usize::MAX), (usize::MAX, 0)] {
        let failure = Attempt::run(work, bytes, &|| false, |attempt| {
            let mut graph = Graph::with_preparation(usize::MAX, None, 1, &healthy, Some(attempt));
            let detail = if work == 0 {
                graph.work(1)
            } else {
                graph.charge(1)
            }
            .unwrap_err();
            assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
            assert!(matches!(detail.code, Cow::Borrowed(_)));
            assert!(matches!(detail.message, Cow::Borrowed(_)));
            let clone = detail.clone();
            assert!(matches!(clone.code, Cow::Borrowed(_)));
            assert!(matches!(clone.message, Cow::Borrowed(_)));
        })
        .unwrap_err();
        assert!(matches!(failure.code, Cow::Borrowed(_)));
        assert!(matches!(failure.message, Cow::Borrowed(_)));
    }
    let cancelled = AtomicBool::new(true);
    let detail = Graph::new(usize::MAX, None, 1, &cancelled)
        .work(1)
        .unwrap_err();
    assert_eq!(detail.reason, NoVerdictReason::Cancelled);
    assert!(matches!(detail.code, Cow::Borrowed("caller-cancelled")));
    assert!(matches!(detail.message, Cow::Borrowed(_)));
    let failure = Attempt::run(usize::MAX, usize::MAX, &cancelled, |_| ()).unwrap_err();
    let detail = preparation_control_inability(failure);
    assert_eq!(detail.reason, NoVerdictReason::Cancelled);
    assert!(matches!(detail.code, Cow::Borrowed("caller-cancelled")));
    assert!(matches!(detail.message, Cow::Borrowed(_)));
}

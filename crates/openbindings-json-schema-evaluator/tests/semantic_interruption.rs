//! A cached semantic predicate belongs to the schema; cancellation belongs to one call.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct CancelAfter {
    calls: AtomicUsize,
    allowance: usize,
}
impl WorkControl for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) >= self.allowance
    }
}
fn schema() -> Schema {
    Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$id":"https://example.test/semantic","type":"string","anyOf":[true,{"$ref":"%"}]}}}}"#)
        .unwrap().validate().unwrap().conformant().unwrap()
        .resolve_operation("op").unwrap().input().unwrap()
}
#[test]
fn interruption_does_not_poison_a_prepared_predicate() {
    let value = JsonValue::string("x").unwrap();
    let mut cancelled = 0;
    let mut completed = 0;
    for allowance in 0..100 {
        let prepared = schema()
            .prepare(
                &DefaultEvaluator::new(),
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
            )
            .unwrap();
        let control = CancelAfter {
            calls: AtomicUsize::new(0),
            allowance,
        };
        match prepared.evaluate_with_control(&value, &control) {
            ValueOutcome::NoVerdict { detail } => {
                assert_eq!(detail.reason, NoVerdictReason::Cancelled);
                cancelled += 1;
            }
            ValueOutcome::Satisfies => completed += 1,
            other => panic!("unexpected first attempt: {other:?}"),
        }
        assert!(
            matches!(prepared.evaluate(&value), ValueOutcome::Satisfies),
            "retry after {allowance} checks"
        );
    }
    assert!(
        cancelled > 0 && completed > 0,
        "exercise both interruption and completion"
    );
}
#[test]
fn direct_validator_checks_cancellation_before_inspecting_duplicates() {
    let validator = schema()
        .prepare(
            &DefaultEvaluator::new(),
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap()
        .into_validator();
    let duplicate = JsonValue::parse(r#"{"x":1,"x":2}"#).unwrap();
    for detail in [Detail::Outcome, Detail::Explain] {
        assert!(
            matches!(validator.evaluate(&duplicate, detail, &AtomicBool::new(true)),
            ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
        );
    }
}

#[test]
fn every_recursive_refinement_and_certificate_stop_keeps_clean_retries() {
    let mut controls: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/semantic-recursive-extension.json")).unwrap();
    let coordinates: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/semantic-coordinate-refinement.json")).unwrap();
    controls["cases"]
        .as_array_mut()
        .unwrap()
        .extend(coordinates["cases"].as_array().unwrap().iter().cloned());
    let selected = [
        "recursive-adversarial/opaque-component-not-self",
        "recursive-adversarial/unresolved-annotation-consumer",
        "recursive-extension/closed-or/size-4",
        "recursive-extension/annotation/known-undefined-producer",
        "recursive-extension/exclusion/absent-content",
        "coordinate-review/pattern-does-not-produce-y",
        "coordinate-review/patternProperties-undefined-correlation",
        "coordinate-review/unknown-external-support",
    ];
    assert_eq!(
        controls["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| selected.contains(&case["id"].as_str().unwrap()))
            .count(),
        selected.len()
    );
    for case in controls["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| selected.contains(&case["id"].as_str().unwrap()))
    {
        let schema = Document::parse(case["document"].to_string())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap()
            .operation("op")
            .unwrap()
            .input()
            .unwrap();
        let prepared = schema
            .prepare(
                &DefaultEvaluator::new(),
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
            )
            .unwrap();
        let value = JsonValue::parse(case["value"].to_string()).unwrap();
        let code = |outcome: &ValueOutcome| match outcome {
            ValueOutcome::Satisfies => "S",
            ValueOutcome::Fails { .. } => "F",
            ValueOutcome::Undefined { .. } => "U",
            ValueOutcome::NoVerdict { detail }
                if detail.reason == NoVerdictReason::ResourceUnavailable =>
            {
                "N:resource-unavailable"
            }
            _ => "unexpected",
        };
        let verdict_counter = CancelAfter {
            calls: AtomicUsize::new(0),
            allowance: usize::MAX,
        };
        let verdict = prepared.evaluate_with_control(&value, &verdict_counter);
        let verdict_checks = verdict_counter.calls.load(Ordering::Relaxed);
        let established_failure = matches!(verdict, ValueOutcome::Fails { .. });
        for explain in [false, true] {
            let invoke = |control: &dyn WorkControl| {
                if explain {
                    prepared.explain_with_control(&value, control)
                } else {
                    prepared.evaluate_with_control(&value, control)
                }
            };
            let counter = CancelAfter {
                calls: AtomicUsize::new(0),
                allowance: usize::MAX,
            };
            assert_eq!(
                code(&invoke(&counter)),
                case["required"].as_str().unwrap(),
                "{}",
                case["id"]
            );
            let checks = counter.calls.load(Ordering::Relaxed);
            for allowance in 0..=checks {
                let control = CancelAfter {
                    calls: AtomicUsize::new(0),
                    allowance,
                };
                let result = invoke(&control);
                if control.calls.load(Ordering::Relaxed) > allowance {
                    if explain && established_failure && allowance >= verdict_checks {
                        // Later diagnostic cancellation cannot erase an already
                        // established failure, including a recursive proof.
                        assert!(
                            matches!(result, ValueOutcome::Fails { problems, problems_complete: false, .. } if problems.is_empty()),
                            "{} diagnostic stop {allowance}",
                            case["id"]
                        );
                    } else {
                        assert!(
                            matches!(result, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled),
                            "{} stop {allowance}",
                            case["id"]
                        );
                    }
                } else {
                    assert_eq!(
                        code(&result),
                        case["required"].as_str().unwrap(),
                        "{} stop {allowance}",
                        case["id"]
                    );
                }
                assert_eq!(
                    code(&invoke(&AtomicBool::new(false))),
                    case["required"].as_str().unwrap(),
                    "{} retry {allowance}",
                    case["id"]
                );
            }
        }
    }
}

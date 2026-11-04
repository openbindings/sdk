use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct CancelAfterChecks {
    calls: AtomicUsize,
    healthy_checks: usize,
}
impl CancelAfterChecks {
    fn new(healthy_checks: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            healthy_checks,
        }
    }
}
impl WorkControl for CancelAfterChecks {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) >= self.healthy_checks
    }
}

// Retain the public evaluator seam so the same prepared schema can be checked
// directly and through the normative contract, which adds its own entry check.
#[derive(Default)]
struct RetainPrepared(Mutex<Option<openbindings_json_schema_evaluator::DefaultValidator>>);
impl SchemaEvaluator for RetainPrepared {
    type Validator = openbindings_json_schema_evaluator::DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let schema = DefaultEvaluator::new().prepare(request, limits, control)?;
        *self.0.lock().unwrap() = Some(schema.clone());
        Ok(schema)
    }
}

fn prepared(
    schema: Value,
) -> (
    PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>,
    openbindings_json_schema_evaluator::DefaultValidator,
) {
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let evaluator = RetainPrepared::default();
    let document = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = document
        .resolve_operation("op")
        .expect("declared operation");
    let contract = operation
        .input()
        .expect("declared input")
        .prepare(
            &evaluator,
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    let schema = evaluator.0.lock().unwrap().take().unwrap();
    (contract, schema)
}

fn assert_interrupted_failure(schema: Value, instance: &str) {
    let (contract, prepared) = prepared(schema);
    let value = JsonValue::parse(instance).unwrap();
    assert!(matches!(
        contract.evaluate(&value),
        ValueOutcome::Fails { .. }
    ));
    assert!(matches!(
        contract.explain(&value),
        ValueOutcome::Fails { problems, .. } if !problems.is_empty()
    ));

    for through_contract in [false, true] {
        let evaluate = |detail, control: &CancelAfterChecks| {
            if through_contract {
                match detail {
                    Detail::Outcome => contract.evaluate_with_control(&value, control),
                    Detail::Explain => contract.explain_with_control(&value, control),
                    _ => unreachable!("only the two explicit test details are passed"),
                }
            } else {
                prepared.evaluate(&value, detail, control)
            }
        };
        let outcome_checks = CancelAfterChecks::new(usize::MAX);
        assert!(matches!(
            evaluate(Detail::Outcome, &outcome_checks),
            ValueOutcome::Fails { .. }
        ));
        let verdict_boundary = outcome_checks.calls.load(Ordering::Relaxed);
        let explain_checks = CancelAfterChecks::new(usize::MAX);
        assert!(
            matches!(evaluate(Detail::Explain, &explain_checks), ValueOutcome::Fails { problems, .. } if !problems.is_empty())
        );
        let completed_explanation = explain_checks.calls.load(Ordering::Relaxed);
        assert!(completed_explanation > verdict_boundary);
        // Measure the same-input ordinary pass instead of assuming a callback
        // ordinal from an outgoing engine. Exhaust every observed boundary.
        for healthy in 0..=completed_explanation {
            let result = evaluate(Detail::Explain, &CancelAfterChecks::new(healthy));
            if healthy < verdict_boundary {
                assert!(matches!(result, ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::Cancelled));
            } else if healthy < completed_explanation {
                assert!(matches!(result, ValueOutcome::Fails {
                    problems, problems_complete: false, ..
                } if problems.is_empty()));
            } else {
                assert!(
                    matches!(result, ValueOutcome::Fails { problems, .. } if !problems.is_empty())
                );
            }
        }
        for healthy in 0..=verdict_boundary {
            let result = evaluate(Detail::Outcome, &CancelAfterChecks::new(healthy));
            if healthy < verdict_boundary {
                assert!(matches!(result, ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::Cancelled));
            } else {
                assert!(matches!(result, ValueOutcome::Fails {
                    problems, problems_complete: false, ..
                } if problems.is_empty()));
            }
        }
        assert!(
            matches!(evaluate(Detail::Explain, &CancelAfterChecks::new(usize::MAX)),
            ValueOutcome::Fails { problems, .. } if !problems.is_empty())
        );
    }
}

#[test]
fn boolean_failure_survives_cancellation_after_verdict() {
    assert_interrupted_failure(json!(false), "null");
}

#[test]
fn type_failure_survives_cancellation_after_verdict() {
    assert_interrupted_failure(json!({"type":"integer"}), r#""not an integer""#);
}

#[test]
fn cancellation_before_verdict_still_yields_no_verdict() {
    for schema in [json!(false), json!({"type":"integer"})] {
        let (contract, prepared) = prepared(schema);
        let value = JsonValue::null();
        for outcome in [
            prepared.evaluate(&value, Detail::Outcome, &AtomicBool::new(true)),
            prepared.evaluate(&value, Detail::Explain, &AtomicBool::new(true)),
            contract.evaluate_with_control(&value, &AtomicBool::new(true)),
            contract.explain_with_control(&value, &AtomicBool::new(true)),
            // Cancellation at the evaluator's entry is still before a verdict.
            contract.evaluate_with_control(&value, &CancelAfterChecks::new(1)),
            contract.explain_with_control(&value, &CancelAfterChecks::new(1)),
        ] {
            assert!(
                matches!(&outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled),
                "cancellation before evaluation must prevent a verdict: {outcome:?}"
            );
        }
    }
}

#[test]
fn established_satisfaction_survives_cancellation_after_verdict() {
    for (schema, instance) in [(json!(true), "null"), (json!({"type":"integer"}), "1")] {
        let (contract, prepared) = prepared(schema);
        let value = JsonValue::parse(instance).unwrap();
        assert!(matches!(contract.evaluate(&value), ValueOutcome::Satisfies));
        for through_contract in [false, true] {
            let evaluate = |detail, control: &CancelAfterChecks| {
                if through_contract {
                    match detail {
                        Detail::Outcome => contract.evaluate_with_control(&value, control),
                        Detail::Explain => contract.explain_with_control(&value, control),
                        _ => unreachable!("only the two explicit test details are passed"),
                    }
                } else {
                    prepared.evaluate(&value, detail, control)
                }
            };
            let baseline = CancelAfterChecks::new(usize::MAX);
            assert!(matches!(
                evaluate(Detail::Outcome, &baseline),
                ValueOutcome::Satisfies
            ));
            let verdict_boundary = baseline.calls.load(Ordering::Relaxed);
            for detail in [Detail::Outcome, Detail::Explain] {
                for healthy in 0..verdict_boundary {
                    assert!(matches!(evaluate(detail, &CancelAfterChecks::new(healthy)),
                        ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled));
                }
                // S has no diagnostic second pass or additional cancellation
                // check that can replace its completed verdict.
                let control = CancelAfterChecks::new(verdict_boundary);
                assert!(matches!(
                    evaluate(detail, &control),
                    ValueOutcome::Satisfies
                ));
                assert_eq!(control.calls.load(Ordering::Relaxed), verdict_boundary);
                assert!(matches!(
                    evaluate(detail, &CancelAfterChecks::new(usize::MAX)),
                    ValueOutcome::Satisfies
                ));
            }
        }
    }
}

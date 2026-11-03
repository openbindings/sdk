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

    // At the evaluator seam, the first observation is healthy and every later
    // observation is cancelled. The contract has one additional entry check.
    let outcomes = [
        prepared.evaluate(&value, Detail::Explain, &CancelAfterChecks::new(1)),
        contract.explain_with_control(&value, &CancelAfterChecks::new(2)),
        prepared.evaluate(&value, Detail::Outcome, &CancelAfterChecks::new(1)),
        contract.evaluate_with_control(&value, &CancelAfterChecks::new(2)),
    ];
    assert!(
        outcomes.iter().all(|outcome| matches!(outcome, ValueOutcome::Fails { problems, problems_complete: false, .. } if problems.is_empty())),
        "an established failure must survive cancelled diagnostics: {outcomes:?}"
    );
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
    // Each control allows the observation an evaluation makes before its
    // verdict and reports cancellation at every later one.
    for (schema, instance, before_verdict) in [
        (json!(true), "null", 1),
        (json!({"type":"integer"}), "1", 1),
    ] {
        let (contract, prepared) = prepared(schema);
        let value = JsonValue::parse(instance).unwrap();
        assert!(matches!(contract.evaluate(&value), ValueOutcome::Satisfies));
        for outcome in [
            prepared.evaluate(
                &value,
                Detail::Outcome,
                &CancelAfterChecks::new(before_verdict),
            ),
            prepared.evaluate(
                &value,
                Detail::Explain,
                &CancelAfterChecks::new(before_verdict),
            ),
            contract.evaluate_with_control(&value, &CancelAfterChecks::new(before_verdict + 1)),
            contract.explain_with_control(&value, &CancelAfterChecks::new(before_verdict + 1)),
        ] {
            assert!(
                matches!(outcome, ValueOutcome::Satisfies),
                "an established satisfaction must survive later cancellation: {outcome:?}"
            );
        }
    }
}

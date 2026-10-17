use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
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
struct RetainPrepared(Mutex<Option<Arc<dyn PreparedSchema>>>);
impl SchemaEvaluator for RetainPrepared {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let schema = DefaultEvaluator::new().prepare(request, control)?;
        *self.0.lock().unwrap() = Some(schema.clone());
        Ok(schema)
    }
}

fn prepared(schema: Value) -> (PreparedContract, Arc<dyn PreparedSchema>) {
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
        .prepare(&evaluator)
        .unwrap();
    let schema = evaluator.0.lock().unwrap().take().unwrap();
    (contract, schema)
}

fn assert_interrupted_failure(schema: Value, instance: &str) {
    let (contract, prepared) = prepared(schema);
    let value = JsonValue::parse(instance).unwrap();
    assert!(matches!(
        contract.validate(&value),
        ValueOutcome::Fails { .. }
    ));
    assert!(matches!(
        contract.explain(&value),
        ValueOutcome::Fails { problems, .. } if !problems.is_empty()
    ));

    // At the evaluator seam, the first observation is healthy and every later
    // observation is cancelled. The contract has one additional entry check.
    let outcomes = [
        prepared.validate(&value, Detail::Explain, &CancelAfterChecks::new(1)),
        contract.explain_with_control(&value, &CancelAfterChecks::new(2)),
        prepared.validate(&value, Detail::Outcome, &CancelAfterChecks::new(1)),
        contract.validate_with_control(&value, &CancelAfterChecks::new(2)),
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

fn partial_schema() -> Value {
    json!({
        "type":"object",
        "properties":{"external":{"$ref":"https://example.invalid/missing"}}
    })
}

#[test]
fn upper_bound_failure_survives_cancellation_after_verdict() {
    let (contract, _) = prepared(partial_schema());
    // Reaching the absent resource cannot decide, proving this preparation is
    // partial even though the known object constraint can establish a failure.
    assert!(matches!(
        contract.validate(&JsonValue::parse(r#"{"external":1}"#).unwrap()),
        ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable
    ));
    assert_interrupted_failure(partial_schema(), "0");
}

#[test]
fn cancellation_before_verdict_still_yields_no_verdict() {
    for schema in [json!(false), json!({"type":"integer"}), partial_schema()] {
        let (contract, prepared) = prepared(schema);
        let value = JsonValue::null();
        for outcome in [
            prepared.validate(&value, Detail::Outcome, &AtomicBool::new(true)),
            prepared.validate(&value, Detail::Explain, &AtomicBool::new(true)),
            contract.validate_with_control(&value, &AtomicBool::new(true)),
            contract.explain_with_control(&value, &AtomicBool::new(true)),
            // Cancellation at the evaluator's entry is still before a verdict.
            contract.validate_with_control(&value, &CancelAfterChecks::new(1)),
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
fn satisfaction_still_checks_cancellation_after_verdict() {
    for (schema, instance) in [
        (json!(true), "null"),
        (json!({"type":"integer"}), "1"),
        (partial_schema(), "{}"),
    ] {
        let (contract, prepared) = prepared(schema);
        let value = JsonValue::parse(instance).unwrap();
        assert!(matches!(contract.validate(&value), ValueOutcome::Satisfies));
        for outcome in [
            prepared.validate(&value, Detail::Outcome, &CancelAfterChecks::new(1)),
            prepared.validate(&value, Detail::Explain, &CancelAfterChecks::new(1)),
            contract.validate_with_control(&value, &CancelAfterChecks::new(2)),
            contract.explain_with_control(&value, &CancelAfterChecks::new(2)),
        ] {
            assert!(
                matches!(&outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled),
                "satisfaction must still observe cancellation: {outcome:?}"
            );
        }
    }
}

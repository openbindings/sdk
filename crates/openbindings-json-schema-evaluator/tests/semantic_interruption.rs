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

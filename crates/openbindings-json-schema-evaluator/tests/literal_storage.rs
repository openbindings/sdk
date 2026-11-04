//! Literal comparisons preserve exact values without retaining scalar copies.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator, Limits};
use std::sync::atomic::{AtomicUsize, Ordering};

fn prepare(schema: &str, steps: Option<usize>) -> PreparedSchema<DefaultValidator> {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
    .resolve_operation("op")
    .unwrap()
    .input()
    .unwrap()
    .prepare(
        &DefaultEvaluator::with_limits(Limits {
            program_bytes: 64 * 1024,
            evaluation_steps: steps,
            ..Limits::default()
        }),
        &ExternalSchemas::default(),
        &PreparationLimits::default(),
    )
    .unwrap()
}

#[test]
fn large_scalar_literals_decide_with_small_proof_storage() {
    for size in [1024, 1 << 20] {
        let text = "a".repeat(size);
        let literal = format!(r#""{text}""#);
        let equal = JsonValue::parse(&literal).unwrap();
        let escaped = JsonValue::parse(format!(r#""{}\u0061""#, &text[1..])).unwrap();
        let unequal = JsonValue::parse(format!(r#""{}b""#, &text[1..])).unwrap();
        for (keyword, schema) in [
            ("const", format!(r#"{{"const":{literal}}}"#)),
            (
                "enum",
                format!(r#"{{"enum":[null,"different",{literal},42]}}"#),
            ),
        ] {
            let contract = prepare(&schema, None);
            for value in [&equal, &escaped] {
                assert!(matches!(contract.evaluate(value), ValueOutcome::Satisfies));
                assert!(matches!(contract.explain(value), ValueOutcome::Satisfies));
            }
            assert!(matches!(
                contract.evaluate(&unequal),
                ValueOutcome::Fails { .. }
            ));
            let ValueOutcome::Fails { problems, .. } = contract.explain(&unequal) else {
                panic!("different final unit must fail {keyword}")
            };
            assert_eq!(problems.len(), 1);
            assert_eq!(
                problems[0].schema.pointer,
                format!("/operations/op/input/{keyword}")
            );
        }
    }
}

struct CancelAt {
    calls: AtomicUsize,
    boundary: usize,
}
impl CancelAt {
    fn new(boundary: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            boundary,
        }
    }
}
impl WorkControl for CancelAt {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) >= self.boundary
    }
}

#[test]
fn long_literal_comparisons_honor_work_cancellation_and_healthy_retries() {
    let literal = format!(r#""{}""#, "a".repeat(65_536));
    let schema = format!(r#"{{"const":{literal}}}"#);
    let value = JsonValue::parse(literal).unwrap();
    let contract = prepare(&schema, None);
    let healthy = CancelAt::new(usize::MAX);
    assert!(matches!(
        contract.evaluate_with_control(&value, &healthy),
        ValueOutcome::Satisfies
    ));
    let checks = healthy.calls.load(Ordering::Relaxed);
    assert!(
        checks > 64,
        "long equality must observe cancellation during its traversal"
    );
    for boundary in 0..checks {
        assert!(
            matches!(
                contract.evaluate_with_control(&value, &CancelAt::new(boundary)),
                ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled
            ),
            "boundary {boundary}"
        );
    }
    assert!(matches!(contract.evaluate(&value), ValueOutcome::Satisfies));
    assert!(matches!(
        prepare(&schema, Some(0)).evaluate(&value),
        ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded
            && detail.code == "evaluation-work-limit"
    ));
    assert!(matches!(
        prepare(&schema, Some(64 << 20)).evaluate(&value),
        ValueOutcome::Satisfies
    ));
}

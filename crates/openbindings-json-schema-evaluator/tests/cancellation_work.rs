//! Cancellation propagation has a fixed poll bound independent of input width.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CancelOnPoll {
    polls: AtomicUsize,
    k: usize,
}

impl CancelOnPoll {
    fn new(k: usize) -> Self {
        Self {
            polls: AtomicUsize::new(0),
            k,
        }
    }

    fn assert_bounded(&self, path: &str, n: usize) {
        let polls = self.polls.load(Ordering::Relaxed);
        let after = polls - self.k;
        println!("{path}: n={n}, k={}, polls={polls}, after={after}", self.k);
        // Allow four cleanup/final-wrapper polls after the first true response.
        // This bound is constant, not a fraction of the document or value size.
        assert!(after <= 4, "{path}: {after} polls after cancellation");
    }
}

impl WorkControl for CancelOnPoll {
    fn is_cancelled(&self) -> bool {
        self.polls.fetch_add(1, Ordering::Relaxed) + 1 >= self.k
    }
}

fn document(schema: &str) -> ConformantDocument {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
}

#[test]
fn large_schema_preparation_returns_cancelled_with_bounded_extra_polls() {
    for n in [1_000, 4_000, 16_000] {
        let properties = (0..n)
            .map(|i| format!(r#""p{i}":{{"type":"integer"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let schema = format!(r#"{{"properties":{{{properties}}}}}"#);
        for k in [1, 16, 64] {
            // Each attempt starts with a fresh document and preparation cache.
            let doc = document(&schema);
            let control = CancelOnPoll::new(k);
            let result = doc
                .resolve_operation("op")
                .unwrap()
                .input()
                .unwrap()
                .prepare_with_control(
                    &DefaultEvaluator::new(),
                    &ExternalSchemas::default(),
                    &PreparationLimits::default(),
                    &control,
                );
            assert!(
                matches!(result, Err(error) if error.kind == PreparationFailureKind::Cancelled)
            );
            control.assert_bounded("preparation", n);
        }
    }
}

#[test]
fn large_value_evaluation_returns_cancelled_with_bounded_extra_polls() {
    for n in [65_536, 262_144, 1_048_576] {
        let value = JsonValue::parse(format!(r#""{}""#, "a".repeat(n))).unwrap();
        let doc = document(&format!(r#"{{"const":{}}}"#, value.text()));
        for work in [None, Some(usize::MAX)] {
            let prepared = doc
                .resolve_operation("op")
                .unwrap()
                .input()
                .unwrap()
                .prepare(
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_work_units: work,
                        ..Limits::default()
                    }),
                    &ExternalSchemas::default(),
                )
                .unwrap();
            let healthy = CancelOnPoll::new(usize::MAX);
            assert!(matches!(
                prepared.evaluate_with_control(&value, &healthy),
                ValueOutcome::Satisfies
            ));
            let total = healthy.polls.load(Ordering::Relaxed);
            assert!(total >= 2);
            println!("evaluation baseline: n={n}, work={work:?}, polls={total}");
            for k in [1, 16, 64, total / 2, total] {
                if k > total {
                    continue;
                }
                let control = CancelOnPoll::new(k);
                let result = prepared.evaluate_with_control(&value, &control);
                assert!(matches!(result, ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::Cancelled));
                control.assert_bounded("evaluation", n);
            }
        }
    }
}

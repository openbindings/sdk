use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

#[test]
fn evaluator_enum_membership_work_grows_no_faster_than_n_log_n() {
    let mut measurements = Vec::new();
    for n in [64, 256, 1_024] {
        let choices = (0..n)
            .map(|i| format!("{i}.0"))
            .collect::<Vec<_>>()
            .join(",");
        let doc = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"items":{{"enum":[{choices}]}}}}}}}}}}"#
        )).unwrap().validate().unwrap().conformant().unwrap();
        let value = JsonValue::parse(format!(
            "[{}]",
            (0..n)
                .rev()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ))
        .unwrap();
        let completes = |work| {
            let prepared = doc
                .resolve_operation("op")
                .unwrap()
                .input()
                .unwrap()
                .prepare(
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_work_units: Some(work),
                        evaluation_sdk_reservation_bytes: 1024 * 1024 * 1024,
                        ..Limits::default()
                    }),
                    &ExternalSchemas::default(),
                )
                .unwrap();
            match prepared.evaluate(&value) {
                ValueOutcome::Satisfies => true,
                ValueOutcome::NoVerdict { detail } => {
                    assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
                    assert_eq!(detail.code, "evaluation-work-limit");
                    false
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
        };
        assert!(!completes(0));
        let (mut low, mut high) = (0, 1usize);
        while !completes(high) {
            high = high.checked_mul(2).unwrap();
        }
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if completes(middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        println!("evaluator enum: n={n}, minimum evaluation_sdk_work_units={high}");
        measurements.push(high);
    }
    for pair in measurements.windows(2) {
        assert!(
            pair[1] <= 8 * pair[0],
            "evaluator enum: {} > 8 * {}",
            pair[1],
            pair[0]
        );
    }
}

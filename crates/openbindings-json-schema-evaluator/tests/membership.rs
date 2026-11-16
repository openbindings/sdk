use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

#[test]
fn numeric_enum_reservations_allow_both_value_orders_and_spellings() {
    for count in [1_440, 4_096] {
        for decimal in [false, true] {
            let choices = (0..count)
                .map(|i| {
                    if decimal {
                        format!("{i}.0")
                    } else {
                        i.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let resources = ExternalSchemas::new([ExternalSchema {
                uri: "https://membership.test/root".into(),
                document: JsonValue::parse(format!(r#"{{"items":{{"enum":[{choices}]}}}}"#))
                    .unwrap(),
            }])
            .unwrap();
            let prepared = resources
                .prepare("https://membership.test/root", &DefaultEvaluator::new())
                .unwrap();
            for descending in [false, true] {
                let values = (0..count)
                    .map(|i| if descending { count - 1 - i } else { i }.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let value = JsonValue::parse(format!("[{values}]")).unwrap();
                for outcome in [prepared.evaluate(&value), prepared.explain(&value)] {
                    assert_eq!(
                        outcome,
                        ValueOutcome::Satisfies,
                        "count={count}, decimal={decimal}, descending={descending}"
                    );
                }
            }
        }
    }
}

#[test]
fn enum_membership_keeps_exact_verdicts_and_original_keyword() {
    let mixed = [
        "null",
        "false",
        "1.0",
        r#""\u0061""#,
        "[0,1]",
        r#"{"a":1,"b":0}"#,
        "2",
        "3",
        "true",
        "1e400",
        r#""😀""#,
        r#"{"a":[1,{"z":null}]}"#,
    ];
    let probes = [
        ("null", 0),
        ("false", 1),
        ("1", 2),
        (r#""a""#, 3),
        ("[0.0,1e0]", 4),
        (r#"{"b":0.0,"a":1e0}"#, 5),
        ("true", 8),
        ("10e399", 9),
        (r#""\ud83d\ude00""#, 10),
        (r#"{"a":[1.0,{"z":null}]}"#, 11),
        ("99", usize::MAX),
        (r#""missing""#, usize::MAX),
        ("[1,0]", usize::MAX),
        (r#"{"a":0,"b":1}"#, usize::MAX),
    ];
    for n in [3, 8, 9, mixed.len()] {
        let doc = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"enum":[{}]}}}}}}}}"#,
            mixed[..n].join(",")
        ))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
        let p = doc
            .operation("op")
            .unwrap()
            .input()
            .unwrap()
            .prepare(
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_work_units: Some(usize::MAX),
                    ..Default::default()
                }),
                &ExternalSchemas::default(),
            )
            .unwrap();
        for (text, first_choice) in probes {
            let value = JsonValue::parse(text).unwrap();
            for outcome in [p.evaluate(&value), p.explain(&value)] {
                assert_eq!(
                    matches!(outcome, ValueOutcome::Satisfies),
                    first_choice < n,
                    "n={n}: {outcome:?}"
                );
                assert!(matches!(
                    outcome,
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ));
            }
            if first_choice >= n {
                let ValueOutcome::Fails { problems, .. } = p.explain(&value) else {
                    panic!("enum miss must fail")
                };
                assert_eq!(problems.len(), 1);
                assert_eq!(problems[0].code, "enum");
                assert_eq!(problems[0].schema.pointer, "/operations/op/input/enum");
            }
        }
    }
}

fn schema_doc(schema: &str) -> ConformantDocument {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
}
fn funded(
    doc: &ConformantDocument,
    work: usize,
    bytes: usize,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    doc.operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::with_limits(Limits {
                evaluation_sdk_work_units: Some(work),
                evaluation_sdk_reservation_bytes: bytes,
                ..Default::default()
            }),
            &ExternalSchemas::default(),
        )
        .unwrap()
}
fn repeated(token: &str, count: usize) -> JsonValue {
    JsonValue::parse(format!("[{}]", vec![token; count].join(","))).unwrap()
}

#[test]
fn enum_index_uses_evaluation_reservation_and_healthy_retry() {
    let choices = (0..20_000)
        .map(|i| format!("\"v{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let doc = schema_doc(&format!(r#"{{"items":{{"enum":[{choices}]}}}}"#));
    let boundary = 20_000usize.ilog2() as usize;
    let linear = repeated(r#""v0""#, boundary);
    let indexed = repeated(r#""v0""#, boundary + 1);
    let small = funded(&doc, usize::MAX, 64 * 1024);
    assert!(matches!(small.evaluate(&linear), ValueOutcome::Satisfies));
    let ValueOutcome::NoVerdict { detail } = small.evaluate(&indexed) else {
        panic!("index must exceed the evaluation reservation")
    };
    assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
    assert_eq!(detail.code, "semantic-analysis-memory-limit");
    assert!(matches!(small.evaluate(&linear), ValueOutcome::Satisfies));
    assert!(matches!(
        funded(&doc, usize::MAX, 1 << 20).evaluate(&indexed),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn enum_membership_at_the_query_trigger_matches_the_plain_walk() {
    for m in [9usize, 16, 1_000] {
        let choices = (0..m).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let doc = schema_doc(&format!(r#"{{"items":{{"enum":[{choices}]}}}}"#));
        let p = funded(&doc, usize::MAX, 1 << 20);
        for q in [m.ilog2() as usize, m.ilog2() as usize + 1] {
            for (token, expected) in [((m - 1).to_string(), true), (m.to_string(), false)] {
                let result = p.evaluate(&repeated(&token, q));
                assert!(matches!(
                    result,
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ));
                assert_eq!(matches!(result, ValueOutcome::Satisfies), expected);
            }
        }
    }
}

#[test]
fn repeated_evaluations_have_identical_minimum_completing_work() {
    let choices = (0..64)
        .map(|i| format!("{i}.0"))
        .collect::<Vec<_>>()
        .join(",");
    let doc = schema_doc(&format!(r#"{{"items":{{"enum":[{choices}]}}}}"#));
    let value = repeated("63", 128);
    let completes = |work: usize, pass: usize| -> bool {
        let p = funded(&doc, work, 1 << 28);
        let results = [p.evaluate(&value), p.evaluate(&value)];
        let decided = results.map(|outcome| match outcome {
            ValueOutcome::Satisfies => true,
            ValueOutcome::NoVerdict { detail } => {
                assert_eq!(detail.code, "evaluation-work-limit");
                false
            }
            other => panic!("unexpected enum outcome: {other:?}"),
        });
        assert_eq!(decided[0], decided[1]);
        decided[pass]
    };
    let minimum = |pass| {
        let (mut lo, mut hi) = (0, 1usize);
        assert!(!completes(0, pass));
        while !completes(hi, pass) {
            hi = hi.checked_mul(2).unwrap();
        }
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if completes(mid, pass) {
                hi = mid
            } else {
                lo = mid
            }
        }
        hi
    };
    assert_eq!(minimum(0), minimum(1));
}

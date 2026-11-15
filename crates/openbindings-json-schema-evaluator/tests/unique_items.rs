use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

#[test]
fn unique_items_preserves_exact_equality_through_public_evaluation() {
    let doc = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"o":{"input":{"uniqueItems":true}}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    for evaluator in [
        DefaultEvaluator::new(),
        DefaultEvaluator::with_limits(Limits {
            evaluation_sdk_work_units: Some(usize::MAX),
            ..Default::default()
        }),
    ] {
        let prepared = doc
            .operation("o")
            .unwrap()
            .input()
            .unwrap()
            .prepare(&evaluator, &ExternalSchemas::default())
            .unwrap();
        for (a, b) in [
            ("0", "-0.0e300"),
            ("1e400", "10e399"),
            (r#""a""#, r#""\u0061""#),
            (r#""\uD800""#, r#""\ud800""#),
            (r#""😀""#, r#""\ud83d\ude00""#),
            (r#"{"a":1,"b":[2,3]}"#, r#"{"b":[2.0,30e-1],"\u0061":1.0}"#),
        ] {
            let padding = (0..64)
                .map(|i| format!("\"padding-{i}\""))
                .collect::<Vec<_>>()
                .join(",");
            for text in [format!("[{a},{padding}]"), format!("[{a},{padding},{b}]")] {
                let value = JsonValue::parse(&text).unwrap();
                for (explain, outcome) in [
                    (false, prepared.evaluate(&value)),
                    (true, prepared.explain(&value)),
                ] {
                    assert!(matches!(
                        outcome,
                        ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                    ));
                    assert_eq!(
                        matches!(outcome, ValueOutcome::Satisfies),
                        value.view().len() == Some(65)
                    );
                    if let ValueOutcome::Fails { problems, .. } = outcome
                        && explain
                    {
                        assert_eq!(problems[0].code, "uniqueItems");
                    }
                }
            }
        }
    }
}

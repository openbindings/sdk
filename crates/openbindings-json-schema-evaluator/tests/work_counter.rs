use openbindings::{Document, ExternalSchemas, JsonValue, ValueOutcome};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

#[test]
fn pattern_work_above_u32_is_uncapped_or_refused_by_the_requested_cap() {
    let document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"pattern":"^(?:a{1,200}){1,200}$"}}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let operation = document.operation("op").unwrap();
    let schema = operation.input().unwrap();
    // An anchored mismatch on the first byte keeps the actual search short;
    // its conservative length-times-program charge still exceeds u32::MAX.
    let value = JsonValue::string(&format!("b{}", "a".repeat(20_000))).unwrap();
    for cap in [None, Some(u32::MAX as usize)] {
        let prepared = schema
            .prepare(
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_work_units: cap,
                    pattern_search_work_units: None,
                    ..Limits::default()
                }),
                &ExternalSchemas::default(),
            )
            .unwrap();
        match (cap, prepared.evaluate(&value)) {
            (None, ValueOutcome::Fails { .. }) => {}
            (Some(_), ValueOutcome::NoVerdict { detail }) => {
                assert_eq!(detail.code, "evaluation-work-limit");
            }
            (_, outcome) => panic!("unexpected work accounting outcome: {outcome:?}"),
        }
    }
}

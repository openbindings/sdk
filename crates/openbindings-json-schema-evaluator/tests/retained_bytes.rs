use openbindings::{Document, ExternalSchema, ExternalSchemas, JsonValue};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn sdk() -> DefaultEvaluator {
    DefaultEvaluator::with_limits(Limits {
        evaluation_sdk_work_units: Some(1_000_000),
        ..Limits::default()
    })
}

#[test]
fn sdk_estimate_grows_with_retained_source_and_is_stable_across_shared_clones() {
    let mut previous = 0;
    for length in [10, 1_000, 100_000] {
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{"run":{{"input":true}}}},"x-padding":"{}"}}"#,
            "x".repeat(length)
        );
        let proof = Document::parse(&text)
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let prepared = proof
            .resolve_operation("run")
            .unwrap()
            .input()
            .unwrap()
            .prepare(&sdk(), &ExternalSchemas::default())
            .unwrap();
        drop(proof);
        let bytes = prepared.retained_bytes().unwrap();
        assert!(bytes >= text.len());
        assert!(bytes > previous);
        previous = bytes;
        let clone = prepared.clone();
        assert_eq!(clone.retained_bytes(), Some(bytes));
        drop(prepared);
        assert_eq!(clone.retained_bytes(), Some(bytes));
        drop(clone.evaluate(&JsonValue::null()));
        assert_eq!(clone.retained_bytes(), Some(bytes));
    }
}

#[test]
fn supplied_roots_keep_full_source_text_in_the_estimate() {
    let text = format!(
        r#"{{"type":"string","description":"{}"}}"#,
        "x".repeat(32_768)
    );
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://schema.test/root".into(),
        document: JsonValue::parse(&text).unwrap(),
    }])
    .unwrap();
    let prepared = resources
        .prepare("https://schema.test/root", &sdk())
        .unwrap();
    drop(resources);
    assert!(prepared.retained_bytes().unwrap() >= text.len());
    assert_eq!(prepared.clone().retained_bytes(), prepared.retained_bytes());
}

#[test]
fn compiled_validator_reports_unknown_size_even_when_evaluation_uses_sdk_explanation() {
    let proof = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"string"}}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let prepared = proof
        .resolve_operation("run")
        .unwrap()
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())
        .unwrap();
    assert_eq!(prepared.retained_bytes(), None);
    drop(prepared.explain(&JsonValue::null()));
    assert_eq!(prepared.retained_bytes(), None);
}

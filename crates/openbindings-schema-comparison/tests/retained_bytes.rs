use openbindings::JsonValue;
use openbindings_schema_comparison::{ComparisonContext, ComparisonLimits, identical};

#[test]
fn estimate_grows_with_schema_and_includes_the_entire_retained_source() {
    let small = ComparisonContext::new(JsonValue::parse("true").unwrap());
    let text = format!(
        r#"{{"$defs":{{"small":true}},"description":"{}"}}"#,
        "x".repeat(8192)
    );
    let root = JsonValue::parse(&text).unwrap();
    let subtree = root
        .view()
        .get("$defs")
        .unwrap()
        .get("small")
        .unwrap()
        .to_owned();
    let large = ComparisonContext::new(subtree);
    drop(root);
    assert!(large.retained_bytes() >= text.len());
    assert!(large.retained_bytes() > small.retained_bytes());
    assert_eq!(large.retained_bytes(), large.clone().retained_bytes());
    assert_eq!(large.retained_bytes(), large.retained_bytes());
}

#[test]
fn shared_arenas_count_once_within_one_context_but_per_owner_across_contexts() {
    let text = format!(r#"{{"description":"{}"}}"#, "x".repeat(8192));
    let root = JsonValue::parse(&text).unwrap();
    let shared = ComparisonContext::new(root.clone()).with_resource("https://schema.test/a", root);
    let separate = ComparisonContext::new(JsonValue::parse(&text).unwrap())
        .with_resource("https://schema.test/a", JsonValue::parse(&text).unwrap());
    assert!(separate.retained_bytes() >= shared.retained_bytes() + text.len());
    assert_eq!(shared.retained_bytes(), shared.clone().retained_bytes());
}

#[test]
fn completed_reference_indexes_grow_the_estimate_and_are_shared_by_clones() {
    let root = JsonValue::parse(r#"{"$ref":"https://schema.test/a#text"}"#).unwrap();
    let context = ComparisonContext::new(root.clone()).with_resource(
        "https://schema.test/a",
        JsonValue::parse(r#"{"$defs":{"value":{"$anchor":"text","type":"string"}}}"#).unwrap(),
    );
    let clone = context.clone();
    let cold = context.retained_bytes();
    let input = context.schema(root);
    assert!(identical(Some(&input), Some(&input), &ComparisonLimits::default()).unwrap());
    let warm = context.retained_bytes();
    assert!(warm > cold);
    assert_eq!(warm, clone.retained_bytes());
    drop(input);
    assert_eq!(warm, context.retained_bytes());
    let detached = context
        .schema(JsonValue::parse(r#"{"$id":"https://detached.test","type":"number"}"#).unwrap());
    assert!(
        identical(
            Some(&detached),
            Some(&detached),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
    drop(detached);
    assert_eq!(warm, context.retained_bytes());
}

//! Allocation-free selections and the exact-arithmetic boundaries they preserve.
use super::*;

fn prepared(schema: &str) -> (DefaultValidator, DefaultValidator) {
    owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    )
}

fn padded(size: usize, fields: &str) -> JsonValue {
    let text = format!(r#"{{{fields},"padding":""}}"#);
    let text = text.replacen(
        r#""padding":"""#,
        &format!(r#""padding":"{}""#, "x".repeat(size - text.len())),
        1,
    );
    assert_eq!(text.len(), size);
    JsonValue::parse(text).unwrap()
}

fn union() -> String {
    let branch = |kind, fields: &[&str]| {
        let mut properties = serde_json::Map::new();
        properties.insert("type".into(), json!({"const":kind}));
        properties.insert("id".into(), json!({"type":"string"}));
        for name in fields {
            properties.insert((*name).into(), json!({"type":"integer","minimum":0}));
        }
        json!({"type":"object","required":["type","id"],"properties":properties})
    };
    json!({"type":"array","items":{"oneOf":[
        branch("order.created", &["total","items"]),
        branch("order.refunded", &["amount"]),
        branch("user.signed_in", &["sessions","devices","attempts"])
    ]}})
    .to_string()
}

#[test]
fn scalar_literals_and_integral_composites_enter_at_large_sizes() {
    for schema in [
        json!({"properties":{"kind":{"enum":["ready","waiting"]}}}).to_string(),
        r#"{"properties":{"kind":{"const":"ready"}}}"#.into(),
        r#"{"properties":{"kind":{"enum":[true,false,null]}}}"#.into(),
    ] {
        let (selected, original) = prepared(&schema);
        for size in [100 * 1024, 16 * 1024 * 1024] {
            assert_eq!(
                compare(&selected, &original, &padded(size, r#""kind":"ready""#)).0,
                "compiled"
            );
        }
    }
    let (selected, original) = prepared(&union());
    for size in [100 * 1024, 16 * 1024 * 1024] {
        let item = padded(
            size - 2,
            r#""type":"order.created","id":"evt_0","total":0,"items":2"#,
        );
        let value = JsonValue::parse(format!("[{}]", item.text())).unwrap();
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
    // Also exercise the dense 100 KiB shape, without padding away its subjects.
    let item = r#"{"type":"order.created","id":"evt_0","total":0,"items":2}"#;
    let text = format!(
        "[{}{}]",
        format!("{item},").repeat(100 * 1024 / (item.len() + 1)),
        item
    );
    assert_eq!(
        compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
        "compiled"
    );
}

#[test]
fn number_tokens_retain_lane_equivalence() {
    let (selected, original) = prepared(
        r#"{"properties":{"n":{"type":"integer","minimum":-9223372036854775808,"maximum":9223372036854775807}}}"#,
    );
    for token in [
        "-9223372036854775808",
        "9223372036854775807",
        "-9223372036854775809",
        "9223372036854775808",
        "1.0",
        "1e3",
        "-0",
        "1.5",
        "123456789012345678901234567890123456789012345678901234567890",
    ] {
        let value = JsonValue::parse(format!(r#"{{"n":{token}}}"#)).unwrap();
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
    for token in ["1.0", "1e3", "1.5"] {
        let small = JsonValue::parse(format!(r#"{{"n":0,"elsewhere":{token}}}"#)).unwrap();
        assert_eq!(compare(&selected, &original, &small).0, "compiled");
        let large = padded(16 * 1024 * 1024, &format!(r#""n":0,"elsewhere":{token}"#));
        assert_eq!(compare(&selected, &original, &large).0, "compiled");
    }
}

#[test]
fn string_spellings_and_duplicates_retain_lane_equivalence() {
    for schema in [
        r#"{"const":"a\n\u0062"}"#,
        r#"{"enum":["a\n\u0062","c"]}"#,
        r#"{"const":"\ud800"}"#,
    ] {
        let (selected, original) = prepared(schema);
        for value in [
            r#""a\n\u0062""#,
            r#""\u0061\nb""#,
            r#""a\nbc""#,
            r#""\ud800""#,
            r#""\udc00""#,
            r#"{"a":1,"a":2}"#,
        ] {
            let value = JsonValue::parse(value).unwrap();
            let lane = compare(&selected, &original, &value).0;
            if schema.contains("ud800") || value.has_duplicate_names() || backend_unpaired(&value) {
                assert_eq!(lane, "original");
            } else {
                assert_eq!(lane, "compiled");
            }
        }
    }
}

fn backend_unpaired(value: &JsonValue) -> bool {
    openbindings_internal_json::backend::has_unpaired(value)
}

#[test]
fn exact_work_and_nested_uniqueness_keep_their_guards() {
    for (schema, fields, large_lane) in [
        (
            r#"{"properties":{"n":{"enum":[1,2]}}}"#,
            r#""n":1"#,
            "original",
        ),
        (
            r#"{"properties":{"n":{"const":{"x":0}}}}"#,
            r#""n":{"x":0}"#,
            "original",
        ),
        (
            r#"{"properties":{"n":{"minimum":0}}}"#,
            r#""n":1.0"#,
            "compiled",
        ),
        (
            r#"{"properties":{"n":{"minimum":0.5}}}"#,
            r#""n":1"#,
            "compiled",
        ),
        (
            r#"{"properties":{"n":{"multipleOf":2}}}"#,
            r#""n":2"#,
            "original",
        ),
    ] {
        let (selected, original) = prepared(schema);
        assert_eq!(
            compare(&selected, &original, &padded(1024, fields)).0,
            "compiled"
        );
        assert_eq!(
            compare(&selected, &original, &padded(16 * 1024 * 1024, fields)).0,
            large_lane
        );
    }
    let (selected, original) = prepared(r#"{"properties":{"n":{"uniqueItems":true}}}"#);
    for size in [1024, 16 * 1024 * 1024] {
        assert_eq!(
            compare(&selected, &original, &padded(size, r#""n":[1,2]"#)).0,
            "original"
        );
    }
    // The old scalar exemption for multipleOf is unchanged.
    let (selected, original) = prepared(r#"{"multipleOf":2}"#);
    let value = JsonValue::parse(format!("\"{}\"", "x".repeat(16 * 1024 * 1024 - 2))).unwrap();
    assert_eq!(compare(&selected, &original, &value).0, "compiled");
}

#[test]
fn integral_composite_scan_honours_cancellation() {
    let (selected, _) = prepared(r#"{"items":{"type":"integer","minimum":0}}"#);
    struct CancelAt(AtomicUsize);
    impl WorkControl for CancelAt {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 20
        }
    }
    // Only a value over the estimate reaches the scan.
    let value = JsonValue::parse(format!("[{}0]", "0,".repeat(60_000))).unwrap();
    compiled::entries::take();
    let result = selected
        .compiled
        .outcome(&value, &CancelAt(AtomicUsize::new(0)));
    assert!(
        matches!(result, Some(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::Cancelled)
    );
    assert_eq!(compiled::entries::take(), [0, 0, 0]);
}

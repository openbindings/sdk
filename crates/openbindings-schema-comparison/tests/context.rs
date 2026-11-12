use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_schema_comparison::*;
use std::sync::atomic::AtomicBool;
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn input(text: &str) -> SchemaInput {
    let value = json(text);
    SchemaInput::new(value.clone(), value)
}
fn doc(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
#[test]
fn a_thousand_operation_interface_fits_default_limits() {
    let operations = (0..1000)
        .map(|i| {
            format!(r#""op{i:04}":{{"input":{{"type":"string"}},"output":{{"type":"number"}}}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    let document = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{{operations}}}}}"#
    ));
    let report = check_interface(
        &document,
        &document,
        &|_| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(report.operations().len(), 1000);
    assert_eq!(report.issues().count(), 0);
    for operation in report.operations() {
        assert_eq!(
            operation.input().unwrap().as_ref().unwrap().verdict(),
            Verdict::Compatible
        );
        assert_eq!(
            operation.output().unwrap().as_ref().unwrap().verdict(),
            Verdict::Compatible
        );
    }
}
#[test]
fn many_reference_comparisons_reuse_a_large_original_at_default_limits() {
    let entries = (0..10_000)
        .map(|i| format!(r#""s{i}":{{"type":"string"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let original = ComparisonContext::new(json(&format!(r#"{{"schemas":{{{entries}}}}}"#)));
    for i in 0..1000 {
        let schema = json(&format!(r##"{{"$ref":"#/schemas/s{i}"}}"##));
        let left = original.schema(schema.clone());
        let offered = ComparisonContext::new(json(&format!(
            r#"{{"schemas":{{"s{i}":{{"type":"number"}}}}}}"#
        )));
        let right = offered.schema(schema);
        assert_eq!(
            identical_with_references(
                Some(&left),
                Some(&right),
                &|reference| reference.target().is_ok(),
                &ComparisonLimits::default()
            ),
            Ok(true)
        );
        assert_eq!(
            identical_with_references(
                Some(&left),
                Some(&right),
                &|_| false,
                &ComparisonLimits::default()
            ),
            Ok(false)
        );
    }
}
#[test]
fn unused_defects_and_carrier_members_do_not_affect_selected_schemas() {
    let deep = format!("{}true{}", r#"{"items":"#.repeat(140), "}".repeat(140));
    let root = json(&format!(
        r#"{{"$id":false,"not":null,"schemas":{{"selected":{{"type":"string","$defs":{{"Unused":true}}}},"unused":{{"$id":"https://example.test/unused","$defs":{{"a":{{"$anchor":"same"}},"b":{{"$anchor":"same"}}}}}},"deep":{deep}}}}}"#
    ));
    let context = ComparisonContext::new(root.clone());
    let selected = context.schema(root.at("/schemas/selected").unwrap().to_owned());
    let expected = input(r#"{"type":"string"}"#);
    assert_eq!(
        identical(
            Some(&selected),
            Some(&expected),
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    assert_eq!(
        compare(
            Direction::Input,
            &selected,
            &expected,
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
    assert_eq!(
        identical_with_references(
            Some(&selected),
            Some(&expected),
            &|_| false,
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    let invalid = context.schema(json(
        r##"{"$ref":"#/schemas/unused/$defs/a","not":{"$ref":"#/schemas/missing"}}"##,
    ));
    assert_eq!(
        identical(
            Some(&invalid),
            Some(&expected),
            &ComparisonLimits::default()
        ),
        Err(ComparisonFailure::MissingReference)
    );
    assert_eq!(
        identical(
            Some(&selected),
            Some(&expected),
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
}
#[test]
fn cancelled_and_refused_indexing_leave_contexts_usable() {
    let context = ComparisonContext::new(json(
        r##"{"$id":"https://example.test/root","$defs":{"request":{"$ref":"#s"},"s":{"$anchor":"s","type":"string"}}}"##,
    ));
    let selected = context.schema(context.document().at("/$defs/request").unwrap().to_owned());
    let expected = input(r#"{"$anchor":"s","type":"string"}"#);
    let limits = ComparisonLimits::default();
    for allowance in [1, 200, 2000] {
        assert!(matches!(
            identical(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits {
                    max_work: allowance,
                    ..limits
                }
            ),
            Err(ComparisonFailure::LimitExceeded(_))
        ));
    }
    assert_eq!(
        identical_with_control(
            Some(&selected),
            Some(&expected),
            &limits,
            &AtomicBool::new(true)
        ),
        Err(ComparisonFailure::Cancelled)
    );
    assert_eq!(
        identical(Some(&selected), Some(&expected), &limits),
        Ok(true)
    );
    assert!(!format!("{context:?}").contains("example.test"));
}
#[test]
fn retained_applicators_compare_normalized_targets_and_preserve_authored_data() {
    for content in [
        r##""not":{"$ref":"#/schemas/S"}"##,
        r##""if":{"$ref":"#/schemas/S"}"##,
        r##""then":{"$ref":"#/schemas/S"}"##,
        r##""else":{"$ref":"#/schemas/S"}"##,
        r##""dependentSchemas":{"p":{"$ref":"#/schemas/S"}}"##,
        r##""prefixItems":[{"$ref":"#/schemas/S"}]"##,
        r##""contains":{"$ref":"#/schemas/S"}"##,
        r##""patternProperties":{"p":{"$ref":"#/schemas/S"}}"##,
        r##""propertyNames":{"$ref":"#/schemas/S"}"##,
        r##""unevaluatedItems":{"$ref":"#/schemas/S"}"##,
        r##""unevaluatedProperties":{"$ref":"#/schemas/S"}"##,
        r##""$dynamicRef":"#/schemas/S""##,
    ] {
        let schema = json(&format!("{{{content}}}"));
        let a = ComparisonContext::new(json(
            r#"{"schemas":{"S":{"type":"string","description":"a"}}}"#,
        ))
        .schema(schema.clone());
        let b = ComparisonContext::new(json(r#"{"schemas":{"S":{"type":["string"]}}}"#))
            .schema(schema.clone());
        let c = ComparisonContext::new(json(r#"{"schemas":{"S":{"type":"number"}}}"#))
            .schema(schema.clone());
        let missing = ComparisonContext::new(json("{}")).schema(schema.clone());
        let cycle = ComparisonContext::new(json(
            r##"{"schemas":{"S":{"not":{"$ref":"#/schemas/S"}}}}"##,
        ))
        .schema(schema);
        assert_eq!(
            identical(Some(&a), Some(&b), &ComparisonLimits::default()),
            Ok(true),
            "{content}"
        );
        assert_eq!(
            identical(Some(&a), Some(&c), &ComparisonLimits::default()),
            Ok(false),
            "{content}"
        );
        assert_eq!(
            compare(Direction::Output, &a, &c, &ComparisonLimits::default())
                .unwrap()
                .verdict(),
            Verdict::Indeterminate
        );
        assert_eq!(
            identical(Some(&missing), Some(&missing), &ComparisonLimits::default()),
            Err(ComparisonFailure::MissingReference)
        );
        assert_eq!(
            identical(Some(&cycle), Some(&cycle), &ComparisonLimits::default()),
            Err(ComparisonFailure::ReferenceCycle)
        );
    }
    let opaque = input(r#"{"not":{"const":{"$ref":"not a uri"}},"unknown":{"$ref":"not a uri"}}"#);
    assert_eq!(
        identical(Some(&opaque), Some(&opaque), &ComparisonLimits::default()),
        Ok(true)
    );
}

#[test]
fn cancelled_resource_discovery_does_not_hide_ambiguous_targets_on_retry() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Control {
        count: AtomicUsize,
        cancel: usize,
    }
    impl openbindings::WorkControl for Control {
        fn is_cancelled(&self) -> bool {
            self.count.fetch_add(1, Ordering::Relaxed) >= self.cancel
        }
    }
    let root = json(
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"a":{"$id":"https://example.test/s"},"b":{"$id":"https://example.test/s"}}}"#,
    );
    let reference = json(r#"{"$ref":"https://example.test/s"}"#);
    let fresh = || ComparisonContext::new(root.clone()).schema(reference.clone());
    let expected = input("true");
    let baseline = Control {
        count: AtomicUsize::new(0),
        cancel: usize::MAX,
    };
    assert_eq!(
        identical_with_control(
            Some(&fresh()),
            Some(&expected),
            &ComparisonLimits::default(),
            &baseline
        ),
        Err(ComparisonFailure::InvalidReference)
    );
    let count = baseline.count.load(Ordering::Relaxed);
    for cancel in [count / 3, count / 2, count - 1] {
        let selected = fresh();
        let control = Control {
            count: AtomicUsize::new(0),
            cancel,
        };
        assert_eq!(
            identical_with_control(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits::default(),
                &control
            ),
            Err(ComparisonFailure::Cancelled)
        );
        assert_eq!(
            identical(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits::default()
            ),
            Err(ComparisonFailure::InvalidReference)
        );
    }
}

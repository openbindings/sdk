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
fn shared_record_interfaces_fit_default_limits() {
    let named = (0..40)
        .map(|i| {
            format!(
                r#""S{i}":{{"type":"object","description":"Shared record {i}.","properties":{{"id":{{"type":"string","minLength":1,"maxLength":64}},"name":{{"type":"string","maxLength":200}},"count":{{"type":"integer","minimum":0}},"ratio":{{"type":"number","minimum":0,"maximum":1}},"tags":{{"type":"array","items":{{"type":"string"}},"maxItems":20}},"state":{{"enum":["open","closed","pending"]}},"created":{{"type":"string","format":"date-time"}},"active":{{"type":"boolean"}}}},"required":["id","name"],"additionalProperties":false}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let count = 400;
    let operations = (0..count)
            .map(|i| {
                let s = i % 40;
                format!(
                    r##""op{i:04}":{{"input":{{"type":"object","properties":{{"id":{{"type":"string"}},"limit":{{"type":"integer","minimum":1,"maximum":100}},"filter":{{"$ref":"#/schemas/S{s}"}}}},"required":["id"]}},"output":{{"type":"object","properties":{{"items":{{"type":"array","items":{{"$ref":"#/schemas/S{s}"}}}},"next":{{"type":"string"}}}}}}}}"##
                )
            })
            .collect::<Vec<_>>()
            .join(",");
    let document = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{{operations}}},"schemas":{{{named}}}}}"#
    ));
    let report = check_interface(
        &document,
        &document,
        &ComparisonContext::new(document.document().value().clone()),
        &ComparisonContext::new(document.document().value().clone()),
        &|_, _| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(report.operations().len(), count);
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
        (identical(
            Some(&invalid),
            Some(&expected),
            &ComparisonLimits::default()
        ))
        .map_err(|failure| failure.kind()),
        Err(ComparisonFailureKind::MissingReference)
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
            (identical(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits {
                    max_work: Some(allowance),
                    ..limits
                }
            ))
            .map_err(|failure| failure.kind()),
            Err(ComparisonFailureKind::LimitExceeded(_))
        ));
    }
    assert_eq!(
        (identical_with_control(
            Some(&selected),
            Some(&expected),
            &limits,
            &AtomicBool::new(true)
        ))
        .map_err(|failure| failure.kind()),
        Err(ComparisonFailureKind::Cancelled)
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
            (identical(Some(&missing), Some(&missing), &ComparisonLimits::default()))
                .map_err(|failure| failure.kind()),
            Err(ComparisonFailureKind::MissingReference)
        );
        assert_eq!(
            (identical(Some(&cycle), Some(&cycle), &ComparisonLimits::default()))
                .map_err(|failure| failure.kind()),
            Err(ComparisonFailureKind::ReferenceCycle)
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
        (identical_with_control(
            Some(&fresh()),
            Some(&expected),
            &ComparisonLimits::default(),
            &baseline
        ))
        .map_err(|failure| failure.kind()),
        Err(ComparisonFailureKind::InvalidReference)
    );
    let count = baseline.count.load(Ordering::Relaxed);
    for cancel in [count / 3, count / 2, count - 1] {
        let selected = fresh();
        let control = Control {
            count: AtomicUsize::new(0),
            cancel,
        };
        assert_eq!(
            (identical_with_control(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits::default(),
                &control
            ))
            .map_err(|failure| failure.kind()),
            Err(ComparisonFailureKind::Cancelled)
        );
        assert_eq!(
            (identical(
                Some(&selected),
                Some(&expected),
                &ComparisonLimits::default()
            ))
            .map_err(|failure| failure.kind()),
            Err(ComparisonFailureKind::InvalidReference)
        );
    }
}

fn at(context: &ComparisonContext, pointer: &str) -> SchemaInput {
    context.schema(context.document().at(pointer).unwrap().to_owned())
}

#[test]
fn carrier_selections_resolve_private_declarations_and_document_pointers() {
    for (root, detached_result) in [
        (
            r##"{"components":{"P":{"$id":"https://example.test/pet","$defs":{"a":{"type":"string"}},"$ref":"#/$defs/a"}}}"##,
            Ok(true),
        ),
        (
            r##"{"components":{"P":{"$defs":{"a":{"$anchor":"a","type":"string"}},"$ref":"#a"}}}"##,
            Ok(true),
        ),
        (
            r##"{"components":{"P":{"$id":"https://example.test/pet","$defs":{"a":{"$anchor":"a","type":"string"}},"$ref":"#a"}}}"##,
            Ok(true),
        ),
        (
            r##"{"components":{"P":{"$ref":"#/components/Q"},"Q":{"type":"string"}}}"##,
            Err(ComparisonFailureKind::MissingReference),
        ),
    ] {
        for warmed in [false, true] {
            for reverse in [false, true] {
                let context = ComparisonContext::new(json(root));
                let selected = at(&context, "/components/P");
                let detached =
                    ComparisonContext::new(json("{}")).schema(json(selected.schema().text()));
                if warmed {
                    assert_eq!(
                        identical(
                            Some(&selected),
                            Some(&selected),
                            &ComparisonLimits::default()
                        ),
                        Ok(true)
                    );
                }
                for (schema, result) in [(&selected, Ok(true)), (&detached, detached_result)] {
                    assert_eq!(
                        identical(Some(schema), Some(schema), &ComparisonLimits::default())
                            .map_err(|e| e.kind()),
                        result
                    );
                    assert_eq!(
                        identical_with_references(
                            Some(schema),
                            Some(schema),
                            &|_| false,
                            &ComparisonLimits::default()
                        )
                        .map_err(|e| e.kind()),
                        result
                    );
                }
                let (a, b) = if reverse {
                    (&detached, &selected)
                } else {
                    (&selected, &detached)
                };
                assert_eq!(
                    identical(Some(a), Some(b), &ComparisonLimits::default()).map_err(|e| e.kind()),
                    detached_result
                );
                assert_eq!(
                    identical_with_references(
                        Some(a),
                        Some(b),
                        &|_| false,
                        &ComparisonLimits::default()
                    )
                    .map_err(|e| e.kind()),
                    detached_result
                );
            }
        }
    }
}

#[test]
fn root_declarations_are_independent_of_selections_and_argument_order() {
    for (root, a, b, expected) in [
        (
            r##"{"components":{"X":{"$anchor":"foo","type":"string"},"Y":{"$ref":"#foo"}}}"##,
            "/components/X",
            "/components/Y",
            Err(ComparisonFailureKind::MissingReference),
        ),
        (
            r##"{"$defs":{"X":{"$anchor":"foo","type":"string"},"Y":{"$ref":"#foo"}}}"##,
            "/$defs/X",
            "/$defs/Y",
            Ok(true),
        ),
        (
            r##"[{"$ref":"#x","$defs":{"d":{"$anchor":"x","type":"string"}}},{"$ref":"#y","$defs":{"d":{"$anchor":"y","type":"string"}}}]"##,
            "/0",
            "/1",
            Ok(false),
        ),
        (
            r##"{"components":{"P":{"$defs":{"a":{"$anchor":"a","type":"string"}},"$ref":"#a"},"Q":{"$ref":"#a"}}}"##,
            "/components/P",
            "/components/Q",
            Err(ComparisonFailureKind::MissingReference),
        ),
        (
            r##"{"components":{"P":{"$id":"https://example.test/pet","$defs":{"a":{"$anchor":"a","type":"string"}},"$ref":"#a"},"Q":{"$ref":"https://example.test/pet#a"}}}"##,
            "/components/P",
            "/components/Q",
            Err(ComparisonFailureKind::MissingReference),
        ),
    ] {
        for reverse in [false, true] {
            let reused = ComparisonContext::new(json(root));
            let warm = at(&reused, a);
            let _ = identical(Some(&warm), Some(&warm), &ComparisonLimits::default());
            for context in [&reused, &ComparisonContext::new(json(root))] {
                let (a, b) = (at(context, a), at(context, b));
                let (a, b) = if reverse { (&b, &a) } else { (&a, &b) };
                assert_eq!(
                    identical(Some(a), Some(b), &ComparisonLimits::default()).map_err(|e| e.kind()),
                    expected
                );
                assert_eq!(
                    identical_with_references(
                        Some(a),
                        Some(b),
                        &|_| false,
                        &ComparisonLimits::default()
                    )
                    .map_err(|e| e.kind()),
                    expected
                );
            }
        }
    }
}

#[test]
fn ambiguous_identifiers_fail_after_selection_and_in_both_orders() {
    let root = r#"{"openbindings":"0.2.0","operations":{},"schemas":{"a":{"$id":"https://example.test/S","type":"string"},"b":{"$id":"https://example.test/S","type":"number"}}}"#;
    for warmed in [false, true] {
        for reverse in [false, true] {
            let context = ComparisonContext::new(json(root));
            let a = at(&context, "/schemas/a");
            if warmed {
                assert_eq!(
                    identical(Some(&a), Some(&a), &ComparisonLimits::default()),
                    Ok(true)
                );
            }
            let r = context.schema(json(r#"{"$ref":"https://example.test/S"}"#));
            for other in [&a, &r] {
                let (a, b) = if reverse { (&r, other) } else { (other, &r) };
                assert_eq!(
                    (identical(Some(a), Some(b), &ComparisonLimits::default()))
                        .map_err(|failure| failure.kind()),
                    Err(ComparisonFailureKind::InvalidReference)
                );
                assert_eq!(
                    (identical_with_references(
                        Some(a),
                        Some(b),
                        &|_| false,
                        &ComparisonLimits::default()
                    ))
                    .map_err(|failure| failure.kind()),
                    Err(ComparisonFailureKind::InvalidReference)
                );
            }
        }
    }
}

#[test]
fn selected_source_data_never_publishes_schema_identifiers() {
    let root = r#"{"openbindings":"0.2.0","operations":{},"sources":{"x":{"kind":"k","data":{"$id":"https://example.test/leak","type":"string"}}},"schemas":{"r":{"$ref":"https://example.test/leak"}}}"#;
    for warmed in [false, true] {
        for reverse in [false, true] {
            let context = ComparisonContext::new(json(root));
            let data = at(&context, "/sources/x/data");
            if warmed {
                assert_eq!(
                    identical(Some(&data), Some(&data), &ComparisonLimits::default()),
                    Ok(true)
                );
            }
            let r = at(&context, "/schemas/r");
            let (a, b) = if reverse { (&r, &data) } else { (&data, &r) };
            assert_eq!(
                (identical(Some(a), Some(b), &ComparisonLimits::default()))
                    .map_err(|failure| failure.kind()),
                Err(ComparisonFailureKind::MissingReference)
            );
            assert_eq!(
                (identical_with_references(
                    Some(a),
                    Some(b),
                    &|_| false,
                    &ComparisonLimits::default()
                ))
                .map_err(|failure| failure.kind()),
                Err(ComparisonFailureKind::MissingReference)
            );
        }
    }
}

#[test]
fn shared_values_keep_each_retrieval_resource_scope() {
    let common = json(r#"{"$ref":"child"}"#);
    let context = ComparisonContext::new(json(r#"{"properties":{"a":{"$ref":"https://example.test/a/root"},"b":{"$ref":"https://example.test/b/root"}}}"#))
        .with_resource("https://example.test/a/root", common.clone())
        .with_resource("https://example.test/b/root", common)
        .with_resource("https://example.test/a/child", json(r#"{"type":"string"}"#))
        .with_resource("https://example.test/b/child", json(r#"{"type":"number"}"#));
    let a = context.schema(context.document().clone());
    let b = input(r#"{"properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#);
    for (a, b) in [(&a, &b), (&b, &a)] {
        assert_eq!(
            identical(Some(a), Some(b), &ComparisonLimits::default()),
            Ok(false)
        );
        assert_eq!(
            identical_with_references(Some(a), Some(b), &|_| false, &ComparisonLimits::default()),
            Ok(false)
        );
        assert_eq!(
            compare(Direction::Input, b, a, &ComparisonLimits::default())
                .unwrap()
                .verdict(),
            Verdict::Incompatible
        );
    }
}

#[test]
fn detached_declarations_do_not_survive_a_call() {
    let context = ComparisonContext::new(json("{}"));
    let detached = context.schema(json(
        r#"{"$id":"https://example.test/detached","type":"string"}"#,
    ));
    assert_eq!(
        identical(
            Some(&detached),
            Some(&detached),
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    drop(detached);
    let reference = context.schema(json(r#"{"$ref":"https://example.test/detached"}"#));
    assert_eq!(
        (identical(
            Some(&reference),
            Some(&reference),
            &ComparisonLimits::default()
        ))
        .map_err(|failure| failure.kind()),
        Err(ComparisonFailureKind::MissingReference)
    );
}

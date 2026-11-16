use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_schema_comparison::*;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
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
fn comparison_rejects_reference_into_instance_data() {
    let required = doc(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://schemas.test/external#/$defs/Use"}}}}"#,
    );
    let provided =
        doc(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"string"}}}}"#);
    let external = json(r##"{"$defs":{"Use":{"$ref":"#/const"}},"const":{"type":"string"}}"##);
    let rc = ComparisonContext::new(required.document().value().clone())
        .with_resource("https://schemas.test/external", external);
    let pc = ComparisonContext::new(provided.document().value().clone());
    let report = check_interface(
        &required,
        &provided,
        &rc,
        &pc,
        &|_, _| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(
        report.operations()[0]
            .input()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .kind(),
        ComparisonFailureKind::MissingReference
    );
}

#[test]
fn caller_base_cannot_make_document_resource_externally_addressable() {
    let required = doc(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://schemas.test/external"}}},"schemas":{"Private":{"type":"string"}}}"#,
    );
    let provided =
        doc(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"string"}}}}"#);
    let external = json(r#"{"$ref":"https://retrieval.test/obi#/schemas/Private"}"#);
    let rc = ComparisonContext::new(required.document().value().clone())
        .with_resource("https://schemas.test/external", external);
    let pc = ComparisonContext::new(provided.document().value().clone());
    let run = |context: &ComparisonContext| {
        check_interface(
            &required,
            &provided,
            context,
            &pc,
            &|_, _| Some(OperationRole::Realized),
            &ComparisonLimits::default(),
        )
        .unwrap()
    };
    let no_base = run(&rc);
    let with_base = run(&rc.with_base("https://retrieval.test/obi"));
    for report in [no_base, with_base] {
        assert_eq!(
            report.operations()[0]
                .input()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .kind(),
            ComparisonFailureKind::MissingReference
        );
    }
}

fn assert_missing(input: &SchemaInput) {
    use std::sync::atomic::AtomicBool;
    let limits = ComparisonLimits::default();
    let control = AtomicBool::new(false);
    let plain = json(r#"{"type":"string"}"#);
    let plain = SchemaInput::new(plain.clone(), plain);
    for direction in [Direction::Input, Direction::Output] {
        for (target, candidate) in [(input, &plain), (&plain, input)] {
            assert_eq!(
                compare(direction, target, candidate, &limits)
                    .unwrap_err()
                    .kind(),
                ComparisonFailureKind::MissingReference
            );
        }
    }
    for result in [
        identical(Some(input), Some(input), &limits),
        identical_with_control(Some(input), Some(input), &limits, &control),
        identical_with_references(Some(input), Some(input), &|_| false, &limits),
        identical_with_references_and_control(
            Some(input),
            Some(input),
            &|_| false,
            &limits,
            &control,
        ),
    ] {
        assert_eq!(
            result.unwrap_err().kind(),
            ComparisonFailureKind::MissingReference
        );
    }
}

#[test]
fn references_to_data_fail_in_supplied_resources_and_document_schemas() {
    for (data, pointer) in [
        (r#""const":{"type":"string"}"#, "/const"),
        (r#""enum":[{"type":"string"}]"#, "/enum/0"),
        (r#""default":{"type":"string"}"#, "/default"),
        (r#""examples":[{"type":"string"}]"#, "/examples/0"),
        (r#""unknown":{"type":"string"}"#, "/unknown"),
        (
            r#""x-data":{"$defs":{"S":{"type":"string"}}}"#,
            "/x-data/$defs/S",
        ),
        (r#""properties":{"p":true}"#, "/properties"),
        (r#""items":[{"type":"string"}]"#, "/items/0"),
        (r#""dependencies":{"p":["string"]}"#, "/dependencies/p"),
        (r#""properties":{"p":null}"#, "/properties/p"),
    ] {
        for embedded in [false, true] {
            let prefix = if embedded { "/schemas/R" } else { "" };
            for keyword in ["$ref", "$dynamicRef"] {
                let resource = format!(
                    r##"{{"$defs":{{"Use":{{"{keyword}":"#{prefix}{pointer}"}}}},{data}}}"##
                );
                let input = if embedded {
                    let root = json(&format!(
                        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"R":{resource}}}}}"#
                    ));
                    ComparisonContext::new(root.clone())
                        .schema(root.at("/schemas/R/$defs/Use").unwrap().to_owned())
                } else {
                    let schema = json(r#"{"$ref":"https://schemas.test/external#/$defs/Use"}"#);
                    SchemaInput::new(schema.clone(), schema)
                        .with_resource("https://schemas.test/external", json(&resource))
                };
                assert_missing(&input);
            }
        }
    }
    for pointer in ["#", "#/schemas", "#/operations", "#/sources/data"] {
        let root =
            json(r#"{"openbindings":"0.2.0","operations":{},"schemas":{},"sources":{"data":{}}}"#);
        assert_missing(
            &ComparisonContext::new(root).schema(json(&format!(r#"{{"$ref":"{pointer}"}}"#))),
        );
    }
    let root = json(r##"{"components":{"P":{"$ref":"#/components/Q"},"Q":{"type":"string"}}}"##);
    assert_missing(
        &ComparisonContext::new(root.clone()).schema(root.at("/components/P").unwrap().to_owned()),
    );
}

#[test]
fn schema_targets_keep_working_in_both_resource_frames() {
    for (body, pointer) in [
        (r#""$defs":{"S":{"type":"string"}}"#, "/$defs/S"),
        (r#""properties":{"p":{"type":"string"}}"#, "/properties/p"),
        (
            r#""properties":{"items":{"type":"string"}}"#,
            "/properties/items",
        ),
        (r#""items":{"type":"string"}"#, "/items"),
        (r#""prefixItems":[{"type":"string"}]"#, "/prefixItems/0"),
        (
            r#""dependencies":{"p":{"type":"string"}}"#,
            "/dependencies/p",
        ),
        (
            r##""$defs":{"S":{"$id":"https://schemas.test/nested","$defs":{"V":{"$ref":"#/$defs/Actual"},"Actual":{"type":"string"}}}}"##,
            "/$defs/S/$defs/V",
        ),
    ] {
        for embedded in [false, true] {
            let prefix = if embedded { "/schemas/R" } else { "" };
            let schema = json(&format!(r##"{{"$ref":"#{prefix}{pointer}"}}"##));
            let root = if embedded {
                json(&format!(
                    r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"R":{{{body}}}}}}}"#
                ))
            } else {
                json(&format!("{{{body}}}"))
            };
            let input = if embedded {
                ComparisonContext::new(root).schema(schema)
            } else {
                let reference = json(&format!(
                    r#"{{"$ref":"https://schemas.test/external#{pointer}"}}"#
                ));
                SchemaInput::new(reference.clone(), reference)
                    .with_resource("https://schemas.test/external", root)
            };
            let plain = json(r#"{"type":"string"}"#);
            let plain = SchemaInput::new(plain.clone(), plain);
            let expected = &plain;
            assert!(identical(Some(&input), Some(expected), &ComparisonLimits::default()).unwrap());
            for direction in [Direction::Input, Direction::Output] {
                assert_eq!(
                    compare(direction, &input, expected, &ComparisonLimits::default())
                        .unwrap()
                        .verdict(),
                    Verdict::Compatible
                );
            }
            assert!(
                identical_with_references(
                    Some(&input),
                    Some(expected),
                    &|_| false,
                    &ComparisonLimits::default()
                )
                .unwrap()
            );
        }
    }
}

#[test]
fn caller_base_is_ignored_from_document_and_identified_resources() {
    for declaring in ["", r#""$id":"https://schemas.test/own","#] {
        let root = json(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"Private":{{"type":"string"}},"Use":{{{declaring}"$ref":"https://retrieval.test/obi#/schemas/Private"}}}}}}"#
        ));
        let selected = root.at("/schemas/Use").unwrap().to_owned();
        let context = ComparisonContext::new(root);
        assert_missing(&context.schema(selected.clone()));
        assert_missing(
            &context
                .with_base("https://retrieval.test/obi")
                .schema(selected),
        );
    }
    let root = json(
        r#"{"$defs":{"Private":{"type":"string"}},"$ref":"https://retrieval.test/schema#/$defs/Private"}"#,
    );
    let input = SchemaInput::new(root.clone(), root).with_base("https://retrieval.test/schema");
    assert!(identical(Some(&input), Some(&input), &ComparisonLimits::default()).unwrap());
}

#[test]
fn relative_references_use_embedded_resource_bases_once() {
    let limits = ComparisonLimits::default();
    let plain = json(r#"{"type":"string"}"#);
    let plain = SchemaInput::new(plain.clone(), plain);
    for (id, nested_id, reference, root_reference) in [
        ("sub/", "b", "b", "sub/"),
        ("https://probe.test/sub/", "b", "b", "sub/"),
        ("sub/", "b", "https://probe.test/sub/b", "sub/"),
        ("sub/", "b", "#/$defs/b", "sub/"),
        ("sub/", "b", "b", "sub/b"),
        ("sub", "b", "b", "sub"),
        ("sub/", "https://probe.test/sub/b", "b", "sub/"),
    ] {
        let root = json(&serde_json::json!({
            "$defs": {"a": {"$id": id, "$defs": {"b": {"$id": nested_id, "type": "string"}}, "$ref": reference}},
            "$ref": root_reference
        }).to_string());
        for decoy in [false, true] {
            let mut context =
                ComparisonContext::new(root.clone()).with_base("https://probe.test/root");
            if decoy && id != "sub" {
                context =
                    context.with_resource("https://probe.test/b", json(r#"{"type":"number"}"#));
            }
            let input = context.schema(root.clone());
            for _ in 0..2 {
                // Cold and completed cached indexes agree.
                assert!(
                    identical(Some(&input), Some(&input), &limits).unwrap_or_else(|e| panic!(
                        "{id}/{nested_id}/{reference}/{root_reference}/{decoy}: {e:?}"
                    ))
                );
                for direction in [Direction::Input, Direction::Output] {
                    for (target, candidate) in [(&input, &plain), (&plain, &input)] {
                        assert_eq!(
                            compare(direction, target, candidate, &limits)
                                .unwrap()
                                .verdict(),
                            Verdict::Indeterminate
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn detached_duplicate_declarations_are_ambiguous_even_with_equal_content() {
    let limits = ComparisonLimits::default();
    let plain = json(r#"{"type":"integer"}"#);
    let plain = SchemaInput::new(plain.clone(), plain);
    for text in [
        r#"{"$defs":{"a":{"$id":"https://other.test/a","type":"integer","maximum":5}},"$ref":"https://other.test/a"}"#,
        r#"{"$id":"https://other.test/root","$defs":{"a":{"$id":"a","type":"integer","maximum":5}},"$ref":"a"}"#,
        r##"{"$dynamicAnchor":"a","$defs":{"use":{"$ref":"#a"}},"type":"integer","maximum":5}"##,
    ] {
        let root = json(text);
        let context = ComparisonContext::new(root.clone()).with_base("https://probe.test/root");
        let pointer = if root.get("$dynamicAnchor").is_some() {
            "/$defs/use"
        } else {
            ""
        };
        let select = |value: &JsonValue| {
            if pointer.is_empty() {
                value.clone()
            } else {
                value.at(pointer).unwrap().to_owned()
            }
        };
        let contained = context.schema(select(&root));
        let detached = context.schema(select(&json(text)));
        for _ in 0..2 {
            for direction in [Direction::Input, Direction::Output] {
                for (target, candidate) in [(&detached, &plain), (&plain, &detached)] {
                    assert_eq!(
                        compare(direction, target, candidate, &limits)
                            .unwrap_err()
                            .kind(),
                        ComparisonFailureKind::InvalidReference
                    );
                }
                // The anchor control contains a reference cycle; its identity
                // policy is independent of the detached declaration collision.
                if pointer.is_empty() {
                    assert_eq!(
                        compare(direction, &contained, &plain, &limits)
                            .unwrap()
                            .verdict(),
                        Verdict::Indeterminate
                    );
                }
            }
        }
    }
    // A detached root's own ID establishes its private resource before lookup.
    let root = json(r#"{"$id":"https://other.test/root","type":"integer","maximum":5}"#);
    let context = ComparisonContext::new(root.clone());
    let detached = context.schema(json(root.text()));
    assert_eq!(
        compare(Direction::Input, &detached, &plain, &limits)
            .unwrap()
            .verdict(),
        Verdict::Indeterminate
    );
}

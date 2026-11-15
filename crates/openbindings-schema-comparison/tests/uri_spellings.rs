use openbindings::{ExternalSchema, ExternalSchemas, JsonValue, ValueOutcome, validate_document};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_schema_comparison::*;

const INTEGER: &str = r#"{"type":"integer"}"#;
const RESOURCE: &str = r#"{"type":"integer","$defs":{"x":{"type":"integer"}}}"#;

fn comparison(context: ComparisonContext) -> Result<Verdict, ComparisonFailureKind> {
    let schema = context
        .document()
        .at("/operations/o/input")
        .unwrap()
        .to_owned();
    let value = JsonValue::parse(INTEGER).unwrap();
    compare(
        Direction::Input,
        &context.schema(schema),
        &SchemaInput::new(value.clone(), value),
        &ComparisonLimits::default(),
    )
    .map(|report| report.verdict())
    .map_err(|failure| failure.kind())
}

fn document(reference: &str, resource: Option<&str>) -> String {
    let schemas = resource.map_or(String::new(), |id| {
        format!(
            r#", "schemas":{{"h":{{"$id":"{id}","type":"integer","$defs":{{"x":{INTEGER}}}}}}}"#
        )
    });
    format!(
        r#"{{"openbindings":"0.2.0","operations":{{"o":{{"input":{{"$ref":"{reference}"}}}}}}{schemas}}}"#
    )
}

fn evaluator(document: &str, resources: &ExternalSchemas) -> bool {
    let checked = validate_document(document.as_bytes()).unwrap();
    let Some(proof) = checked.conformant() else {
        return false;
    };
    let operation = proof.resolve_operation("o").unwrap();
    match operation
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new(), resources)
    {
        Ok(prepared) => {
            let mut resolved = None;
            for evaluate in [false, true] {
                let run = |text| {
                    let value = JsonValue::parse(text).unwrap();
                    if evaluate {
                        prepared.evaluate(&value)
                    } else {
                        prepared.explain(&value)
                    }
                };
                let one = run("1");
                let found = matches!(one, ValueOutcome::Satisfies);
                if found {
                    assert!(matches!(run(r#""a""#), ValueOutcome::Fails { .. }));
                } else {
                    assert!(
                        matches!(
                            one,
                            ValueOutcome::NoVerdict { .. } | ValueOutcome::Undefined { .. }
                        ),
                        "{one:?}"
                    );
                }
                if let Some(prior) = resolved {
                    assert_eq!(found, prior);
                }
                resolved = Some(found);
            }
            resolved.unwrap()
        }
        Err(_) => false,
    }
}

#[test]
fn uri_spellings_agree_with_the_evaluator() {
    // The evaluator is the authority for each spelling, including URL-specific
    // normalizations performed by its pinned URI library.
    let cases = [
        (
            "exact",
            "https://e.test/n",
            "https://e.test/n#/$defs/x",
            true,
        ),
        (
            "scheme case",
            "https://e.test/n",
            "HTTPS://e.test/n#/$defs/x",
            true,
        ),
        (
            "host case",
            "https://e.test/n",
            "https://E.TEST/n#/$defs/x",
            true,
        ),
        (
            "scheme and host case",
            "https://e.test/n",
            "HTTPS://E.TEST/n#/$defs/x",
            true,
        ),
        (
            "path unreserved",
            "https://e.test/n",
            "https://e.test/%6E#/$defs/x",
            true,
        ),
        (
            "hex case",
            "https://e.test/%6E",
            "https://e.test/%6e#/$defs/x",
            true,
        ),
        (
            "query unreserved",
            "https://e.test/n?q=n",
            "https://e.test/n?q=%6E#/$defs/x",
            true,
        ),
        (
            "query hex case",
            "https://e.test/n?q=%2F",
            "https://e.test/n?q=%2f#/$defs/x",
            true,
        ),
        (
            "path reserved hex case",
            "https://e.test/a%2Fb",
            "https://e.test/a%2fb#/$defs/x",
            true,
        ),
        (
            "reserved slash",
            "https://e.test/a/b",
            "https://e.test/a%2Fb#/$defs/x",
            false,
        ),
        (
            "default port",
            "https://e.test/n",
            "https://e.test:443/n#/$defs/x",
            true,
        ),
        (
            "empty path",
            "https://e.test",
            "https://e.test/#/$defs/x",
            false,
        ),
        (
            "dot segments",
            "https://e.test/n",
            "https://e.test/a/../n#/$defs/x",
            true,
        ),
        (
            "encoded dots",
            "https://e.test/n",
            "https://e.test/a/%2e%2E/n#/$defs/x",
            true,
        ),
        (
            "trailing hash",
            "https://e.test/n#",
            "https://e.test/n#/$defs/x",
            true,
        ),
        (
            "encoded fragment",
            "https://e.test/n",
            "https://e.test/n#%2F%24defs%2Fx",
            true,
        ),
        (
            "local encoded fragment",
            "https://e.test/n",
            "#%2Fschemas%2Fh%2F%24defs%2Fx",
            true,
        ),
        (
            "trailing reference hash",
            "https://e.test/n",
            "https://e.test/n#",
            true,
        ),
        (
            "absent fragment",
            "https://e.test/n",
            "https://e.test/n",
            true,
        ),
        (
            "fragment unreserved",
            "https://e.test/n",
            "https://e.test/n#/$defs/%78",
            true,
        ),
        (
            "fragment decoded once",
            "https://e.test/n",
            "https://e.test/n#/$defs/%2578",
            false,
        ),
        (
            "fragment invalid UTF-8",
            "https://e.test/n",
            "https://e.test/n#%FF",
            false,
        ),
        (
            "rootless URI dots",
            "urn:a/../b",
            "urn:a/../b#/$defs/x",
            true,
        ),
        (
            "rootless resolved URI",
            "urn:a/../b",
            "urn:/b#/$defs/x",
            true,
        ),
        (
            "mixed dots resolve before normalization",
            "https://e.test/a/b",
            "https://e.test/a/%2e%2e/../b#/$defs/x",
            true,
        ),
        (
            "mixed dots do not overnormalize",
            "https://e.test/b",
            "https://e.test/a/%2e%2e/../b#/$defs/x",
            false,
        ),
    ];
    let mut agreements = Vec::new();
    for (label, id, reference, expected) in cases {
        let text = document(reference, Some(id));
        let evaluated = evaluator(&text, &ExternalSchemas::default());
        let compared = comparison(ComparisonContext::new(JsonValue::parse(&text).unwrap()));
        println!("{label}: evaluator={evaluated}, comparison={compared:?}");
        agreements.push((label, evaluated, compared, expected));
    }
    for (label, evaluated, compared, expected) in agreements {
        assert_eq!(evaluated, expected, "{label}");
        assert_eq!(compared.is_ok(), evaluated, "{label}: {compared:?}");
        if evaluated {
            let verdict = if matches!(label, "trailing reference hash" | "absent fragment") {
                Verdict::Indeterminate
            } else {
                Verdict::Compatible
            };
            assert_eq!(compared, Ok(verdict), "{label}");
        }
    }
}

#[test]
fn supplied_uri_spellings_agree_with_external_schemas() {
    let mut agreements = Vec::new();
    for (registered, reference) in [
        ("HTTPS://E.TEST/%6e?q=%6E", "https://e.test/n?q=n#/$defs/x"),
        ("https://e.test/n?q=n", "HTTPS://E.TEST/%6E?q=%6e#/$defs/x"),
    ] {
        let text = document(reference, None);
        let value = JsonValue::parse(RESOURCE).unwrap();
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: registered.into(),
            document: value.clone(),
        }])
        .unwrap();
        assert!(evaluator(&text, &resources));
        agreements.push(comparison(
            ComparisonContext::new(JsonValue::parse(&text).unwrap())
                .with_resource(registered, value),
        ));
    }
    for compared in agreements {
        assert_eq!(compared, Ok(Verdict::Compatible));
    }
}

#[test]
fn duplicate_registration_and_reached_ambiguity_agree_with_external_schemas() {
    let mut agreements = Vec::new();
    for other in [
        "https://e.test/n",
        "https://e.test/n#",
        "https://e.test/a/../n",
        "HTTPS://E.TEST/n",
        "https://e.test/%6E",
        "https://e.test:443/n",
        "https://e.test/other",
    ] {
        let text = document("https://e.test/n#/$defs/x", None);
        let value = JsonValue::parse(RESOURCE).unwrap();
        let resources =
            ExternalSchemas::new(["https://e.test/n", other].map(|uri| ExternalSchema {
                uri: uri.into(),
                document: value.clone(),
            }));
        let compared = comparison(
            ComparisonContext::new(JsonValue::parse(&text).unwrap())
                .with_resource("https://e.test/n", value.clone())
                .with_resource(other, value),
        );
        println!(
            "duplicate {other}: registration={:?}, comparison={compared:?}",
            resources.as_ref().map(|_| ()).map_err(|e| &e.kind)
        );
        let evaluated = resources
            .as_ref()
            .is_ok_and(|resources| evaluator(&text, resources));
        // Registering distinct spellings is allowed even when their canonical
        // aliases conflict. An unused conflict must not poison the comparison.
        let unused = r#"{"openbindings":"0.2.0","operations":{"o":{"input":{"type":"integer"}}}}"#;
        let value = JsonValue::parse(RESOURCE).unwrap();
        let unused_comparison = comparison(
            ComparisonContext::new(JsonValue::parse(unused).unwrap())
                .with_resource("https://e.test/n", value.clone())
                .with_resource(other, value),
        );
        assert_eq!(
            unused_comparison.is_ok(),
            resources.is_ok(),
            "registration: {other}"
        );
        if let Ok(resources) = &resources {
            assert!(evaluator(unused, resources));
        }
        agreements.push((other, compared, evaluated));
    }
    for (other, compared, evaluated) in agreements {
        assert_eq!(compared.is_ok(), evaluated, "{other}: {compared:?}");
    }
}

#[test]
fn initial_bases_and_declared_ids_use_the_same_identity_as_references() {
    let integer = JsonValue::parse(INTEGER).unwrap();
    let candidate = SchemaInput::new(integer.clone(), integer);
    for base in ["HTTPS://E.TEST/%6e?q=%6E", "https://e.test:443/n?q=n#"] {
        let reference = "https://e.test/n?q=n#/$defs/x";
        let value = JsonValue::parse(format!(
            r#"{{"$ref":"{reference}","$defs":{{"x":{INTEGER}}}}}"#
        ))
        .unwrap();
        let target = ComparisonContext::new(value.clone())
            .with_base(base)
            .schema(value);
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: base.into(),
            document: target.document().clone(),
        }])
        .unwrap();
        assert!(evaluator(&document(reference, None), &resources));
        assert_eq!(
            compare(
                Direction::Input,
                &target,
                &candidate,
                &ComparisonLimits::default()
            )
            .unwrap()
            .verdict(),
            Verdict::Compatible
        );

        // Exercise normalization on the declaration side as well as the ref.
        let text = document(reference, Some(base));
        assert!(evaluator(&text, &ExternalSchemas::default()));
        assert_eq!(
            comparison(ComparisonContext::new(JsonValue::parse(text).unwrap())),
            Ok(Verdict::Compatible)
        );
    }
}

#[test]
fn canonical_identity_preserves_the_authored_base_for_relative_resolution() {
    let text = r##"{"openbindings":"0.2.0","operations":{"o":{"input":{"$ref":"#/schemas/h"}}},"schemas":{"h":{"$id":"https://e.test/a/%2e%2e","$ref":"n#/$defs/x"},"t":{"$id":"https://e.test/a/n","$defs":{"x":{"type":"integer"}}}}}"##;
    assert!(evaluator(text, &ExternalSchemas::default()));
    assert_eq!(
        comparison(ComparisonContext::new(JsonValue::parse(text).unwrap())),
        Ok(Verdict::Indeterminate)
    );

    for (id, target_uri) in [
        (None, "https://e.test/a/n"),
        (Some("https://e.test/"), "https://e.test/n"),
    ] {
        let verdict = if id.is_some() {
            Verdict::Indeterminate
        } else {
            Verdict::Compatible
        };
        let declaration = id.map_or(String::new(), |id| format!(r#""$id":"{id}","#));
        let root = JsonValue::parse(format!(r##"{{{declaration}"$ref":"n#/$defs/x"}}"##)).unwrap();
        let resource = JsonValue::parse(RESOURCE).unwrap();
        let base = "https://e.test/a/%2e%2e";
        let resources = ExternalSchemas::new([
            ExternalSchema {
                uri: base.into(),
                document: root.clone(),
            },
            ExternalSchema {
                uri: target_uri.into(),
                document: resource.clone(),
            },
        ])
        .unwrap();
        let text = document(base, None);
        assert!(evaluator(&text, &resources));
        assert_eq!(
            comparison(
                ComparisonContext::new(JsonValue::parse(&text).unwrap())
                    .with_resource(base, root.clone())
                    .with_resource(target_uri, resource.clone())
            ),
            Ok(verdict)
        );
        let integer = JsonValue::parse(INTEGER).unwrap();
        let target = ComparisonContext::new(root.clone())
            .with_base(base)
            .with_resource(target_uri, resource)
            .schema(root);
        assert_eq!(
            compare(
                Direction::Input,
                &target,
                &SchemaInput::new(integer.clone(), integer),
                &ComparisonLimits::default()
            )
            .unwrap()
            .verdict(),
            verdict
        );
    }
}

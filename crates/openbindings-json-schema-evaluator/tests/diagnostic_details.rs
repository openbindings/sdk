use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare(text: &str, resources: ResourceSet) -> PreparedContract {
    let doc = Document::parse(text).unwrap();
    let doc = doc
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let evaluator = DefaultEvaluator::new();
    let operation = doc.resolve_operation("op").expect("declared operation");
    let Ok(contract) = operation
        .input()
        .expect("declared input")
        .prepare_with(&evaluator, &resources)
    else {
        panic!("not ready")
    };
    contract
}
/// Check a value cheaply, or explain it when `details` is set.
fn run(schema: &str, input: &str, details: bool) -> ValueOutcome {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let contract = prepare(&text, ResourceSet::default());
    let value = JsonValue::parse(input).unwrap();
    if details {
        contract.explain(&value)
    } else {
        contract.validate(&value)
    }
}
fn failure(result: &ValueOutcome) -> (&[ValueProblem], bool) {
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    (problems, *problems_complete)
}
#[test]
fn explained_locations_exist_in_original_document_and_supplied_resources() {
    let uri = "https://example.test/schema";
    for (schema, input, expected) in [
        (
            r#"{"dependentRequired":{"a":["b"]}}"#,
            r#"{"a":1}"#,
            "/dependentRequired",
        ),
        (
            r#"{"properties":{"a/~雪":{"dependentRequired":{"x":["y"]}}}}"#,
            r#"{"a/~雪":{"x":1}}"#,
            "/properties/a~1~0雪/dependentRequired",
        ),
        (
            r#"{"prefixItems":[{"dependentRequired":{"a":["b"]}}]}"#,
            r#"[{"a":1}]"#,
            "/prefixItems/0/dependentRequired",
        ),
        (
            r##"{"$id":"https://example.test/schema","$defs":{"dep":{"dependentRequired":{"a":["b"]}}},"$ref":"#/$defs/dep"}"##,
            r#"{"a":1}"#,
            "/$defs/dep/dependentRequired",
        ),
        (
            r#"{"dependentSchemas":{"a":{"required":["b"]}}}"#,
            r#"{"a":1}"#,
            "/dependentSchemas/a/required",
        ),
        (
            r#"{"contains":true,"minContains":2}"#,
            "[1]",
            "/minContains",
        ),
        (
            r#"{"contains":true,"maxContains":0}"#,
            "[1]",
            "/maxContains",
        ),
        (
            r#"{"propertyNames":{"pattern":"^a$"}}"#,
            r#"{"b":1}"#,
            "/propertyNames",
        ),
        (r#"{"unevaluatedItems":false}"#, "[1]", "/unevaluatedItems"),
        (
            r#"{"unevaluatedProperties":false}"#,
            r#"{"a":1}"#,
            "/unevaluatedProperties",
        ),
    ] {
        for external in [false, true] {
            let (entry, resources) = if external {
                (
                    format!(r#"{{"$ref":"{uri}"}}"#),
                    ResourceSet::new([SchemaResource {
                        uri: uri.into(),
                        document: JsonValue::parse(schema).unwrap(),
                    }])
                    .unwrap(),
                )
            } else {
                (schema.to_owned(), ResourceSet::default())
            };
            let text =
                format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{entry}}}}}}}"#);
            let value = JsonValue::parse(input).unwrap();
            let contract = prepare(&text, resources);
            let coarse = contract.validate(&value);
            assert!(failure(&coarse).0.is_empty());
            let upstream = contract.explain(&value);
            let location = &failure(&upstream).0[0].schema;
            assert_eq!(location.resource.as_deref(), external.then_some(uri));
            assert_eq!(
                location.pointer,
                if external {
                    expected.to_owned()
                } else {
                    format!("/operations/op/input{expected}")
                },
                "{schema}"
            );
            let source = JsonValue::parse(if external { schema } else { &text }).unwrap();
            assert!(source.at(location.pointer.as_str().unwrap()).is_some());
        }
    }
}

#[test]
fn exact_facts_private_defaults_and_recovery() {
    let cases = [
        (r#"{"type":"integer"}"#, r#""SECRET-instance""#, "type"),
        (r#"{"required":["SECRET/member~key"]}"#, "{}", "required"),
        (
            r#"{"minimum":9007199254740993}"#,
            "9007199254740992",
            "numeric-bound",
        ),
        (r#"{"maxLength":3}"#, r#""long""#, "size-bound"),
        (
            r#"{"enum":["SECRET-choice",9007199254740993,{"x":1e-999}]}"#,
            "null",
            "enum",
        ),
    ];
    for (schema, input, tag) in cases {
        let ordinary = run(schema, input, false);
        let (problems, complete) = failure(&ordinary);
        assert!(!complete);
        assert!(problems.is_empty());
        let enriched = run(schema, input, true);
        let (problems, complete) = failure(&enriched);
        assert!(!complete);
        let details = serde_json::to_value(problems[0].details.as_ref().unwrap()).unwrap();
        assert_eq!(details["kind"], tag);
        match problems[0].details.as_ref().unwrap() {
            ValueProblemDetails::Required { member } => {
                assert_eq!(member, "SECRET/member~key");
                assert_eq!(problems[0].instance, "");
            }
            ValueProblemDetails::NumericBound { bound } => assert_eq!(bound, "9007199254740993"),
            ValueProblemDetails::Enum { choices } => assert_eq!(
                choices,
                &[r#""SECRET-choice""#, "9007199254740993", r#"{"x":1e-999}"#]
            ),
            _ => (),
        }
    }
}
#[test]
fn external_old_snapshot_nested_coordinates_and_escaped_names() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    let resource = |schema: &str| {
        ResourceSet::new([SchemaResource {
            uri: "https://example.test/schema".into(),
            document: JsonValue::parse(schema).unwrap(),
        }])
        .unwrap()
    };
    let old = prepare(
        document,
        resource(r#"{"allOf":[{"properties":{"a/~":{"minimum":9007199254740993}}}]}"#),
    );
    let new = prepare(
        document,
        resource(r#"{"properties":{"a/~":{"minimum":7}}}"#),
    );
    let value = JsonValue::parse(r#"{"a/~":6}"#).unwrap();
    let result = old.explain(&value);
    let problem = &failure(&result).0[0];
    assert_eq!(problem.instance, "/a~1~0");
    assert_eq!(problem.schema.pointer, "/allOf/0/properties/a~1~0/minimum");
    assert_eq!(
        problem.schema.resource.as_deref(),
        Some("https://example.test/schema")
    );
    assert!(
        matches!(&problem.details,Some(ValueProblemDetails::NumericBound{bound}) if bound=="9007199254740993")
    );
    let result = new.explain(&value);
    assert!(
        matches!(&failure(&result).0[0].details,Some(ValueProblemDetails::NumericBound{bound}) if bound=="7")
    );
}
#[test]
fn every_bound_and_required_specialization_recovers_original_facts() {
    for (keyword, input) in [
        ("minimum", "0"),
        ("maximum", "2"),
        ("exclusiveMinimum", "1"),
        ("exclusiveMaximum", "1"),
        ("minLength", "\"\""),
        ("maxLength", "\"xx\""),
        ("minItems", "[]"),
        ("maxItems", "[0,1]"),
        ("minProperties", "{}"),
        ("maxProperties", "{\"a\":0,\"b\":0}"),
    ] {
        let result = run(&format!("{{\"{keyword}\":1e0}}"), input, true);
        let (problems, complete) = failure(&result);
        assert!(!complete, "{keyword}");
        assert_eq!(problems[0].code, keyword);
        match problems[0].details.as_ref().unwrap() {
            ValueProblemDetails::NumericBound { bound }
            | ValueProblemDetails::SizeBound { bound } => assert_eq!(bound, "1e0"),
            detail => panic!("{detail:?}"),
        }
    }
    for required in [
        vec!["a"],
        vec!["a", "b"],
        vec!["a", "b", "c"],
        vec!["a", "b", "c", "d"],
    ] {
        for missing in &required {
            let schema = serde_json::json!({"required": required}).to_string();
            let mut input = serde_json::Map::new();
            for member in &required {
                if member != missing {
                    input.insert((*member).to_owned(), serde_json::Value::Null);
                }
            }
            let result = run(&schema, &serde_json::Value::Object(input).to_string(), true);
            let (problems, complete) = failure(&result);
            assert!(!complete);
            assert_eq!(problems.len(), 1);
            assert!(
                matches!(&problems[0].details, Some(ValueProblemDetails::Required { member }) if member == missing)
            );
        }
    }
    let unsupported = run(r#"{"const":"private"}"#, "null", true);
    let (problems, complete) = failure(&unsupported);
    assert!(!complete);
    assert!(problems[0].details.is_none());
}

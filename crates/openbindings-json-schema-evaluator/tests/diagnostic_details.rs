use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare(text: &str, resources: ResourceSet) -> PreparedValueContract {
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

// Each keyword's failure names the keyword in the original schema, from the
// schema's own definition: every subschema position is one, so no keyword is
// reported at a position the original does not hold. Explanations add no schema
// facts; the location is where a reader finds them.
#[test]
fn every_keyword_is_explained_at_its_original_location() {
    for (schema, input, code, pointer, instance) in [
        (r#"{"type":"integer"}"#, r#""x""#, "type", "/type", ""),
        (r#"{"minimum":3}"#, "1", "minimum", "/minimum", ""),
        (
            r#"{"exclusiveMaximum":3}"#,
            "3",
            "exclusiveMaximum",
            "/exclusiveMaximum",
            "",
        ),
        (r#"{"multipleOf":3}"#, "4", "multipleOf", "/multipleOf", ""),
        (
            r#"{"maxLength":1}"#,
            r#""ab""#,
            "maxLength",
            "/maxLength",
            "",
        ),
        (
            r#"{"pattern":"^[a-z]+\\d$"}"#,
            r#""b""#,
            "pattern",
            "/pattern",
            "",
        ),
        (
            r#"{"uniqueItems":true}"#,
            "[1,1]",
            "uniqueItems",
            "/uniqueItems",
            "",
        ),
        (
            r#"{"contains":{"type":"string"},"maxContains":1}"#,
            r#"["a","b"]"#,
            "contains",
            "/maxContains",
            "",
        ),
        (r#"{"items":false}"#, "[1]", "falseSchema", "/items", "/0"),
        (
            r#"{"prefixItems":[{"type":"string"}]}"#,
            "[1]",
            "type",
            "/prefixItems/0/type",
            "/0",
        ),
        (
            r#"{"type":"array","minItems":1,"items":{"type":"string"}}"#,
            "[]",
            "minItems",
            "/minItems",
            "",
        ),
        (
            r#"{"prefixItems":[true],"unevaluatedItems":{"type":"string"}}"#,
            "[1,2]",
            "unevaluatedItems",
            "/unevaluatedItems",
            "",
        ),
        (
            r#"{"required":["a","b"]}"#,
            r#"{"a":1}"#,
            "required",
            "/required",
            "",
        ),
        (
            r#"{"maxProperties":0}"#,
            r#"{"a":1}"#,
            "maxProperties",
            "/maxProperties",
            "",
        ),
        (
            r#"{"properties":{"a":false}}"#,
            r#"{"a":1}"#,
            "falseSchema",
            "/properties/a",
            "/a",
        ),
        (
            r#"{"patternProperties":{"^a":{"type":"string"}}}"#,
            r#"{"a":1}"#,
            "type",
            "/patternProperties/^a/type",
            "/a",
        ),
        (
            r#"{"properties":{"b":true},"additionalProperties":{"type":"string"}}"#,
            r#"{"a":1}"#,
            "type",
            "/additionalProperties/type",
            "/a",
        ),
        (
            r#"{"propertyNames":{"maxLength":1}}"#,
            r#"{"ab":1}"#,
            "propertyNames",
            "/propertyNames",
            "",
        ),
        (
            r#"{"unevaluatedProperties":false}"#,
            r#"{"a":1}"#,
            "unevaluatedProperties",
            "/unevaluatedProperties",
            "",
        ),
        (
            r#"{"dependentSchemas":{"a":{"required":["b"]}}}"#,
            r#"{"a":1}"#,
            "required",
            "/dependentSchemas/a/required",
            "",
        ),
        (
            r#"{"allOf":[true,{"type":"string"}]}"#,
            "1",
            "type",
            "/allOf/1/type",
            "",
        ),
        (
            r#"{"anyOf":[{"type":"string"},{"type":"null"}]}"#,
            "1",
            "anyOf",
            "/anyOf",
            "",
        ),
        (
            r#"{"oneOf":[{"type":"integer"},{"minimum":0}]}"#,
            "1",
            "oneOf",
            "/oneOf",
            "",
        ),
        (r#"{"not":true}"#, "1", "not", "/not", ""),
        (
            r#"{"if":{"type":"integer"},"then":{"minimum":5}}"#,
            "1",
            "minimum",
            "/then/minimum",
            "",
        ),
        (
            r#"{"if":{"type":"integer"},"else":{"type":"null"}}"#,
            r#""x""#,
            "type",
            "/else/type",
            "",
        ),
        (r#"{"const":1}"#, "2", "const", "/const", ""),
        (r#"{"enum":[1,2]}"#, "3", "enum", "/enum", ""),
        ("false", "1", "falseSchema", "", ""),
        (
            r##"{"$ref":"#/operations/op/input/$defs/x","$defs":{"x":{"type":"string"}}}"##,
            "1",
            "type",
            "/$defs/x/type",
            "",
        ),
    ] {
        let result = run(schema, input, true);
        let (problems, complete) = failure(&result);
        assert!(!complete, "{schema}");
        assert_eq!(problems.len(), 1, "{schema}");
        let problem = &problems[0];
        assert_eq!(problem.code, code, "{schema}");
        assert_eq!(problem.instance, instance, "{schema}");
        assert_eq!(problem.schema.resource, None, "{schema}");
        assert_eq!(
            problem.schema.pointer,
            format!("/operations/op/input{pointer}"),
            "{schema}"
        );
        assert!(problem.details.is_none(), "{schema}");
    }
}
#[test]
fn nested_coordinates_and_escaped_names_in_a_supplied_resource() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    let resource = |schema: &str| {
        ResourceSet::new([SchemaResource {
            uri: "https://example.test/schema".into(),
            document: JsonValue::parse(schema).unwrap(),
        }])
        .unwrap()
    };
    let contract = prepare(
        document,
        resource(r#"{"allOf":[{"properties":{"a/~":{"minimum":9007199254740993}}}]}"#),
    );
    let value = JsonValue::parse(r#"{"a/~":6}"#).unwrap();
    let result = contract.explain(&value);
    let problem = &failure(&result).0[0];
    assert_eq!(problem.instance, "/a~1~0");
    assert_eq!(problem.schema.pointer, "/allOf/0/properties/a~1~0/minimum");
    assert_eq!(
        problem.schema.resource.as_deref(),
        Some("https://example.test/schema")
    );
}

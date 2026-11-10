use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare(
    text: &str,
    resources: ExternalSchemas,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
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
        .prepare(&evaluator, &resources)
    else {
        panic!("not ready")
    };
    contract
}
/// Check a value cheaply, or explain it when `details` is set.
fn run(schema: &str, input: &str, details: bool) -> ValueOutcome {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let contract = prepare(&text, ExternalSchemas::default());
    let value = JsonValue::parse(input).unwrap();
    if details {
        contract.explain(&value)
    } else {
        contract.evaluate(&value)
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
        (r#"{"contains":true,"minContains":2}"#, "[1]", "/contains"),
        (r#"{"contains":true,"maxContains":0}"#, "[1]", "/contains"),
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
                    ExternalSchemas::new([ExternalSchema {
                        uri: uri.into(),
                        document: JsonValue::parse(schema).unwrap(),
                    }])
                    .unwrap(),
                )
            } else {
                (schema.to_owned(), ExternalSchemas::default())
            };
            let text =
                format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{entry}}}}}}}"#);
            let value = JsonValue::parse(input).unwrap();
            let contract = prepare(&text, resources);
            let coarse = contract.evaluate(&value);
            assert!(failure(&coarse).0.is_empty());
            let upstream = contract.explain(&value);
            let location = &failure(&upstream).0[0].schema;
            assert_eq!(location.source_uri.as_deref(), external.then_some(uri));
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
// reported at a position the original does not hold. Facts are compared with
// that source, never with an evaluator's private projection.
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
            "/contains",
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
            "type",
            "/unevaluatedItems/type",
            "/1",
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
            "falseSchema",
            "/unevaluatedProperties",
            "/a",
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
        assert!(complete, "{schema}");
        assert_eq!(problems.len(), 1, "{schema}");
        let problem = &problems[0];
        assert_eq!(problem.code, code, "{schema}");
        assert_eq!(problem.instance, instance, "{schema}");
        assert_eq!(problem.schema.source_uri, None, "{schema}");
        assert_eq!(
            problem.schema.pointer,
            format!("/operations/op/input{pointer}"),
            "{schema}"
        );
        let source = JsonValue::parse(schema).unwrap();
        assert_facts_match_source(problem, source.at(pointer).unwrap());
    }
}
#[test]
fn nested_coordinates_and_escaped_names_in_a_supplied_resource() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    let resource = |schema: &str| {
        ExternalSchemas::new([ExternalSchema {
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
        problem.schema.source_uri.as_deref(),
        Some("https://example.test/schema")
    );
    assert_eq!(
        problem.details,
        Some(ValueProblemDetails::NumericBound {
            bound: "9007199254740993".into(),
        })
    );
}

fn assert_facts_match_source(problem: &ValueProblem, operand: JsonRef<'_>) {
    match problem.details.as_ref() {
        Some(ValueProblemDetails::Type { expected }) => {
            let types = if let Some(types) = operand.elements() {
                types.map(|v| v.as_str().unwrap()).collect::<Vec<_>>()
            } else {
                vec![operand.as_str().unwrap()]
            };
            assert_eq!(expected, &types);
        }
        Some(ValueProblemDetails::Required { member }) => {
            assert!(
                operand
                    .elements()
                    .unwrap()
                    .any(|v| v.as_str() == Some(member))
            );
        }
        Some(
            ValueProblemDetails::NumericBound { bound } | ValueProblemDetails::SizeBound { bound },
        ) => {
            assert_eq!(bound, operand.text());
        }
        Some(ValueProblemDetails::Enum { choices }) => {
            assert_eq!(
                choices,
                &operand
                    .elements()
                    .unwrap()
                    .map(|v| v.text())
                    .collect::<Vec<_>>()
            );
        }
        None => assert!(matches!(
            problem.code.as_str(),
            "const"
                | "pattern"
                | "multipleOf"
                | "uniqueItems"
                | "contains"
                | "propertyNames"
                | "falseSchema"
                | "anyOf"
                | "oneOf"
                | "not"
        )),
        _ => panic!("unexpected detail variant"),
    }
}

#[test]
fn every_populated_keyword_serializes_exact_schema_facts_without_rejected_values() {
    use serde_json::json;
    let secret = "SECRET-rejected-instance";
    let number = "987654321.123456789";
    let mut cases = vec![
        (r#"{"required":["schema-member"]}"#.to_owned(), format!(r#"{{"present":"{secret}"}}"#), secret,
            json!({"kind":"required","member":"schema-member"})),
        (r#"{"type":"integer"}"#.to_owned(), format!("\"{secret}\""), secret,
            json!({"kind":"type","expected":["integer"]})),
        (r#"{"type":["null","object"]}"#.to_owned(), format!("\"{secret}\""), secret,
            json!({"kind":"type","expected":["null","object"]})),
        (r#"{"enum":["schema\u002dchoice",9007199254740993,1E+400,-0.00,{ "x" : [1e-999, "\ud83d\ude00"] }] }"#.to_owned(), format!("\"{secret}\""), secret,
            json!({"kind":"enum","choices":[r#""schema\u002dchoice""#, "9007199254740993", "1E+400", "-0.00", r#"{ "x" : [1e-999, "\ud83d\ude00"] }"#]})),
    ];
    for (keyword, token) in [
        ("minimum", "1E+400"),
        ("maximum", "-0.00"),
        ("exclusiveMinimum", "9007199254740993"),
        ("exclusiveMaximum", "1e-999"),
    ] {
        cases.push((
            format!(r#"{{"{keyword}":{token}}}"#),
            number.into(),
            number,
            json!({"kind":"numeric-bound","bound":token}),
        ));
    }
    for (minimum, maximum, input) in [
        ("minLength", "maxLength", format!("\"{secret}\"")),
        ("minItems", "maxItems", format!("[\"{secret}\"]")),
        (
            "minProperties",
            "maxProperties",
            format!(r#"{{"present":"{secret}"}}"#),
        ),
    ] {
        for (keyword, token) in [(minimum, "1.000e+3"), (maximum, "0e0")] {
            cases.push((
                format!(r#"{{"{keyword}":{token}}}"#),
                input.clone(),
                secret,
                json!({"kind":"size-bound","bound":token}),
            ));
        }
    }
    for (schema, input, rejected, expected) in cases {
        let result = run(&schema, &input, true);
        let (problems, complete) = failure(&result);
        assert!(complete, "{schema}");
        assert_eq!(problems.len(), 1, "{schema}");
        let problem = &problems[0];
        let source = JsonValue::parse(&schema).unwrap();
        assert_facts_match_source(problem, source.get(&problem.code).unwrap());
        assert_eq!(serde_json::to_value(&problem.details).unwrap(), expected);
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["problems"][0]["details"], expected);
        assert_eq!(problem.message, "value fails this schema assertion");
        assert_eq!(serialized["message"], "value does not satisfy the schema");
        assert_eq!(
            format!("{:?}", problem.details.as_ref().unwrap()),
            "ValueProblemDetails { .. }"
        );
        for text in [
            serde_json::to_string(&result).unwrap(),
            serde_json::to_string(&problem.details).unwrap(),
            problem.message.clone(),
            format!("{result:?}"),
            format!("{result:#?}"),
            format!("{problem:?}"),
        ] {
            assert!(!text.contains(rejected), "{schema}");
        }
        let ordinary = run(&schema, &input, false);
        assert!(failure(&ordinary).0.is_empty());
        assert!(!failure(&ordinary).1);
    }
}

#[test]
fn missing_members_keep_required_array_order_even_when_locations_tie() {
    // More than a small sort partition, with names deliberately not lexical.
    let names = (0..40)
        .rev()
        .map(|i| format!("member-{i}"))
        .collect::<Vec<_>>();
    for required in [vec!["z".into(), "a".into()], names] {
        let schema = serde_json::json!({"required":required}).to_string();
        for _ in 0..5 {
            let result = run(&schema, "{}", true);
            let (problems, complete) = failure(&result);
            assert!(complete);
            assert_eq!(problems.len(), required.len());
            for (problem, member) in problems.iter().zip(&required) {
                assert_eq!(problem.instance, "");
                assert_eq!(problem.schema.pointer, "/operations/op/input/required");
                assert_eq!(
                    problem.details,
                    Some(ValueProblemDetails::Required {
                        member: member.clone()
                    })
                );
            }
        }
    }
}

#[test]
fn unpaired_required_member_is_omitted_without_loss_or_verdict_change() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://example.test/schema".into(),
        document: JsonValue::parse(r#"{"required":["\ud800","scalar"]}"#).unwrap(),
    }])
    .unwrap();
    let contract = prepare(document, resources);
    let value = JsonValue::parse("{}").unwrap();
    assert!(failure(&contract.evaluate(&value)).0.is_empty());
    let result = contract.explain(&value);
    let (problems, complete) = failure(&result);
    assert!(!complete);
    assert_eq!(problems.len(), 2);
    assert!(problems[0].details.is_none());
    assert_eq!(
        problems[1].details,
        Some(ValueProblemDetails::Required {
            member: "scalar".into()
        })
    );
    let serialized = serde_json::to_value(&result).unwrap();
    assert!(serialized["problems"][0].get("details").is_none());
    assert!(!serde_json::to_string(&result).unwrap().contains('\u{fffd}'));
}

#[test]
fn supplied_dependency_facts_are_original_names_and_keep_instance_data_private() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    for (dialect, keyword) in [
        (
            "https://json-schema.org/draft/2020-12/schema",
            "dependentRequired",
        ),
        ("http://json-schema.org/draft-07/schema#", "dependencies"),
    ] {
        let source = JsonValue::parse(format!(
            r#"{{"$schema":"{dialect}","{keyword}":{{"present":["schema\u002dmember"]}}}}"#
        ))
        .unwrap();
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: "https://example.test/schema".into(),
            document: source.clone(),
        }])
        .unwrap();
        let contract = prepare(document, resources);
        let value = JsonValue::parse(r#"{"present":"SECRET-instance"}"#).unwrap();
        let result = contract.explain(&value);
        let (problems, complete) = failure(&result);
        assert!(complete);
        assert_eq!(problems.len(), 1);
        let problem = &problems[0];
        assert_eq!(problem.code, keyword);
        assert_eq!(
            problem.details,
            Some(ValueProblemDetails::Required {
                member: source
                    .get(keyword)
                    .unwrap()
                    .get("present")
                    .unwrap()
                    .element(0)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into(),
            })
        );
        assert_eq!(
            serde_json::to_value(&problem.details).unwrap(),
            serde_json::json!({"kind":"required","member":"schema-member"})
        );
        assert_eq!(problem.message, "value fails this schema assertion");
        for text in [
            serde_json::to_string(&result).unwrap(),
            format!("{result:?}"),
            format!("{problem:?}"),
            format!("{:?}", problem.details),
        ] {
            assert!(!text.contains("SECRET-instance"));
        }
        assert!(!format!("{problem:?}").contains("schema-member"));
        assert!(!format!("{:?}", problem.details).contains("schema-member"));
    }
}

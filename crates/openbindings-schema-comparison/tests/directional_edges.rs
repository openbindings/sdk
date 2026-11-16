use openbindings::JsonValue;
use openbindings_schema_comparison::{
    ComparisonContext, ComparisonLimits, Direction, SchemaInput, Verdict, compare, identical,
};
use serde_json::{Value, json};

fn exact(value: &Value) -> JsonValue {
    JsonValue::parse(value.to_string()).unwrap()
}
fn input(value: &Value) -> SchemaInput {
    let root = exact(value);
    SchemaInput::new(root.clone(), root)
}
fn assert_flow(flow: &SchemaInput, receiver: &SchemaInput, expected: Verdict) {
    for direction in [Direction::Input, Direction::Output] {
        let (target, candidate) = if direction == Direction::Input {
            (flow, receiver)
        } else {
            (receiver, flow)
        };
        let result = compare(direction, target, candidate, &ComparisonLimits::default()).unwrap();
        assert_eq!(result.verdict(), expected, "{direction:?}");
    }
}
fn supplied(input: SchemaInput) -> SchemaInput {
    input.with_resource("https://r.test/s", exact(&json!({"type":"string"})))
}

#[test]
fn standalone_dialects_are_preserved_on_every_input_route() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for dialect in [
        "http://json-schema.org/draft-07/schema#",
        "http://json-schema.org/draft-04/schema#",
    ] {
        let root = json!({"$schema":dialect,"$ref":"https://r.test/s","maxLength":1});
        for base in [None, Some("https://r.test/root")] {
            let mut context = ComparisonContext::new(exact(&root));
            if let Some(base) = base {
                context = context.with_base(base);
            }
            let direct = supplied(context.schema(context.document().clone()));
            let detached = supplied(context.schema(exact(&root)));
            let mut separate = supplied(SchemaInput::new(exact(&root), exact(&root)));
            if let Some(base) = base {
                separate = separate.with_base(base);
            }
            let external = supplied(
                input(&json!({"$ref":"https://r.test/root"}))
                    .with_resource("https://r.test/root", exact(&root)),
            );
            for route in [&direct, &detached, &separate, &external] {
                assert_flow(route, &receiver, Verdict::Indeterminate);
                assert_flow(route, route, Verdict::Compatible);
            }
            assert!(
                identical(Some(&direct), Some(&detached), &ComparisonLimits::default()).unwrap()
            );
        }
    }
}

#[test]
fn selected_descendants_inherit_their_standalone_or_detached_root_dialect() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for dialect in [
        "http://json-schema.org/draft-07/schema#",
        "http://json-schema.org/draft-04/schema#",
    ] {
        let root =
            json!({"$schema":dialect,"properties":{"p":{"$ref":"https://r.test/s","maxLength":1}}});
        for base in [None, Some("https://r.test/root")] {
            let mut context = ComparisonContext::new(exact(&root));
            if let Some(base) = base {
                context = context.with_base(base);
            }
            let detached = exact(&root);
            for document in [context.document(), &detached] {
                let selected = document.at("/properties/p").unwrap().to_owned();
                assert_flow(
                    &supplied(context.schema(selected)),
                    &receiver,
                    Verdict::Indeterminate,
                );
            }
        }
    }
}

#[test]
fn selected_positions_inherit_resource_dialects_through_arbitrary_members() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for (dialect, expected) in [
        (
            Some("http://json-schema.org/draft-07/schema#"),
            Verdict::Indeterminate,
        ),
        (
            Some("https://json-schema.org/draft/2019-09/schema"),
            Verdict::Indeterminate,
        ),
        (None, Verdict::Compatible),
    ] {
        let schema = json!({"$ref":"https://r.test/s","maxLength":1});
        for (path, mut root) in [
            ("/schemas/A", json!({"schemas":{"A":schema}})),
            (
                "/carrier/schemas/A",
                json!({"carrier":{"schemas":{"A":schema}}}),
            ),
            ("/array/0/A", json!({"array":[{"A":schema}]})),
            ("/definitions/A", json!({"definitions":{"A":schema}})),
            (
                "/definitions/A",
                json!({"definitions":{"$id":{},"$schema":{},"A":schema}}),
            ),
        ] {
            if let Some(dialect) = dialect {
                root["$schema"] = json!(dialect);
            }
            let context = ComparisonContext::new(exact(&root)).with_base("https://r.test/root");
            let selected =
                supplied(context.schema(context.document().at(path).unwrap().to_owned()));
            // In draft-07 the $ref hides maxLength, so "ab" flows to a
            // receiver that refuses it. Other foreign dialects also stop proof.
            assert_flow(&selected, &receiver, expected);
            assert_flow(&selected, &selected, Verdict::Compatible);
        }
    }
}

#[test]
fn selected_positions_use_the_nearest_declared_resource_dialect() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for (outer, inner, expected) in [
        (
            "https://json-schema.org/draft/2020-12/schema",
            "http://json-schema.org/draft-07/schema#",
            Verdict::Indeterminate,
        ),
        (
            "http://json-schema.org/draft-07/schema#",
            "https://json-schema.org/draft/2020-12/schema",
            Verdict::Compatible,
        ),
    ] {
        let root = json!({"$schema":outer,"definitions":{"resource":{
            "$id":"https://r.test/embedded","$schema":inner,
            "carrier":{"schemas":{"A":{"$ref":"https://r.test/s","maxLength":1}}}
        }}});
        let context = ComparisonContext::new(exact(&root)).with_base("https://r.test/root");
        let selected = supplied(
            context.schema(
                context
                    .document()
                    .at("/definitions/resource/carrier/schemas/A")
                    .unwrap()
                    .to_owned(),
            ),
        );
        assert_flow(&selected, &receiver, expected);
    }
}

#[test]
fn obi_selections_keep_2020_12_through_arbitrary_members() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    let schema = json!({"$schema":"http://json-schema.org/draft-07/schema#","$ref":"https://r.test/s","maxLength":1});
    let root = json!({"openbindings":"0.2.0","operations":{},
        "$schema":"http://json-schema.org/draft-07/schema#",
        "schemas":{"A":schema},"carrier":{"schemas":{"A":schema}}});
    let context = ComparisonContext::new(exact(&root));
    for path in ["/schemas/A", "/carrier/schemas/A"] {
        let selected = supplied(context.schema(context.document().at(path).unwrap().to_owned()));
        assert_flow(&selected, &receiver, Verdict::Compatible);
    }
}

#[test]
fn obi_positions_and_local_references_keep_the_2020_12_interpretation() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for dialect in [
        "http://json-schema.org/draft-07/schema#",
        "http://json-schema.org/draft-04/schema#",
    ] {
        let schema = json!({"$schema":dialect,"$ref":"https://r.test/s","maxLength":1});
        let root = json!({"openbindings":"0.2.0","schemas":{"S":schema},"operations":{"op":{"input":{"$ref":"#/schemas/S"}}}});
        for base in [None, Some("https://r.test/obi")] {
            let mut context = ComparisonContext::new(exact(&root));
            if let Some(base) = base {
                context = context.with_base(base);
            }
            for path in ["/schemas/S", "/operations/op/input"] {
                let selected =
                    supplied(context.schema(context.document().at(path).unwrap().to_owned()));
                assert_flow(&selected, &receiver, Verdict::Compatible);
            }
            let copy = supplied(context.schema(exact(&schema)));
            assert_flow(&copy, &receiver, Verdict::Indeterminate);
        }
    }
}

#[test]
fn kept_definitions_do_not_change_union_counterexamples() {
    for definitions in [false, true] {
        let mut receiver = json!({"type":"object"});
        let mut flow =
            json!({"anyOf":[{"anyOf":[{}]},{"type":"object","additionalProperties":false}]});
        if definitions {
            receiver["$defs"] = json!({"unused":{}});
            flow["$defs"] = json!({"unused":{}});
        }
        assert_flow(&input(&flow), &input(&receiver), Verdict::Incompatible);
    }
}

#[test]
fn finite_eligibility_disregards_inapplicable_and_unconstraining_keywords() {
    for (keyword, value, applicable) in [
        ("minimum", json!(0), "number"),
        ("maximum", json!(0), "number"),
        ("exclusiveMinimum", json!(0), "number"),
        ("exclusiveMaximum", json!(0), "number"),
        ("minLength", json!(0), "string"),
        ("maxLength", json!(1), "string"),
        ("minItems", json!(0), "array"),
        ("maxItems", json!(1), "array"),
        ("items", json!({}), "array"),
        ("properties", json!({}), "object"),
        ("required", json!([]), "object"),
        ("additionalProperties", json!(true), "object"),
    ] {
        let receiver = input(&json!({"const":false}));
        let flow = json!({"type":"boolean","const":true,keyword:value});
        assert_flow(&input(&flow), &receiver, Verdict::Incompatible);
        let other = if applicable == "string" {
            "number"
        } else {
            "string"
        };
        let flow = json!({"type":other,keyword:value});
        assert_flow(&input(&flow), &receiver, Verdict::Incompatible);
        let literal = match applicable {
            "number" => json!(1),
            "string" => json!("a"),
            "array" => json!([]),
            "object" => json!({}),
            _ => unreachable!(),
        };
        let flow = json!({"type":applicable,"const":literal,keyword:value});
        let expected =
            if ["items", "properties", "required", "additionalProperties"].contains(&keyword) {
                Verdict::Incompatible
            } else {
                Verdict::Indeterminate
            };
        assert_flow(&input(&flow), &receiver, expected);
        let flow = json!({"type":applicable,keyword:value});
        assert_flow(&input(&flow), &receiver, Verdict::Indeterminate);
    }
    for types in [json!("integer"), json!(["boolean", "number"])] {
        assert_flow(
            &input(&json!({"type":types,"maximum":0})),
            &input(&json!({"const":false})),
            Verdict::Indeterminate,
        );
    }
}

#[test]
fn detached_references_use_the_configured_targets_dialect_and_obi_policy() {
    let receiver = input(&json!({"type":"string","maxLength":1}));
    for dialect in [
        "http://json-schema.org/draft-07/schema#",
        "http://json-schema.org/draft-04/schema#",
    ] {
        for base in [None, Some("https://r.test/root")] {
            let root = json!({"$schema":dialect,"properties":{"p":{"$ref":"https://r.test/s","maxLength":1}}});
            let obi = json!({"openbindings":"0.2.0","operations":{},"schemas":{"S":{"$id":"https://r.test/nested","$schema":dialect,"$ref":"https://r.test/s","maxLength":1}}});
            for (document, reference, expected) in [
                (root, "#/properties/p", Verdict::Indeterminate),
                (obi, "#/schemas/S", Verdict::Compatible),
            ] {
                let mut context = ComparisonContext::new(exact(&document));
                if let Some(base) = base {
                    context = context.with_base(base);
                }
                let selected = supplied(context.schema(exact(&json!({"$ref":reference}))));
                // The OBI target's $id is retained identity context. Compare it
                // with the same configured target rather than dropping that context.
                let accepts = if expected == Verdict::Compatible {
                    supplied(
                        context.schema(context.document().at("/schemas/S").unwrap().to_owned()),
                    )
                } else {
                    receiver.clone()
                };
                assert_flow(&selected, &accepts, expected);
            }
        }
    }
}

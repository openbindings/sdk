use openbindings::{Document, JsonValue};
use openbindings_schema_comparison::*;
use serde_json::{Value, json};

fn input(value: &Value) -> SchemaInput {
    let value = JsonValue::parse(value.to_string()).unwrap();
    SchemaInput::new(value.clone(), value)
}
fn check(flow: &Value, receiver: &Value) -> Comparison {
    compare(
        Direction::Input,
        &input(flow),
        &input(receiver),
        &ComparisonLimits::default(),
    )
    .unwrap()
}
#[test]
fn externally_assigned_dialects_keep_root_fragment_and_embedded_inheritance() {
    let vectors = include_str!("fixtures/external-resources.md");
    let mut count = 0;
    for block in vectors.split("```json\n").skip(1) {
        let vector: Value = serde_json::from_str(block.split("```").next().unwrap()).unwrap();
        let expected = block
            .split("Expected profile verdict: **")
            .nth(1)
            .unwrap()
            .split("**")
            .next()
            .unwrap();
        let expected = match expected {
            "compatible" => Verdict::Compatible,
            "indeterminate" => Verdict::Indeterminate,
            "incompatible" => Verdict::Incompatible,
            other => panic!("unknown expected verdict: {other}"),
        };
        let supplied = |schema: SchemaInput| {
            vector["resources"].as_object().unwrap().iter().fold(
                schema,
                |context, (uri, resource)| {
                    context.with_resource(uri, JsonValue::parse(resource.to_string()).unwrap())
                },
            )
        };
        let target = if let Some(document) = vector.get("document") {
            let document = JsonValue::parse(document.to_string()).unwrap();
            let selected = document
                .at(vector["pointer"].as_str().unwrap())
                .unwrap()
                .to_owned();
            supplied(SchemaInput::new(selected, document))
        } else {
            supplied(input(&vector["target"]))
        };
        let candidate = supplied(input(&vector["candidate"]));
        for direction in [Direction::Input, Direction::Output] {
            let result =
                compare(direction, &target, &candidate, &ComparisonLimits::default()).unwrap();
            assert_eq!(result.verdict(), expected, "{vector}");
            assert_eq!(
                result.finding(),
                (expected == Verdict::Indeterminate).then_some("outside-profile")
            );
        }
        assert!(identical(Some(&target), Some(&target), &ComparisonLimits::default()).unwrap());
        count += 1;
    }
    assert_eq!(count, 9);
    // A foreign descendant inherits without either $id or a local declaration.
    let root = json!({"$schema":"http://json-schema.org/draft-07/schema#","properties":{"p":{"type":"string"}}});
    let target = input(&json!({"$ref":"https://r.test/a#/properties/p"})).with_resource(
        "https://r.test/a",
        JsonValue::parse(root.to_string()).unwrap(),
    );
    assert_eq!(
        compare(
            Direction::Input,
            &target,
            &input(&json!({"type":"string"})),
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Indeterminate
    );
    let standalone = input(&root).with_base("https://r.test/root");
    assert_eq!(
        compare(
            Direction::Input,
            &standalone,
            &input(&json!({"type":"string"})),
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Indeterminate
    );
    // Entire supplied context participates in foreign identity.
    let other = target.clone().with_resource(
        "https://r.test/unrelated",
        JsonValue::parse("false").unwrap(),
    );
    assert!(!identical(Some(&target), Some(&other), &ComparisonLimits::default()).unwrap());
    // Contained schemas follow section 5.5, including an invalid declaration.
    let root = JsonValue::parse(json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$schema":"http://json-schema.org/draft-07/schema#","type":"string"}}}}).to_string()).unwrap();
    let root = SchemaInput::new(root.at("/operations/op/input").unwrap().to_owned(), root);
    assert!(
        identical(
            Some(&root),
            Some(&input(&json!({"type":"string"}))),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
}
#[test]
fn retained_schema_positions_strip_annotations_and_canonicalize_only_schemas() {
    let a = json!({"$comment":"private annotation","description":"annotation","x-extra":false,"type":"string","required":["b","a","a"]});
    let b = json!({"type":["string"],"required":["a","b"]});
    for key in [
        "not",
        "if",
        "then",
        "else",
        "dependentSchemas",
        "prefixItems",
        "contains",
        "patternProperties",
        "propertyNames",
        "unevaluatedItems",
        "unevaluatedProperties",
        "$defs",
    ] {
        let wrap = |child: &Value| match key {
            "dependentSchemas" | "patternProperties" => json!({key:{"p":child}}),
            "$defs" => json!({"pattern":"x",key:{"p":child}}),
            "prefixItems" => json!({key:[child]}),
            _ => json!({key:child}),
        };
        let (a, b) = (wrap(&a), wrap(&b));
        assert!(
            identical(
                Some(&input(&a)),
                Some(&input(&b)),
                &ComparisonLimits::default()
            )
            .unwrap(),
            "{key}"
        );
        assert_eq!(check(&a, &b).verdict(), Verdict::Compatible, "{key}");
        assert_eq!(
            check(&wrap(&json!(false)), &wrap(&json!({"not":{}}))).verdict(),
            Verdict::Compatible,
            "{key}"
        );
        assert_eq!(
            check(&wrap(&json!(true)), &wrap(&json!({}))).verdict(),
            Verdict::Compatible,
            "{key}"
        );
    }
    for key in ["const", "enum", "unknown"] {
        assert!(
            !identical(
                Some(&input(
                    &json!({key:if key=="enum" {json!([a])}else{a.clone()}})
                )),
                Some(&input(
                    &json!({key:if key=="enum" {json!([b])}else{b.clone()}})
                )),
                &ComparisonLimits::default()
            )
            .unwrap()
        );
    }
    assert!(
        !identical(
            Some(&input(&json!({"prefixItems":[true,false]}))),
            Some(&input(&json!({"prefixItems":[false,true]}))),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
}
#[test]
fn top_length_and_union_proofs_establish_inclusion_and_keep_other_obligations() {
    for (flow, receiver) in [
        (
            json!({"$defs":{"d":false}}),
            json!({"type":["array","boolean","null","number","object","string"]}),
        ),
        (
            json!({}),
            json!({"items":{"properties":{}},"additionalProperties":{"required":[]}}),
        ),
        (json!({}), json!({"minLength":0,"minItems":0})),
        (
            json!({"type":"string","minLength":5,"maxLength":3}),
            json!({"enum":["x"]}),
        ),
        (
            json!({"type":["string","boolean"],"minLength":5,"maxLength":3}),
            json!({"type":"boolean"}),
        ),
        (
            json!({"type":"array","maxItems":0,"items":{"type":"integer"}}),
            json!({"type":"array","items":{"type":"string"}}),
        ),
        (
            json!({"anyOf":[{"type":"string","minLength":5,"maxLength":3},{"type":"boolean"}]}),
            json!({"type":"boolean"}),
        ),
        (
            json!({"anyOf":[{"type":"string"},{"type":"integer"}]}),
            json!({"type":["string","number"]}),
        ),
    ] {
        assert_eq!(
            check(&flow, &receiver).verdict(),
            Verdict::Compatible,
            "{flow} -> {receiver}"
        );
        assert_eq!(
            compare(
                Direction::Output,
                &input(&receiver),
                &input(&flow),
                &ComparisonLimits::default()
            )
            .unwrap()
            .verdict(),
            Verdict::Compatible
        );
    }
    assert_eq!(
        check(
            &json!({"type":"string","minLength":5,"maxLength":3}),
            &json!({"pattern":"x"})
        )
        .verdict(),
        Verdict::Indeterminate
    );
    assert_eq!(
        check(
            &json!({"type":"array","maxItems":0}),
            &json!({"minItems":1})
        )
        .verdict(),
        Verdict::Incompatible
    );
    assert_eq!(
        check(
            &json!({"minimum":3,"anyOf":[{"type":"integer","minimum":0}]}),
            &json!({"type":"integer","minimum":5})
        )
        .verdict(),
        Verdict::Incompatible
    );
}
#[test]
fn single_allof_deduplicates_enum_and_preserves_identity_context_and_errors() {
    for (a, b) in [
        (json!({"allOf":[{"enum":[1,1.0,2]}]}), json!({"enum":[1,2]})),
        (
            json!({"allOf":[{"not":{"type":"string"}}]}),
            json!({"not":{"type":["string"]}}),
        ),
        (
            json!({"allOf":[{"anyOf":[true,false]}]}),
            json!({"anyOf":[true,false]}),
        ),
    ] {
        assert!(
            identical(
                Some(&input(&a)),
                Some(&input(&b)),
                &ComparisonLimits::default()
            )
            .unwrap()
        );
    }
    let union = json!({"oneOf":[{"type":"string"}]});
    let empty_definition = json!({"$defs":{"d":false},"allOf":[union]});
    assert!(
        identical(
            Some(&input(&empty_definition)),
            Some(&input(&union)),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
    assert_eq!(
        check(&empty_definition, &union).verdict(),
        Verdict::Compatible
    );
    let kept = json!({"$defs":{"d":{"pattern":"x"}},"allOf":[union]});
    assert!(
        !identical(
            Some(&input(&kept)),
            Some(&input(&union)),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
    assert_eq!(check(&kept, &union).verdict(), Verdict::Indeterminate);
    let bad = json!({"allOf":[{"additionalProperties":false},{"additionalProperties":{"$ref":"https://missing.test/a"}}]});
    assert_eq!(
        compare(
            Direction::Input,
            &input(&bad),
            &input(&json!({})),
            &ComparisonLimits::default()
        )
        .unwrap_err()
        .kind(),
        ComparisonFailureKind::MissingReference
    );
    // Enum order stays authored where the conjunction is retained, not merged.
    let retained = json!({"allOf":[{"enum":[1,1]},{"pattern":"x"}]});
    let distinct = json!({"allOf":[{"enum":[1]},{"pattern":"x"}]});
    assert!(
        !identical(
            Some(&input(&retained)),
            Some(&input(&distinct)),
            &ComparisonLimits::default()
        )
        .unwrap()
    );
}
#[test]
fn unresolved_failures_keep_located_keywords_and_boundaries_keep_their_finding() {
    let finite = check(
        &json!({"type":["string","integer"],"const":5}),
        &json!({"type":"integer"}),
    );
    assert_eq!(finite.verdict(), Verdict::Compatible);
    assert_eq!(finite.finding(), None);
    assert_eq!(finite.reason(), None);
    let cases = [
        (
            json!({"type":"integer","minimum":0.5}),
            json!({"type":"integer","minimum":1}),
            "minimum: candidate minimum 1 is greater than target minimum 0.5; unresolved: integer endpoint does not establish a failure",
        ),
        (
            json!({"type":"object","properties":{"p":{"type":"boolean"}}}),
            json!({"type":"object","properties":{"p":{"enum":[true,false]}}}),
            "properties[\"p\"]: enum: target is unconstrained but candidate has enum; unresolved: finite restriction has no eligible flowing value",
        ),
    ];
    for (flow, receiver, reason) in cases {
        let answer = check(&flow, &receiver);
        assert_eq!(answer.verdict(), Verdict::Indeterminate);
        assert_eq!(answer.finding(), Some("unresolved"));
        assert_eq!(answer.reason(), Some(reason));
    }
    let answer = check(&json!({"oneOf":[true]}), &json!({"type":"string"}));
    assert_eq!(answer.finding(), Some("outside-profile"));
    assert_eq!(
        answer.reason(),
        Some("outside-profile: differing position has no profile decision rule")
    );
}
#[test]
fn empty_skipped_and_cross_slot_folds_preserve_each_outcome() {
    for operations in [json!({}), json!({"test":{}})] {
        let doc =
            Document::parse(json!({"openbindings":"0.2.0","operations":operations}).to_string())
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .unwrap();
        let context = ComparisonContext::new(doc.document().value().clone());
        let report = check_interface(
            &doc,
            &doc,
            &context,
            &context,
            &|_, _| Some(OperationRole::Realized),
            &ComparisonLimits::default(),
        )
        .unwrap();
        assert_eq!(report.issues().count(), 0);
        assert_eq!(
            Verdict::collapse(
                report
                    .operations()
                    .iter()
                    .flat_map(|op| [op.output(), op.input()])
                    .flatten()
                    .map(|s| s.as_ref().unwrap().verdict())
            ),
            Verdict::Compatible
        );
    }
    let docs = [
        json!({"output":{"pattern":"a"},"input":{"type":"string"}}),
        json!({"output":{"pattern":"b"},"input":{"type":"integer"}}),
    ]
    .map(|op| {
        Document::parse(json!({"openbindings":"0.2.0","operations":{"test":op}}).to_string())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap()
    });
    let report = check_interface(
        &docs[0],
        &docs[1],
        &ComparisonContext::new(docs[0].document().value().clone()),
        &ComparisonContext::new(docs[1].document().value().clone()),
        &|_, _| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    let outcomes: Vec<_> = [
        report.operations()[0].output(),
        report.operations()[0].input(),
    ]
    .into_iter()
    .flatten()
    .map(|s| s.as_ref().unwrap().verdict())
    .collect();
    assert_eq!(outcomes, [Verdict::Indeterminate, Verdict::Incompatible]);
    assert_eq!(Verdict::collapse(outcomes), Verdict::Incompatible);
}

#[test]
fn length_domains_keep_exact_symbolic_endpoints_without_constructing_instances() {
    let exact = |text: &str| {
        let v = JsonValue::parse(text).unwrap();
        SchemaInput::new(v.clone(), v)
    };
    let empty =
        exact(r#"{"type":"string","minLength":9007199254740993,"maxLength":9007199254740992}"#);
    let huge = exact(
        r#"{"type":"array","minItems":1e999999999999999999999,"maxItems":10e999999999999999999998}"#,
    );
    let same = exact(r#"{"type":"array","minItems":10e999999999999999999998}"#);
    assert_eq!(
        compare(
            Direction::Input,
            &empty,
            &input(&json!({"type":"boolean"})),
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
    assert_eq!(
        compare(Direction::Input, &huge, &same, &ComparisonLimits::default())
            .unwrap()
            .verdict(),
        Verdict::Compatible
    );
}

#[test]
fn length_top_and_empty_members_ignore_kept_identity_context() {
    assert_eq!(
        check(
            &json!({}),
            &json!({"minLength":0,"properties":{},"items":true})
        )
        .verdict(),
        Verdict::Compatible
    );
    let flow = json!({"type":"object","additionalProperties":{"$defs":{"d":false},"not":{}}});
    let receiver = json!({"type":"object","additionalProperties":{"type":"integer"}});
    assert_eq!(check(&flow, &receiver).verdict(), Verdict::Compatible);
}

#[test]
fn external_descendant_declarations_without_resource_ids_do_not_override_inheritance() {
    let resource = json!({"$schema":"http://json-schema.org/draft-07/schema#","properties":{"p":{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"https://r.test/any","type":"string"}}});
    let target = input(&json!({"$ref":"https://r.test/a#/properties/p"}))
        .with_resource(
            "https://r.test/a",
            JsonValue::parse(resource.to_string()).unwrap(),
        )
        .with_resource("https://r.test/any", JsonValue::parse("{}").unwrap());
    // The assigned draft-07 dialect admits -0.5 by ignoring the type sibling.
    let result = compare(
        Direction::Input,
        &target,
        &input(&json!({"type":"string"})),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(result.verdict(), Verdict::Indeterminate);
    assert_eq!(result.finding(), Some("outside-profile"));
}

#[test]
fn unions_inside_retained_schema_positions_keep_multiset_identity() {
    for key in ["oneOf", "anyOf"] {
        let a = json!({"not":{key:[{"type":"string"},{"type":"integer"},{"type":"string"}]}});
        let b = json!({"not":{key:[{"type":"string"},{"type":"string"},{"type":"integer"}]}});
        let c = json!({"not":{key:[{"type":"integer"},{"type":"string"}]}});
        assert!(
            identical(
                Some(&input(&a)),
                Some(&input(&b)),
                &ComparisonLimits::default()
            )
            .unwrap()
        );
        assert!(
            !identical(
                Some(&input(&a)),
                Some(&input(&c)),
                &ComparisonLimits::default()
            )
            .unwrap()
        );
    }
}

#[test]
fn effective_nested_members_check_valid_additional_values() {
    // This witness shape is absent from the exhaustive oracle's nested universe:
    // {"a":{"a":0,"b":""}} flows and the closed receiver refuses it.
    let flow = json!({"properties":{"a":{"allOf":[{"required":["a"]},{"properties":{"b":{"type":"string"}}}]}}});
    let receiver = json!({"properties":{"a":{"properties":{"a":{}},"additionalProperties":false}}});
    assert_eq!(check(&flow, &receiver).verdict(), Verdict::Incompatible);
}

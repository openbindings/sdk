//! A schema reached through an external URI follows the dialect JSON Schema
//! assigns it (specification §5.2; JSON Schema Core §8.1.1): a supplied
//! resource's own `$schema` names its dialect, an embedded resource without one
//! takes its enclosing resource's, and a resource that declares none is read as
//! 2020-12. Supported external dialects retain their own inventory, assertions
//! and annotations rather than receiving a 2020-12 interpretation.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

const RESOURCE: &str = "https://dialects.test/r";

fn prepare(
    contract: &str,
    resource: &str,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let text =
        format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{contract}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant");
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: RESOURCE.into(),
        document: JsonValue::parse(resource).unwrap(),
    }])
    .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation.input().unwrap().prepare(
        &DefaultEvaluator::new(),
        &resources,
        &openbindings::PreparationLimits::default(),
    )
}

/// `S`atisfies, `F`ails, or `D` for a refusal naming the unread dialect.
fn outcome(contract: &str, resource: &str, value: &str) -> char {
    let dialect = |detail: &NoVerdict| {
        assert_eq!(
            (detail.reason, detail.code.as_ref()),
            (NoVerdictReason::UnsupportedCapability, "schema-dialect"),
            "{resource}"
        );
        'D'
    };
    let prepared = match prepare(contract, resource) {
        Ok(prepared) => prepared,
        Err(failure) => {
            assert!(
                matches!(
                    failure.kind,
                    PreparationFailureKind::UnsupportedCapability { .. }
                        | PreparationFailureKind::Resolution {
                            cause: ResolutionFailureKind::UnsupportedDialect { .. }
                        }
                ),
                "{:?}",
                failure.kind
            );
            return 'D';
        }
    };
    match prepared.evaluate(&JsonValue::parse(value).unwrap()) {
        ValueOutcome::Satisfies => 'S',
        ValueOutcome::Fails { .. } => 'F',
        ValueOutcome::NoVerdict { detail } => dialect(&detail),
        other => panic!("{other:?}"),
    }
}

#[test]
fn supplied_resources_are_read_in_the_dialect_they_declare() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    // `prefixItems` is a 2020-12 keyword: read as 2020-12, `[1]` fails it; read
    // as 2019-09, the keyword is unknown and `[1]` would satisfy.
    let prefix = r#""prefixItems":[{"type":"string"}]"#;
    for (resource, expected) in [
        (format!("{{{prefix}}}"), ['S', 'F']),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema",{prefix}}}"#),
            ['S', 'F'],
        ),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema#",{prefix}}}"#),
            ['S', 'F'],
        ),
        (
            format!(r#"{{"$schema":"https://json-schema.org/draft/2019-09/schema",{prefix}}}"#),
            ['S', 'S'],
        ),
        // Draft-07's array form of `items`, which 2020-12 has no reading for.
        (
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":[{"type":"string"}]}"#
                .to_owned(),
            ['S', 'F'],
        ),
    ] {
        let got = [r#"["a"]"#, "[1]"].map(|value| outcome(&contract, &resource, value));
        assert_eq!(got, expected, "{resource}");
    }
}

#[test]
fn embedded_resources_take_their_enclosing_resources_dialect_unless_they_declare_one() {
    // A 2020-12 resource embedding a 2019-09 one, and a 2019-09 resource
    // embedding one that declares nothing: both use 2019-09 semantics.
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/$defs/inner"}}"#);
    let inner = r#""$id":"https://dialects.test/inner","prefixItems":[{"type":"string"}]"#;
    let nested_2019 = format!(
        r#"{{"$defs":{{"inner":{{"$schema":"https://json-schema.org/draft/2019-09/schema",{inner}}}}}}}"#
    );
    let enclosing_2019 = format!(
        r#"{{"$schema":"https://json-schema.org/draft/2019-09/schema","$defs":{{"inner":{{{inner}}}}}}}"#
    );
    let enclosing_2020 = format!(r#"{{"$defs":{{"inner":{{{inner}}}}}}}"#);
    for (resource, expected) in [
        (nested_2019, ['S', 'S']),
        (enclosing_2019, ['S', 'S']),
        (enclosing_2020, ['S', 'F']),
    ] {
        let got = [r#"["a"]"#, "[1]"].map(|value| outcome(&contract, &resource, value));
        assert_eq!(got, expected, "{resource}");
    }
}

#[test]
fn legacy_dependencies_and_tuple_items_retain_their_assertions() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    for (body, value, expected) in [
        (r#""dependencies":{"a":["b"]}"#, r#"{"a":1}"#, 'F'),
        (r#""dependencies":{"a":["b"]}"#, r#"{"a":1,"b":2}"#, 'S'),
        (
            r#""dependencies":{"a":{"required":["b"]}}"#,
            r#"{"a":1}"#,
            'F',
        ),
        (
            r#""items":[{"type":"string"}],"additionalItems":false"#,
            r#"["x"]"#,
            'S',
        ),
        (
            r#""items":[{"type":"string"}],"additionalItems":false"#,
            r#"["x",1]"#,
            'F',
        ),
        (r#""items":[{"type":"string"}]"#, r#"[1]"#, 'F'),
        (
            r#""dependentRequired":{"a":["b"]},"prefixItems":[false],"unevaluatedProperties":false"#,
            r#"{"a":1}"#,
            'S',
        ),
    ] {
        let schema = format!(r#"{{"$schema":"http://json-schema.org/draft-07/schema#",{body}}}"#);
        assert_eq!(
            outcome(&contract, &schema, value),
            expected,
            "{schema} on {value}"
        );
    }
}

#[test]
fn draft7_reference_siblings_and_modern_keywords_are_opaque() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/definitions/ref"}}"#);
    let schema = r##"{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"target":{"type":"string"},"ref":{"$ref":"#/definitions/target","$id":"https://wrong.test/id","type":17,"properties":{"x":{"$id":"https://dialects.test/r"}},"$defs":{"invisible":false}}}}"##;
    assert_eq!(outcome(&contract, schema, r#""x""#), 'S');
    assert_eq!(outcome(&contract, schema, "4"), 'F');
}

#[test]
fn draft7_fragment_name_preserves_the_enclosing_pointer_root() {
    let schema = r##"{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"target":{"type":"string"},"named":{"$id":"#node:part","allOf":[{"$ref":"#/definitions/target"}]}}}"##;
    let contract = format!(r#"{{"$ref":"{RESOURCE}#node:part"}}"#);
    assert_eq!(outcome(&contract, schema, r#""x""#), 'S');
    assert_eq!(outcome(&contract, schema, "4"), 'F');
}

#[test]
fn contains_annotations_follow_the_resource_dialect() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    for (dialect, expected) in [("2019-09", 'F'), ("2020-12", 'S')] {
        let schema = format!(
            r#"{{"$schema":"https://json-schema.org/draft/{dialect}/schema","contains":true,"unevaluatedItems":false}}"#
        );
        assert_eq!(outcome(&contract, &schema, "[1]"), expected);
    }
}

#[test]
fn inherited_draft7_schema_dependencies_and_nonresource_declarations() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/definitions/inner"}}"#);
    for id in ["", r#""$id":"inner.json","#] {
        let schema = format!(
            r#"{{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{{"inner":{{{id}"dependencies":{{"a":["b"]}}}}}}}}"#
        );
        assert_eq!(outcome(&contract, &schema, r#"{"a":1}"#), 'F');
    }
    let schema = r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","$defs":{"inner":{"$schema":"https://json-schema.org/draft/2020-12/schema","prefixItems":[false]}}}"#;
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/$defs/inner"}}"#);
    assert_eq!(outcome(&contract, schema, "[1]"), 'S');
}

#[test]
fn recursive_references_use_the_outermost_enabled_2019_scope() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    for (anchor, keyword, expected) in [
        (true, "$recursiveRef", 'F'),
        (false, "$recursiveRef", 'S'),
        (true, "$ref", 'S'),
    ] {
        let schema = format!(
            r##"{{"$schema":"https://json-schema.org/draft/2019-09/schema","$recursiveAnchor":{anchor},"$ref":"tree.json","unevaluatedProperties":false,"$defs":{{"tree":{{"$id":"tree.json","$recursiveAnchor":true,"type":"object","properties":{{"data":true,"children":{{"type":"array","items":{{"{keyword}":"#"}}}}}}}}}}}}"##
        );
        assert_eq!(
            outcome(&contract, &schema, r#"{"children":[{"data":1}]}"#),
            'S'
        );
        assert_eq!(
            outcome(&contract, &schema, r#"{"children":[{"typo":1}]}"#),
            expected
        );
    }
}

#[test]
fn foreign_schema_shape_failures_and_ignored_reference_siblings_are_distinct() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    for dialect in [
        "http://json-schema.org/draft-07/schema#",
        "https://json-schema.org/draft/2019-09/schema",
    ] {
        for body in [
            r#""type":17"#,
            r#""items":[17]"#,
            r#""properties":{"x":17}"#,
        ] {
            let schema = format!(r#"{{"$schema":"{dialect}",{body}}}"#);
            let error = prepare(&contract, &schema).unwrap_err();
            assert!(
                matches!(error.kind, PreparationFailureKind::InvalidExternalSchema),
                "{error:?}"
            );
        }
    }
}

#[test]
fn exact_foreign_property_and_dependency_names_are_not_silently_skipped() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}"}}"#);
    for (dialect, keyword) in [
        ("http://json-schema.org/draft-07/schema#", "dependencies"),
        (
            "https://json-schema.org/draft/2019-09/schema",
            "dependentSchemas",
        ),
    ] {
        for body in [
            r#""properties":{"\ud800":false}"#.to_owned(),
            format!(r#""{keyword}":{{"\ud800":false}}"#),
        ] {
            let schema = format!(r#"{{"$schema":"{dialect}",{body}}}"#);
            assert_eq!(outcome(&contract, &schema, r#"{"\ud800":1}"#), 'F');
            assert_eq!(outcome(&contract, &schema, r#"{"\ud801":1}"#), 'S');
        }
    }
}

#[test]
fn uninterpreted_identifier_inventory_is_a_capability_refusal_not_a_defect() {
    for id in ["child.json#name", "https://other.test/x#name", "#bad?"] {
        let resource = format!(
            r#"{{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{{"opaque":{{"$id":"{id}","properties":{{"x":false}}}}}}}}"#
        );
        let contract = format!(r#"{{"$ref":"{RESOURCE}#/definitions/opaque/properties/x"}}"#);
        let refusal = prepare(&contract, &resource).unwrap_err();
        assert!(
            matches!(refusal.kind, PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::UnsupportedCapability { ref capability }
        } if capability == "schema-identifier-form"),
            "{refusal:?}"
        );
    }
}

#[test]
fn fragment_alias_allocation_is_bounded_and_ignored_siblings_are_not_charged() {
    let limits = PreparationLimits {
        max_identity_bytes: 1024,
        ..PreparationLimits::default()
    };
    for (reference, accepted) in [("", false), (r##""$ref":"#", "##, true)] {
        let resource = format!(
            r##"{{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{{"n":{{{reference}"$id":"#a{}"}}}}}}"##,
            "x".repeat(100_000)
        );
        let result = ExternalSchemas::new_with_limits(
            [ExternalSchema {
                uri: RESOURCE.into(),
                document: JsonValue::parse(resource).unwrap(),
            }],
            &limits,
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert!(matches!(
                result.unwrap_err().kind,
                ExternalSchemaErrorKind::Limit { .. }
            ));
        }
    }
}

#[test]
fn mixed_resource_boundary_and_reference_object_have_one_profile_and_base() {
    let contract = format!(r#"{{"$ref":"{RESOURCE}#/$defs/inner"}}"#);
    let resource = r##"{"$defs":{"target":{"type":"string"},"inner":{"$id":"child.json","$schema":"http://json-schema.org/draft-07/schema#","$ref":"https://dialects.test/r#/$defs/target","type":"integer"}}}"##;
    assert_eq!(outcome(&contract, resource, r#""x""#), 'S');
    assert_eq!(outcome(&contract, resource, "1"), 'F');
    // Its declared URI selects exactly the same resource and interpretation.
    assert_eq!(
        outcome(
            r#"{"$ref":"https://dialects.test/child.json"}"#,
            resource,
            r#""x""#
        ),
        'S'
    );
}

#[test]
fn stopped_inventory_cannot_certify_external_names_but_contained_priority_survives() {
    let opaque = r##"{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"known":{"$id":"#nested","type":"integer"},"opaque":{"$id":"child.json#name","definitions":{"hidden":{"$id":"https://dialects.test/shared","type":"string"},"duplicate":{"$id":"#nested","type":"string"}}}}}"##;
    let external = ExternalSchemas::new([
        ExternalSchema {
            uri: RESOURCE.into(),
            document: JsonValue::parse(opaque).unwrap(),
        },
        ExternalSchema {
            uri: "https://dialects.test/shared".into(),
            document: JsonValue::parse(r#"{"type":"integer"}"#).unwrap(),
        },
    ])
    .unwrap();
    let prepare = |contract: &str, contained: &str| {
        let proof = Document::parse(format!(
            r#"{{"openbindings":"0.2.0",{contained}"operations":{{"op":{{"input":{contract}}}}}}}"#
        ))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
        proof
            .resolve_operation("op")
            .unwrap()
            .input()
            .unwrap()
            .prepare(
                &DefaultEvaluator::new(),
                &external,
                &PreparationLimits::default(),
            )
    };
    for reference in [
        "https://dialects.test/shared",
        "https://dialects.test/missing",
        "https://dialects.test/r#nested",
        "https://dialects.test/r#absent",
    ] {
        let error = prepare(&format!(r#"{{"$ref":"{reference}"}}"#), "").unwrap_err();
        assert!(
            matches!(error.kind, PreparationFailureKind::Resolution { cause: ResolutionFailureKind::UnsupportedCapability { ref capability } } if capability == "schema-identifier-form"),
            "{error:?}"
        );
    }
    for (contract, contained) in [
        (r#"{"type":"integer"}"#, ""),
        (
            r#"{"$ref":"https://dialects.test/shared"}"#,
            r#""schemas":{"owned":{"$id":"https://dialects.test/shared","type":"integer"}},"#,
        ),
    ] {
        let prepared = prepare(contract, contained).unwrap();
        assert!(matches!(
            prepared.evaluate(&JsonValue::parse("1").unwrap()),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            prepared.evaluate(&JsonValue::parse(r#""x""#).unwrap()),
            ValueOutcome::Fails { .. }
        ));
    }
}

use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn prepare(schema: &str, extra: &str) -> PreparedValueContract {
    // Schema text and opaque examples live in a supplied resource. OBI-01
    // governs the interface document, not supplied schemas or caller values.
    let text = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://strings.invalid/schema#/$defs/target"}}}}"#;
    let resource = format!(r#"{{"$defs":{{"target":{schema}}},"examples":["\ud800"]{extra}}}"#);
    let resources = ResourceSet::new([SchemaResource {
        uri: "https://strings.invalid/schema".into(),
        document: JsonValue::parse(resource).unwrap(),
    }])
    .unwrap();
    let document = Document::parse(text).unwrap();
    let document = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = document
        .resolve_operation("op")
        .expect("declared operation");
    let Ok(contract) = operation
        .input()
        .expect("declared input")
        .prepare_with(&DefaultEvaluator::new(), &resources)
    else {
        panic!("{text}")
    };
    contract
}
fn outcome(schema: &str, value: &str) -> ValueOutcome {
    prepare(schema, "").validate(&JsonValue::parse(value).unwrap())
}
#[test]
fn uninterpreted_surrogates_do_not_disable_contracts() {
    // Supplied resource metadata and opaque content, plus reached annotations.
    for extra in [
        r#", "description":"cut \ud83d""#,
        r#", "sources":{"s":{"kind":"k","content":"\ud800"}}"#,
        "",
    ] {
        let contract = prepare(
            r#"{"description":"\ud800","default":"\ud800","examples":["\ud800"],"x-data":{"\ud800":"\ud800"},"type":"integer"}"#,
            extra,
        );
        assert!(matches!(
            contract.validate(&JsonValue::parse("1").unwrap()),
            ValueOutcome::Satisfies
        ));
    }
    for schema in [
        "true",
        r#"{"type":"string"}"#,
        r#"{"anyOf":[true,{"pattern":"a"}]}"#,
    ] {
        assert!(
            matches!(outcome(schema, r#""\ud800""#), ValueOutcome::Satisfies),
            "{schema}"
        );
    }
    assert!(matches!(
        outcome("false", r#""\ud800""#),
        ValueOutcome::Fails { .. }
    ));
    for schema in ["true", r#"{"type":"object"}"#, r#"{"maxProperties":2}"#] {
        assert!(
            matches!(
                outcome(schema, r#"{"\ud800":"\ud800"}"#),
                ValueOutcome::Satisfies
            ),
            "{schema}"
        );
    }
    assert!(matches!(
        outcome(
            r#"{"properties":{"checked":{"pattern":"x"}}}"#,
            r#"{"opaque":"\ud800"}"#
        ),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        outcome(
            r#"{"properties":{"checked":{"pattern":"x"}}}"#,
            r#"{"checked":1,"opaque":"\ud800"}"#
        ),
        ValueOutcome::Satisfies
    ));
}
#[test]
fn only_reached_string_interpretation_refuses_and_scopes_do_not_leak() {
    // never skip an actual JSON string as if it were another JSON type.
    for (schema, value) in [
        (r#"{"pattern":"a"}"#, r#""\ud800""#),
        (r#"{"minLength":1}"#, r#""\ud800""#),
        (r#"{"maxLength":9}"#, r#""\ud800""#),
        (r#"{"propertyNames":{"pattern":"a"}}"#, r#"{"\ud800":1}"#),
        (r#"{"patternProperties":{"a":true}}"#, r#"{"\ud800":1}"#),
        (
            r#"{"properties":{"checked":{"pattern":"x"}}}"#,
            r#"{"checked":"\ud800"}"#,
        ),
    ] {
        let result = outcome(schema, value);
        assert!(
            matches!(result, ValueOutcome::NoVerdict { ref detail } if detail.reason == NoVerdictReason::UnsupportedCapability && detail.code == "lone-surrogate-instance"),
            "{schema}: {result:?}"
        );
        assert!(matches!(outcome("true", value), ValueOutcome::Satisfies));
        assert!(matches!(
            outcome("false", value),
            ValueOutcome::Fails { .. }
        ));
    }
    let contract = prepare(r#"{"pattern":"a"}"#, "");
    for _ in 0..2 {
        assert!(matches!(
            contract.validate(&JsonValue::parse(r#""\ud800""#).unwrap()),
            ValueOutcome::NoVerdict { .. }
        ));
        assert!(matches!(
            contract.validate(&JsonValue::parse(r#""a""#).unwrap()),
            ValueOutcome::Satisfies
        ));
    }
}
#[test]
fn exact_equality_decides_strings_with_unpaired_units() {
    // Strings and member names are equal exactly when their UTF-16 units are,
    // so equality needs no Unicode scalar interpretation.
    for (schema, value, satisfies) in [
        (r#"{"const":"a"}"#, r#""\ud800""#, false),
        (r#"{"enum":["a"]}"#, r#""\ud800""#, false),
        (r#"{"const":"\ud800"}"#, r#""\ud800""#, true),
        (r#"{"const":"\ud800"}"#, r#""\udc00""#, false),
        (r#"{"const":"\ud800"}"#, r#""s""#, false),
        (r#"{"enum":["a","\ud800"]}"#, r#""\ud800""#, true),
        (r#"{"const":{"\ud800":1}}"#, r#"{"\ud800":1}"#, true),
        (r#"{"const":{"\ud800":1}}"#, r#"{"\udc00":1}"#, false),
        (r#"{"const":{"a":"\ud800"}}"#, r#"{"a":"\ud800"}"#, true),
        (r#"{"uniqueItems":true}"#, r#"["\ud800","\ud800"]"#, false),
        (r#"{"uniqueItems":true}"#, r#"["\ud800","\udc00"]"#, true),
        (
            r#"{"uniqueItems":true}"#,
            r#"[{"\ud800":1},{"\ud800":1}]"#,
            false,
        ),
        (
            r#"{"uniqueItems":true}"#,
            r#"[{"\ud800":1},{"\udc00":1}]"#,
            true,
        ),
    ] {
        let result = outcome(schema, value);
        let expected = if satisfies {
            matches!(result, ValueOutcome::Satisfies)
        } else {
            matches!(result, ValueOutcome::Fails { .. })
        };
        assert!(expected, "{schema} against {value}: {result:?}");
    }
}
#[test]
fn partial_evaluations_use_the_same_string_scope() {
    // both bounds use the runtime access gate, including decisive bounds.
    for (schema, succeeds) in [
        (
            r#"{"anyOf":[true,{"$ref":"https://missing.invalid/schema"}]}"#,
            true,
        ),
        (
            r#"{"allOf":[false,{"$ref":"https://missing.invalid/schema"}]}"#,
            false,
        ),
    ] {
        let result = outcome(schema, r#""\ud800""#);
        assert_eq!(matches!(result, ValueOutcome::Satisfies), succeeds);
        assert_eq!(matches!(result, ValueOutcome::Fails { .. }), !succeeds);
    }
    let result = outcome(
        r#"{"anyOf":[{"pattern":"a"},{"$ref":"https://missing.invalid/schema"}]}"#,
        r#""\ud800""#,
    );
    assert!(
        matches!(result, ValueOutcome::NoVerdict { detail } if detail.code == "lone-surrogate-instance")
    );
}

#[test]
fn visiting_values_does_not_interpret_their_names_and_diagnostics_preserve_failure() {
    // an applicator can visit every value without interpreting name strings.
    assert!(matches!(
        outcome(
            r#"{"additionalProperties":{"type":"integer"}}"#,
            r#"{"\ud800":1}"#
        ),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        outcome(r#"{"additionalProperties":false}"#, r#"{"\ud800":1}"#),
        ValueOutcome::Fails { .. }
    ));
    assert!(matches!(
        outcome(
            r#"{"additionalProperties":{"type":"integer"}}"#,
            r#"{"\ud800":"bad"}"#
        ),
        ValueOutcome::Fails {
            problems_complete: false,
            ..
        }
    ));
    assert!(matches!(
        outcome(r#"{"allOf":[false,{"pattern":"a"}]}"#, r#""\ud800""#),
        ValueOutcome::Fails { .. }
    ));
}

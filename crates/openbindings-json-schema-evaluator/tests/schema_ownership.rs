use openbindings::{Document, Edit, JsonValue, Schema, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;

#[test]
fn named_and_operation_schemas_retain_original_reference_context() {
    let (named, input, changed) = {
        let original = Document::parse(r##"{"openbindings":"0.2.0","operations":{"read":{"input":{"$ref":"#/schemas/number"}}},"schemas":{"number":{"type":"integer"}}}"##).unwrap();
        let proof = original.validate().unwrap().conformant().unwrap();
        let operation = proof.resolve_operation("read").unwrap();
        let changed = original
            .edit(&[Edit::set(
                &["schemas", "number"],
                JsonValue::parse(r#"{"type":"string"}"#).unwrap(),
            )])
            .unwrap();
        (
            proof.schema("number").unwrap(),
            operation.input().unwrap(),
            changed,
        )
    };
    let changed = changed
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .schema("number")
        .unwrap();
    let evaluator = DefaultEvaluator::new();
    for schema in [named.clone(), input] {
        let prepared = schema.prepare(&evaluator).unwrap();
        assert!(matches!(
            prepared.validate(&JsonValue::parse("2").unwrap()),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            prepared.validate(&JsonValue::parse(r#""two""#).unwrap()),
            ValueOutcome::Fails { .. }
        ));
    }
    assert_eq!(named.location().pointer.as_str(), Some("/schemas/number"));
    assert!(matches!(
        changed
            .prepare(&evaluator)
            .unwrap()
            .validate(&JsonValue::parse("2").unwrap()),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn absent_false_true_and_null_schemas_remain_distinct_owned_values() {
    let schemas: Vec<Option<Schema>> = {
        let document = Document::parse(r##"{"openbindings":"0.2.0","operations":{"absent":{},"never":{"input":false},"any":{"input":true},"null":{"input":{"type":"null"}}}}"##).unwrap();
        let proof = document.validate().unwrap().conformant().unwrap();
        ["absent", "never", "any", "null"]
            .into_iter()
            .map(|key| proof.resolve_operation(key).unwrap().input())
            .collect()
    };
    assert!(schemas[0].is_none());
    for (schema, null_matches, number_matches) in [
        (&schemas[1], false, false),
        (&schemas[2], true, true),
        (&schemas[3], true, false),
    ] {
        let prepared = schema
            .as_ref()
            .unwrap()
            .prepare(&DefaultEvaluator::new())
            .unwrap();
        for (text, expected) in [("null", null_matches), ("2", number_matches)] {
            assert_eq!(
                matches!(
                    prepared.validate(&JsonValue::parse(text).unwrap()),
                    ValueOutcome::Satisfies
                ),
                expected
            );
        }
    }
}

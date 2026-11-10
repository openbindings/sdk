//! Original annotation walks keep authored subschemas and exact locations.
//! The former dependency projection and its recursive walk transform are retired.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
struct Resource {
    uri: String,
    schema: Box<RawValue>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    // Raw text keeps each schema's keyword order, which decides what the library compiles first.
    document: Box<RawValue>,
    resources: Vec<Resource>,
    values: Vec<Box<RawValue>>,
    expected: Vec<String>,
}

fn outcome(outcome: &ValueOutcome) -> &'static str {
    match outcome {
        ValueOutcome::Satisfies => "S",
        ValueOutcome::Fails { .. } => "F",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "N",
    }
}

#[test]
fn pattern_subschemas_reached_through_references_stay_evaluable() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/unevaluated-walk-placements.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let parsed = Document::parse(case.document.get()).unwrap();
        let validation = parsed.validate().unwrap();
        let proof = validation.conformant().expect("conformant fixture");
        let resources = ExternalSchemas::new(case.resources.iter().map(|r| ExternalSchema {
            uri: r.uri.clone(),
            document: JsonValue::parse(r.schema.get()).unwrap(),
        }))
        .unwrap();
        let contract = proof
            .resolve_operation("op")
            .expect("declared operation")
            .input()
            .expect("declared input")
            .prepare(&DefaultEvaluator::new(), &resources)
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value.get()).unwrap();
            let label = format!("{} {}", case.id, value.text());
            assert_eq!(outcome(&contract.evaluate(&value)), expected, "{label}");
            assert_eq!(outcome(&contract.explain(&value)), expected, "{label}");
        }
    }
}

// A failing child assertion retains its original patternProperties location
// and the instance member it actually checks.
#[test]
fn pattern_subschema_failures_name_the_original_child_assertion() {
    let document = Document::parse(
        r##"{"openbindings":"0.2.0","schemas":{"p":{"patternProperties":{"^a":{"type":"integer"}}}},"operations":{"op":{"input":{"properties":{"o":{"unevaluatedProperties":false,"$ref":"#/schemas/p"}}}}}}"##,
    )
    .unwrap();
    let validation = document.validate().unwrap();
    let proof = validation.conformant().expect("conformant fixture");
    let contract = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
        )
        .unwrap();
    let value = JsonValue::parse(r#"{"o":{"a":"x"}}"#).unwrap();
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = contract.explain(&value)
    else {
        panic!("the member fails its pattern's schema")
    };
    assert!(problems_complete);
    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0].code, "falseSchema");
    assert_eq!(problems[0].instance, "/o/a");
    assert_eq!(
        problems[0].schema.pointer,
        "/operations/op/input/properties/o/unevaluatedProperties"
    );
    assert_eq!(problems[1].code, "type");
    assert_eq!(problems[1].instance, "/o/a");
    assert_eq!(
        problems[1].schema,
        SchemaOrigin::new(
            None,
            "/schemas/p/patternProperties/^a/type".parse().unwrap()
        )
    );
    assert!(matches!(
        contract.evaluate(&JsonValue::parse(r#"{"o":{"a":1}}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
}

// The original41-reference fixture has no compiler walk-depth limitation.
#[test]
fn the_original_annotation_walk_is_not_compiler_recursion() {
    let mut defs = serde_json::Map::new();
    for i in 0..20 {
        defs.insert(
            format!("a{i}"),
            serde_json::json!({"allOf": [{"$ref": format!("#/operations/op/input/$defs/a{}", i + 1)}]}),
        );
    }
    defs.insert("a20".into(), serde_json::json!({"type": "object"}));
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"inner": {
            "allOf": [{"$ref": "#/operations/op/input/$defs/a0"}],
            "unevaluatedProperties": false
        }},
        "$defs": defs
    });
    let document = Document::parse(
        serde_json::json!({"openbindings": "0.2.0", "operations": {"op": {"input": schema}}})
            .to_string(),
    )
    .unwrap();
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant")
        .clone();
    let operation = proof.resolve_operation("op").unwrap();
    let prepared = operation
        .input()
        .unwrap()
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())
        .unwrap();
    assert!(matches!(
        prepared.evaluate(&JsonValue::parse(r#"{"inner":{}}"#).unwrap()),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepared.evaluate(&JsonValue::parse(r#"{"inner":{"extra":1}}"#).unwrap()),
        ValueOutcome::Fails { .. }
    ));
}

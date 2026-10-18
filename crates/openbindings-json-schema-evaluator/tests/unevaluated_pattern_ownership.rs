//! `unevaluatedProperties` reaches `patternProperties` subschemas through references. The
//! library compiles those subschemas, and the `$ref` targets inside them, while it builds the
//! walk; a later compilation round can share such a target. Every placement of the same schemas
//! must evaluate, with the same verdicts as the single-resource control.
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
        serde_json::from_str(include_str!("fixtures/unevaluated-pattern-ownership.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let parsed = Document::parse(case.document.get()).unwrap();
        let validation = parsed.validate().unwrap();
        let proof = validation.conformant().expect("conformant fixture");
        let resources = ResourceSet::new(case.resources.iter().map(|r| SchemaResource {
            uri: r.uri.clone(),
            document: JsonValue::parse(r.schema.get()).unwrap(),
        }))
        .unwrap();
        let contract = proof
            .resolve_operation("op")
            .expect("declared operation")
            .input()
            .expect("declared input")
            .prepare_with(&DefaultEvaluator::new(), &resources)
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value.get()).unwrap();
            let label = format!("{} {}", case.id, value.text());
            assert_eq!(outcome(&contract.validate(&value)), expected, "{label}");
            assert_eq!(outcome(&contract.explain(&value)), expected, "{label}");
        }
    }
}

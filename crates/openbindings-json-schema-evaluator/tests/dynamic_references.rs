//! A `$dynamicRef` resolves through the dynamic scope of the evaluation path
//! that reaches it (JSON Schema Core §8.2.3.2), also when a `$ref` cycle
//! re-enters a resource that declares a dynamic anchor. Each case's verdicts
//! are JSON Schema's: an independent implementation's, and, where a case has
//! one, those of its control with the cycle unrolled into distinct resources.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Case {
    id: String,
    document: String,
    #[serde(default)]
    resources: BTreeMap<String, String>,
    values: Vec<String>,
    expected: Vec<String>,
}

#[test]
fn dynamic_references_follow_each_paths_scope_through_reference_cycles() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/dynamic-references.json")).unwrap();
    assert_eq!(cases.len(), 9);
    for case in cases {
        let resources = ResourceSet::new(case.resources.iter().map(|(uri, text)| SchemaResource {
            uri: uri.clone(),
            document: JsonValue::parse(text).unwrap(),
        }))
        .unwrap();
        let proof = Document::parse(&case.document)
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant fixture");
        let contract = proof
            .resolve_operation("op")
            .unwrap()
            .input()
            .unwrap()
            .prepare_with(&DefaultEvaluator::new(), &resources)
            .unwrap_or_else(|detail| panic!("{}: {detail:?}", case.id));
        for (value, expected) in case.values.iter().zip(&case.expected) {
            let value = JsonValue::parse(value).unwrap();
            for outcome in [contract.validate(&value), contract.explain(&value)] {
                let verdict = match outcome {
                    ValueOutcome::Satisfies => "S",
                    ValueOutcome::Fails { .. } => "F",
                    other => panic!("{} {}: {other:?}", case.id, value.text()),
                };
                assert_eq!(verdict, expected, "{} {}", case.id, value.text());
            }
        }
    }
}

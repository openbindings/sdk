//! Independent current-spec identity witnesses. A missing semantic capability
//! fails this gate; no preparation refusal substitutes for an expected verdict.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::Value;

#[test]
fn contained_identities_and_fragments_preserve_the_specified_resource() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/contained-identity.json")).unwrap();
    assert_eq!(cases.len(), 182);
    let mut gaps = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let document = Document::parse(
            case["document"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| case["document"].to_string()),
        )
        .unwrap();
        let proof = document
            .validate()
            .unwrap()
            .conformant()
            .expect("admitted identity witness");
        let resources = ResourceSet::new(case["resources"].as_array().unwrap().iter().map(
            |resource| {
                SchemaResource {
                    uri: resource["uri"].as_str().unwrap().to_owned(),
                    document: JsonValue::parse(
                        resource["document"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| resource["document"].to_string()),
                    )
                    .unwrap(),
                }
            },
        ))
        .unwrap();
        let schema = proof.resolve_operation("op").unwrap().input().unwrap();
        let prepared = schema.prepare_with(&DefaultEvaluator::new(), &resources);
        if let Some(code) = case["expected_preparation"].as_str() {
            assert!(
                matches!(prepared, Err(ref refusal) if refusal.code == code),
                "{id}: {prepared:?}"
            );
            continue;
        }
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(refusal) => {
                gaps.push(format!(
                    "{id}: preparation {} instead of {:?}",
                    refusal.code, case["expected"]
                ));
                continue;
            }
        };
        for (value, expected) in case["values"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["expected"].as_array().unwrap())
        {
            let actual = match prepared.validate(&JsonValue::parse(value.to_string()).unwrap()) {
                ValueOutcome::Satisfies => "satisfies",
                ValueOutcome::Fails { .. } => "fails",
                ValueOutcome::Undefined { .. } => "undefined",
                ValueOutcome::NoVerdict { .. } => "no-verdict",
            };
            if expected.as_str() != Some(actual) {
                gaps.push(format!("{id}: expected {expected}, observed {actual}"));
            }
        }
    }
    assert!(gaps.is_empty(), "{}", gaps.join("\n"));
}

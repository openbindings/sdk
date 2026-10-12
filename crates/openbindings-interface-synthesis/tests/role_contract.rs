mod support;
use openbindings::{ContractPreparation, ParsedDocument, ResourceSet, Side, ValueOutcome};
use openbindings_interface_synthesis::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;
use support::*;
#[tokio::test]
async fn sdk_policy_cases_and_interface_output_shapes_are_distinct() {
    let role =
        ParsedDocument::parse(include_bytes!("fixtures/interface-synthesizer-0.2.json")).unwrap();
    let context = role
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare(
        "openbindings.interface-synthesizer.synthesizeInterfaceWithCoverage",
        Side::Output,
    ) else {
        panic!("role contract failed")
    };
    let corpus = corpus();
    let mut total = 0;
    for case in corpus.get("tests").unwrap().elements().unwrap() {
        total += 1;
        let candidate = from_case(case);
        let request = case.get("request").unwrap();
        let owned: Vec<_> = request
            .get("sources")
            .unwrap()
            .elements()
            .unwrap()
            .map(|s| {
                let carrier = ParsedDocument::parse(format!(
                    r#"{{"openbindings":"0.2.0","operations":{{}},"sources":{{"api":{}}}}}"#,
                    s.get("source").unwrap().text()
                ))
                .unwrap()
                .to_authoring()
                .unwrap()
                .sources
                .unwrap()
                .remove("api")
                .unwrap();
                (optional(s, "name"), carrier)
            })
            .collect();
        let inputs: Vec<_> = owned
            .iter()
            .map(|(name, source)| SynthesisSource {
                name: name.as_deref(),
                source,
            })
            .collect();
        let r = SynthesisRequest {
            sources: &inputs,
            ..Default::default()
        };
        let result = Synthesizer::new(Fixed::new(candidate))
            .synthesize_with_coverage(&r)
            .await;
        let expected = case.get("expected").unwrap();
        let valid = expected.get("valid").unwrap().as_bool().unwrap();
        assert_eq!(
            result.is_ok(),
            valid,
            "{}: {result:?}",
            case.get("description").unwrap().text()
        );
        if let Ok(result) = result {
            let expected_keys: Vec<_> = expected
                .get("sourceKeys")
                .unwrap()
                .elements()
                .unwrap()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(result.coverage().source_keys(), expected_keys);
            assert_eq!(
                result.coverage().fully_represented(),
                expected.get("fullyRepresented").unwrap().as_bool().unwrap()
            );
            assert_eq!(
                result.coverage().strict_eligible(),
                expected.get("strictEligible").unwrap().as_bool().unwrap()
            );
            assert!(matches!(
                contract.validate(&result.to_json().unwrap()),
                ValueOutcome::Satisfies
            ));
        }
    }
    assert!(total >= 26);
}

#[test]
fn sdk_policy_fixture_file_satisfies_its_local_schema() {
    let mut draft = openbindings::DocumentBuilder::new();
    draft.operations.insert(
        "check".into(),
        openbindings::Operation {
            input: Some(json(include_str!("fixtures/coverage-fixture.schema.json"))),
            ..Default::default()
        },
    );
    let doc = draft.build().unwrap();
    let context = doc
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare("check", Side::Input) else {
        panic!("fixture schema failed")
    };
    assert!(matches!(
        contract.validate(&corpus()),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn independent_interface_examples_allow_variation_beyond_sdk_policy() {
    let role =
        ParsedDocument::parse(include_bytes!("fixtures/interface-synthesizer-0.2.json")).unwrap();
    let context = role
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare(
        "openbindings.interface-synthesizer.synthesizeInterfaceWithCoverage",
        Side::Output,
    ) else {
        panic!("role output contract failed")
    };
    let examples = json(include_str!("fixtures/interface-output-cases.json"));
    for case in examples.get("tests").unwrap().elements().unwrap() {
        let wire = json(&format!(
            r#"{{"interface":{},"coverage":{}}}"#,
            case.get("interface").unwrap().text(),
            case.get("report").unwrap().text(),
        ));
        let expected = case.get("schemaValid").unwrap().as_bool().unwrap();
        let outcome = contract.validate(&wire);
        assert!(
            matches!(
                outcome,
                ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
            ),
            "undetermined example: {outcome:?}"
        );
        assert_eq!(
            matches!(outcome, ValueOutcome::Satisfies),
            expected,
            "{}: {outcome:?}",
            case.get("description").unwrap().text()
        );
    }
}

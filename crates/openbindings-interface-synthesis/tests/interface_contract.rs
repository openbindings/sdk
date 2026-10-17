mod support;
use openbindings::{Document, ValueOutcome};
use openbindings_interface_synthesis::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use support::*;
#[tokio::test]
async fn sdk_policy_cases_and_interface_output_shapes_are_distinct() {
    let interface =
        Document::parse(include_bytes!("fixtures/interface-synthesizer-0.2.json")).unwrap();
    let proof = interface
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("openbindings.interface-synthesizer.synthesizeInterfaceWithCoverage")
        .and_then(|operation| operation.output()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("interface contract failed")
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
                let carrier = read_source(s.get("source").unwrap());
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
    let mut draft = openbindings::Document::new();
    draft
        .set_operation(
            "check",
            &openbindings::Operation {
                input: Some(json(include_str!("fixtures/coverage-fixture.schema.json"))),
                ..Default::default()
            },
        )
        .unwrap();
    let doc = draft.clone();
    let proof = doc
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("check")
        .and_then(|operation| operation.input()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("fixture schema failed")
    };
    assert!(matches!(
        contract.validate(&corpus()),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn independent_interface_examples_allow_variation_beyond_sdk_policy() {
    let interface =
        Document::parse(include_bytes!("fixtures/interface-synthesizer-0.2.json")).unwrap();
    let proof = interface
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let Some(contract) = proof
        .resolve_operation("openbindings.interface-synthesizer.synthesizeInterfaceWithCoverage")
        .and_then(|operation| operation.output()?.prepare(&DefaultEvaluator::new()).ok())
    else {
        panic!("interface output contract failed")
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

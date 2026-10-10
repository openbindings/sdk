use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;
use std::sync::Arc;

fn evaluate(document: &ParsedDocument, resources: ResourceSet) -> ValueOutcome {
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), resources)
        .unwrap();
    match context.prepare("op", Side::Input) {
        ContractPreparation::Ready(contract) => contract.validate(&JsonValue::integer(7)),
        ContractPreparation::NoVerdict { detail } => ValueOutcome::NoVerdict { detail },
        other => panic!("unexpected setup: {other:?}"),
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = &args[1];
    let uri = &args[2];
    if mode == "positive" {
        let schema = json!({"$ref": uri});
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .unwrap();
        assert!(!validator.is_valid(&json!(7)));
        println!(
            "{}",
            json!({"mode": mode, "retrievalControl": "false remote resource evaluated"})
        );
        return;
    }
    assert_eq!(mode, "sdk");
    let mut observations = Vec::new();
    for key in ["$ref", "$schema"] {
        let document = ParsedDocument::parse(
            json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$id":"urn:no-io:boundary",key:uri}}}}).to_string(),
        ).unwrap();
        let result = evaluate(&document, ResourceSet::default());
        assert!(matches!(result, ValueOutcome::NoVerdict { .. }));
        observations.push(json!({"name":key,"outcome":result}));
    }
    let document = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":true}},"sources":{"source":{"kind":uri,"content":{"$ref":uri,"$schema":uri}}},"bindings":{"binding":{"source":"source","operation":"op"}}}).to_string(),
    ).unwrap();
    assert!(document.assess().unwrap().validated().is_some());
    assert!(matches!(
        evaluate(&document, ResourceSet::default()),
        ValueOutcome::Satisfies
    ));
    observations.push(json!({"name":"kind/content ignored","outcome":"satisfies"}));
    let resource = SchemaResource {
        uri: uri.clone(),
        document: JsonValue::parse("false").unwrap(),
    };
    let document = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":uri}}}}).to_string(),
    )
    .unwrap();
    assert!(matches!(
        evaluate(&document, ResourceSet::new([resource]).unwrap()),
        ValueOutcome::Fails { .. }
    ));
    observations.push(json!({"name":"explicit resource only","outcome":"fails"}));
    println!("{}", json!({"mode":mode,"observations":observations}));
}

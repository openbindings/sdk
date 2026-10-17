use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;

fn evaluate(document: &Document, resources: ResourceSet) -> ValueOutcome {
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = proof.resolve_operation("op").expect("declared operation");
    let input = operation.input().expect("declared input");
    match input.prepare_with(&DefaultEvaluator::new(), &resources) {
        Ok(contract) => contract.validate(&JsonValue::integer(7)),
        Err(detail) => ValueOutcome::NoVerdict { detail },
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
        // A foreign dialect belongs in an explicitly supplied resource (§5.2),
        // while the OpenBindings document itself must pass proof admission.
        let schema = json!({"$id":"urn:no-io:boundary",key:uri});
        let (input, resources) = if key == "$schema" {
            (
                json!({"$ref":"urn:no-io:boundary"}),
                ResourceSet::new([SchemaResource {
                    uri: "urn:no-io:boundary".into(),
                    document: JsonValue::parse(schema.to_string()).unwrap(),
                }])
                .unwrap(),
            )
        } else {
            (schema, ResourceSet::default())
        };
        let document = Document::parse(
            json!({"openbindings":"0.2.0","operations":{"op":{"input":input}}}).to_string(),
        )
        .unwrap();
        let result = evaluate(&document, resources);
        assert!(matches!(result, ValueOutcome::NoVerdict { .. }));
        observations.push(json!({"name":key,"outcome":result}));
    }
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":true}},"sources":{"source":{"kind":uri,"content":{"$ref":uri,"$schema":uri}}},"bindings":{"binding":{"source":"source","operation":"op"}}}).to_string(),
    ).unwrap();
    assert!(document.validate().unwrap().conformant().is_some());
    assert!(matches!(
        evaluate(&document, ResourceSet::default()),
        ValueOutcome::Satisfies
    ));
    observations.push(json!({"name":"kind/content ignored","outcome":"satisfies"}));
    let resource = SchemaResource {
        uri: uri.clone(),
        document: JsonValue::parse("false").unwrap(),
    };
    let document = Document::parse(
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

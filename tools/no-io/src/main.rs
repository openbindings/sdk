use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::json;

fn evaluate(
    document: &Document,
    resources: ExternalSchemas,
) -> Result<ValueOutcome, PreparationFailure> {
    let proof = document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = proof.resolve_operation("op").expect("declared operation");
    let input = operation.input().expect("declared input");
    let schema = input.prepare(
        &DefaultEvaluator::new(),
        &resources,
        &PreparationLimits::default(),
    )?;
    Ok(schema.evaluate(&JsonValue::integer(7)))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = &args[1];
    let uri = &args[2];
    // The library, with retrieval enabled, fetches a remote reference itself.
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
        // A foreign dialect belongs in an explicitly supplied resource (§5.5),
        // while the OpenBindings document itself must pass proof admission.
        let schema = json!({"$id":"urn:no-io:boundary",key:uri});
        let (input, resources) = if key == "$schema" {
            (
                json!({"$ref":"urn:no-io:boundary"}),
                ExternalSchemas::new([ExternalSchema {
                    uri: "urn:no-io:boundary".into(),
                    document: JsonValue::parse(schema.to_string()).unwrap(),
                }])
                .unwrap(),
            )
        } else {
            (schema, ExternalSchemas::default())
        };
        let document = Document::parse(
            json!({"openbindings":"0.2.0","operations":{"op":{"input":input}}}).to_string(),
        )
        .unwrap();
        let result = evaluate(&document, resources);
        if key == "$schema" {
            // The original-node plan retains an unknown custom profile as an
            // unsupported capability. Value evaluation cannot decide it and
            // never retrieves a dialect to fill that gap.
            assert!(
                matches!(
                    &result,
                    Ok(ValueOutcome::NoVerdict { detail })
                        if detail.reason == NoVerdictReason::UnsupportedCapability
                ),
                "{}",
                serde_json::to_string(&result).unwrap()
            );
        } else {
            assert!(
                matches!(&result, Ok(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::ResourceUnavailable),
                "{}",
                serde_json::to_string(&result).unwrap()
            );
        }
        observations.push(json!({"name":key,"outcome":result}));
    }
    // Once the custom profile is known from supplied bytes, a missing resource
    // demanded by its meta-schema remains unavailable. Meta admission must not
    // activate dependency retrieval either.
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"urn:no-io:consumer"}}}})
            .to_string(),
    )
    .unwrap();
    let resources = ExternalSchemas::new([
        ExternalSchema {
            uri: "urn:no-io:consumer".into(),
            document: JsonValue::parse(
                json!({"$schema":"urn:no-io:meta","type":"integer"}).to_string(),
            )
            .unwrap(),
        },
        ExternalSchema {
            uri: "urn:no-io:meta".into(),
            document: JsonValue::parse(
                json!({
                    "$schema":"https://json-schema.org/draft/2020-12/schema",
                    "$vocabulary": {
                        "https://json-schema.org/draft/2020-12/vocab/core":true,
                        "https://json-schema.org/draft/2020-12/vocab/applicator":true,
                        "https://json-schema.org/draft/2020-12/vocab/validation":true
                    },
                    "$ref":uri
                })
                .to_string(),
            )
            .unwrap(),
        },
    ])
    .unwrap();
    let result = evaluate(&document, resources);
    assert!(
        matches!(&result, Err(PreparationFailure {
        kind: PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable { uri: missing }
        }, ..
    }) if missing == uri),
        "{}",
        serde_json::to_string(&result).unwrap()
    );
    observations.push(json!({"name":"known meta dependency unavailable","outcome":result}));
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":true}},"sources":{"source":{"kind":uri,"data":{"$ref":uri,"$schema":uri}}},"bindings":{"binding":{"source":"source","operation":"op"}}}).to_string(),
    ).unwrap();
    assert!(document.validate().unwrap().conformant().is_some());
    assert!(matches!(
        evaluate(&document, ExternalSchemas::default()),
        Ok(ValueOutcome::Satisfies)
    ));
    observations.push(json!({"name":"kind/data ignored","outcome":"satisfies"}));
    let resource = ExternalSchema {
        uri: uri.clone(),
        document: JsonValue::parse("false").unwrap(),
    };
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":uri}}}}).to_string(),
    )
    .unwrap();
    assert!(matches!(
        evaluate(&document, ExternalSchemas::new([resource]).unwrap()),
        Ok(ValueOutcome::Fails { .. })
    ));
    observations.push(json!({"name":"explicit resource only","outcome":"fails"}));
    println!("{}", json!({"mode":mode,"observations":observations}));
}

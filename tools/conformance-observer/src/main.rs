use base64::Engine;
use openbindings::*;
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn observe(request: &Value) -> Value {
    let given = &request["given"];
    let data = base64::engine::general_purpose::STANDARD
        .decode(given["documentBase64"].as_str().unwrap())
        .unwrap();
    let mut out = json!({"id":request["id"],"executed":true});
    match request["action"].as_str().unwrap() {
        "validate-document" => match validate_document(&data) {
            Ok(result) => {
                let report = result.report();
                out["outcome"] = json!(report.conclusion);
                // Translate SDK vocabulary to the frozen conformance-observation protocol.
                out["rules"] = json!(
                    report
                        .evidence
                        .iter()
                        .map(|(rule, evidence)| {
                            (
                                *rule,
                                if *evidence == Evidence::Met {
                                    json!("satisfied")
                                } else {
                                    json!(evidence)
                                },
                            )
                        })
                        .collect::<std::collections::BTreeMap<_, _>>()
                );
                out["findings"] = json!(report.findings);
                if let Some(findings) = out["findings"].as_array_mut() {
                    for finding in findings {
                        if finding["status"] == "met" {
                            finding["status"] = json!("satisfied");
                        }
                        if matches!(
                            (finding["rule"].as_str(), finding["code"].as_str()),
                            (Some("OBI-06" | "OBI-08"), Some("unknown-operation"))
                                | (Some("OBI-07"), Some("unknown-source"))
                        ) {
                            finding["code"] = json!("missing-target");
                        }
                    }
                }
            }
            Err(error) => {
                out["outcome"] = json!("version-refused");
                out["refusal"] = json!(error);
            }
        },
        "resolve-operation" => {
            let doc = Document::parse(&data).unwrap();
            match doc
                .resolve_operation(given["name"].as_str().unwrap())
                .unwrap()
            {
                OperationSelection::Found(op) => {
                    out["outcome"] = json!("resolved");
                    out["bindingKeys"] = json!(op.bindings().unwrap());
                    out["operationKey"] = json!(op.key());
                }
                OperationSelection::Missing | OperationSelection::Ambiguous { .. } => {
                    out["outcome"] = json!("not-found")
                }
            }
        }
        "check-dependency-kind" => {
            let doc = Document::parse(&data).unwrap();
            let binding = given["binding"].as_str().unwrap();
            let source = doc
                .value()
                .get("bindings")
                .unwrap()
                .get(binding)
                .unwrap()
                .get("source")
                .unwrap()
                .as_str()
                .unwrap();
            let kind = doc
                .value()
                .get("sources")
                .unwrap()
                .get(source)
                .unwrap()
                .get("kind")
                .unwrap()
                .as_str()
                .unwrap();
            out["outcome"] = json!(if doc
                .dependency_accepts_kind(given["dependency"].as_str().unwrap(), kind)
                .unwrap()
                .unwrap()
            {
                "meets"
            } else {
                "does-not-meet"
            });
        }
        "validate-operation-values" | "check-examples" => {
            let doc = Document::parse(&data).unwrap();
            let resources = ResourceSet::new(
                given["resources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| SchemaResource {
                        uri: r["uri"].as_str().unwrap().into(),
                        document: JsonValue::parse(r["documentJson"].as_str().unwrap()).unwrap(),
                    }),
            )
            .unwrap();
            let contracts =
                doc.validate()
                    .unwrap()
                    .conformant()
                    .expect("conformant document")
                    .value_contracts(
                        std::sync::Arc::new(
                            openbindings_json_schema_evaluator::DefaultEvaluator::new(),
                        ),
                        resources,
                    );
            let operation = given["operation"].as_str().unwrap();
            if request["action"] == "validate-operation-values" {
                let direction = if given["side"] == "input" {
                    Direction::Input
                } else {
                    Direction::Output
                };
                let prepared = contracts.prepare(operation, direction);
                out["values"] = json!(
                    given["valuesJson"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| observe_value(
                            &prepared,
                            &JsonValue::parse(v.as_str().unwrap()).unwrap()
                        ))
                        .collect::<Vec<_>>()
                );
            } else {
                let OperationSelection::Found(op) = doc.resolve_operation(operation).unwrap()
                else {
                    panic!("fixture operation must resolve")
                };
                let mut results = Vec::new();
                if let Some(examples) = op.value().get("examples").and_then(|v| v.members()) {
                    for example in examples {
                        for direction in [Direction::Input, Direction::Output] {
                            if let Some(value) = example.value.get(direction.as_str()) {
                                let mut result = observe_value(
                                    &contracts.prepare(operation, direction),
                                    &value.to_owned(),
                                );
                                result["example"] = json!(example.name.as_str().unwrap());
                                result["side"] = json!(direction.as_str());
                                results.push(result);
                            }
                        }
                    }
                }
                out["examples"] = json!(results);
            }
        }
        _ => {
            out["executed"] = json!(false);
            out["outcome"] = json!("not-implemented");
        }
    }
    out
}
fn main() {
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        println!("{}", observe(&request));
    }
}

// Corpus observation format predates setup/value separation. Preserve its wire
// categories explicitly without manufacturing a compiled contract or value verdict.
fn observe_value(preparation: &ValueContractPreparation, value: &JsonValue) -> Value {
    match preparation {
        ValueContractPreparation::Ready(contract) => {
            serde_json::to_value(contract.validate(value)).unwrap()
        }
        ValueContractPreparation::NoValueContract => json!({"outcome":"no-contract"}),
        ValueContractPreparation::OperationMissing => json!({"outcome":"operation-missing"}),
        ValueContractPreparation::OperationAmbiguous { .. } => {
            json!({"outcome":"operation-missing"})
        }
        ValueContractPreparation::NoVerdict { detail } => {
            json!({"outcome":"no-verdict","detail":detail})
        }
    }
}

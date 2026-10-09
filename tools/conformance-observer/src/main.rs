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
        "validate-document" => match assess_document(&data) {
            Ok(result) => {
                let report = result.report();
                out["outcome"] = json!(report.conclusion);
                out["rules"] = json!(report.evidence);
                out["findings"] = json!(report.findings);
            }
            Err(error) => {
                out["outcome"] = json!("version-refused");
                out["refusal"] = json!(error);
            }
        },
        "resolve-operation" => {
            let doc = ParsedDocument::parse(&data).unwrap();
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
            let doc = ParsedDocument::parse(&data).unwrap();
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
            let doc = ParsedDocument::parse(&data).unwrap();
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
            let contracts=doc.value_contracts(std::sync::Arc::new(openbindings_json_schema_evaluator::DefaultEvaluator::new()),resources).unwrap();
            let operation = given["operation"].as_str().unwrap();
            if request["action"] == "validate-operation-values" {
                let side = if given["side"] == "input" {
                    Side::Input
                } else {
                    Side::Output
                };
                let prepared = contracts.prepare(operation, side);
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
                        for side in [Side::Input, Side::Output] {
                            if let Some(value) = example.value.get(side.as_str()) {
                                let mut result = observe_value(
                                    &contracts.prepare(operation, side),
                                    &value.to_owned(),
                                );
                                result["example"] = json!(example.name.as_str().unwrap());
                                result["side"] = json!(side.as_str());
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
fn observe_value(preparation: &ContractPreparation, value: &JsonValue) -> Value {
    match preparation {
        ContractPreparation::Ready(contract) => {
            serde_json::to_value(contract.validate(value)).unwrap()
        }
        ContractPreparation::NoContract => json!({"outcome":"no-contract"}),
        ContractPreparation::OperationMissing => json!({"outcome":"operation-missing"}),
        ContractPreparation::OperationAmbiguous { .. } => json!({"outcome":"operation-missing"}),
        ContractPreparation::NoVerdict { detail } => {
            json!({"outcome":"no-verdict","detail":detail})
        }
    }
}

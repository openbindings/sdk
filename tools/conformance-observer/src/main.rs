use base64::Engine;
use openbindings::*;
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn observe(request: &Value) -> String {
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
                return with_findings(out, &report.findings);
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
            let proof = doc
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant document");
            let evaluator = openbindings_json_schema_evaluator::DefaultEvaluator::new();
            let operation = given["operation"].as_str().unwrap();
            if request["action"] == "validate-operation-values" {
                let side = if given["side"] == "input" {
                    "input"
                } else {
                    "output"
                };
                let prepared = prepare(&proof, operation, side, &evaluator, &resources);
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
                        for side in ["input", "output"] {
                            if let Some(value) = example.value.get(side) {
                                let mut result = observe_value(
                                    &prepare(&proof, operation, side, &evaluator, &resources),
                                    &value.to_owned(),
                                );
                                result["example"] = json!(example.name.as_str().unwrap());
                                result["side"] = json!(side);
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
    out.to_string()
}
// Findings in the frozen protocol's vocabulary, written as JSON text: an exact
// pointer through a member name with an unpaired unit has no `Value` form.
fn with_findings(out: Value, findings: &[Finding]) -> String {
    let rows: Vec<String> = findings
        .iter()
        .map(|finding| {
            let status = if finding.status == Evidence::Met {
                json!("satisfied")
            } else {
                json!(finding.status)
            };
            let code = match (finding.rule, finding.code) {
                ("OBI-06" | "OBI-08", "unknown-operation") | ("OBI-07", "unknown-source") => {
                    "missing-target"
                }
                (_, code) => code,
            };
            format!(
                r#"{{"code":{},"location":{},"message":{},"rule":{},"status":{status}}}"#,
                serde_json::to_string(code).unwrap(),
                serde_json::to_string(&finding.location).unwrap(),
                serde_json::to_string(&finding.message).unwrap(),
                serde_json::to_string(finding.rule).unwrap(),
            )
        })
        .collect();
    let mut observation = out.to_string();
    observation.pop();
    observation.push_str(r#","findings":["#);
    observation.push_str(&rows.join(","));
    observation.push_str("]}");
    observation
}
fn main() {
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        println!("{}", observe(&request));
    }
}

// Corpus observation format predates setup/value separation. Preserve its wire
// categories explicitly without manufacturing a compiled contract or value verdict.
enum Preparation {
    OperationMissing,
    NoContract,
    Prepared(Result<PreparedContract, NoVerdict>),
}
fn prepare(
    proof: &Conformant<Document>,
    operation: &str,
    side: &str,
    evaluator: &dyn SchemaEvaluator,
    resources: &ResourceSet,
) -> Preparation {
    let Some(operation) = proof.resolve_operation(operation) else {
        return Preparation::OperationMissing;
    };
    let contract = if side == "input" {
        operation.input()
    } else {
        operation.output()
    };
    match contract {
        Some(contract) => Preparation::Prepared(contract.prepare_with(evaluator, resources)),
        None => Preparation::NoContract,
    }
}
fn observe_value(preparation: &Preparation, value: &JsonValue) -> Value {
    match preparation {
        Preparation::Prepared(Ok(contract)) => {
            serde_json::to_value(contract.validate(value)).unwrap()
        }
        Preparation::NoContract => json!({"outcome":"no-contract"}),
        Preparation::OperationMissing => json!({"outcome":"operation-missing"}),
        Preparation::Prepared(Err(detail)) => {
            json!({"outcome":"no-verdict","detail":detail})
        }
    }
}

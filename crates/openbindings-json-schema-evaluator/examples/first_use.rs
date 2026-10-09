//! Run with: cargo run -p openbindings-json-schema-evaluator --example first_use
use openbindings::{
    ContractPreparation, JsonValue, OperationSelection, ParsedDocument, ResourceSet, Side,
    ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Serialize;
use std::{error::Error, sync::Arc};

#[derive(Serialize)]
struct Input {
    id: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let document = ParsedDocument::parse(
        r#"{
          "openbindings": "0.2.0",
          "operations": {
            "lookup": {
              "description": "Find an item",
              "aliases": ["find"],
              "input": {"type":"object", "properties":{"id":{"type":"integer","minimum":9007199254740993}}, "required":["id"]}
            }
          }
        }"#,
    )?;
    let assessment = document.assess()?;
    if assessment.validated().is_none() {
        for finding in &assessment.report().findings {
            eprintln!(
                "{} {} at {:?}: {}",
                finding.rule, finding.code, finding.location, finding.message
            );
        }
        return Err(format!("document conformance: {:?}", assessment.report().conclusion).into());
    }
    // Primary names and aliases use the same selection API.
    let operation = match document.resolve_operation("find")? {
        OperationSelection::Found(operation) => operation,
        OperationSelection::Missing => return Err("operation is missing".into()),
        OperationSelection::Ambiguous { candidates } => {
            return Err(format!("operation is ambiguous: {candidates:?}").into());
        }
    };
    println!(
        "{}: {}",
        operation.key(),
        operation.description()?.unwrap_or("")
    );
    if let Some(schema) = operation.input() {
        // Exact schema access returns Option; interpreting metadata can also fail.
        println!("input schema at {:?}", schema.location().pointer);
    }
    // The evaluator is an explicit optional companion. No resources are fetched.
    let context =
        document.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
    let input = match context.prepare("lookup", Side::Input) {
        ContractPreparation::Ready(input) => input,
        ContractPreparation::NoContract => return Err("operation has no input contract".into()),
        ContractPreparation::OperationMissing => return Err("operation is missing".into()),
        ContractPreparation::OperationAmbiguous { candidates } => {
            return Err(format!("operation is ambiguous: {candidates:?}").into());
        }
        ContractPreparation::NoVerdict { detail } => {
            return Err(format!(
                "preparation refused ({:?}, {}): {}",
                detail.reason, detail.code, detail.message
            )
            .into());
        }
    };
    // Ordinary values have checked Serde admission. Exact text preserves its tokens.
    let ordinary = JsonValue::from_serializable(&Input {
        id: 9_007_199_254_740_993,
    })?;
    let exact = JsonValue::parse(r#"{"id":9007199254740992}"#)?;
    for value in [&ordinary, &exact] {
        match input.validate(value) {
            ValueOutcome::Satisfies => println!("input satisfies the contract"),
            ValueOutcome::Mismatch {
                problems,
                problems_complete,
            } => {
                for problem in problems {
                    println!(
                        "mismatch at {} (schema {:?}): {}",
                        problem.instance_pointer, problem.schema_location, problem.message
                    );
                }
                println!("selected diagnostics complete: {problems_complete}");
            }
            ValueOutcome::NoVerdict { detail } => {
                println!(
                    "no verdict ({:?}, {}): {}",
                    detail.reason, detail.code, detail.message
                );
            }
        }
    }
    Ok(())
}

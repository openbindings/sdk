//! Run with: cargo run -p openbindings-json-schema-evaluator --example first_use
use openbindings::{
    ConformanceReport, ContractPreparation, JsonValue, OperationSelection, ParsedDocument,
    ResourceSet, Side, ValueOutcome,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Serialize;
use std::{error::Error, sync::Arc};

#[derive(Serialize)]
struct Input {
    id: u64,
}

fn print_findings(report: &ConformanceReport) {
    println!("document conformance: {:?}", report.conclusion);
    for finding in &report.findings {
        // Debug quotes/escapes caller-controlled pointers, including control characters.
        let at = match &finding.location {
            Some(location) => format!(
                "pointer {:?}, line {}, UTF-8 byte column {}, byte offset {}",
                location.pointer, location.line, location.byte_column, location.byte_offset
            ),
            None => "location unavailable".into(),
        };
        println!(
            "{}/{} ({:?}) at {at}: {}",
            finding.rule, finding.code, finding.status, finding.message
        );
    }
    if report.findings_truncated {
        println!("More findings were omitted; the report's rule evidence remains available.");
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let draft_text = r#"{
          "openbindings": "0.2.0",
          "operations": {
            "lookup": {
              "description": "Find an item",
              "aliases": ["find"],
              "inputSchema": {"type":"object", "properties":{"id":{"type":"integer","minimum":9007199254740993}}, "required":["id"]}
            }
          }
        }"#;
    // Valid JSON can still contain an unrecognized normative member.
    let draft = ParsedDocument::parse(draft_text)?;
    print_findings(draft.assess()?.report());
    // The application corrects this known fixture's original text, not a normalized value
    // or a parsed diagnostic message. Existing snapshots remain immutable.
    let corrected_text = draft_text.replace("\"inputSchema\":", "\"input\":");
    println!("corrected source:\n{corrected_text}");
    let document = ParsedDocument::parse(corrected_text)?;
    let assessment = document.assess()?;
    let Some(proof) = assessment.validated() else {
        print_findings(assessment.report());
        return Err(format!("document conformance: {:?}", assessment.report().conclusion).into());
    };
    println!("corrected document is conformant");
    // Primary names and aliases use the same selection API.
    let operation = match proof.parsed().resolve_operation("find")? {
        OperationSelection::Found(operation) => operation,
        OperationSelection::Missing => return Err("operation is missing".into()),
        OperationSelection::Ambiguous { candidates } => {
            return Err(format!("operation is ambiguous: {candidates:?}").into());
        }
    };
    println!(
        "{:?}: {:?}",
        operation.key(),
        operation.description()?.unwrap_or("")
    );
    if let Some(schema) = operation.input() {
        // Exact schema access returns Option; interpreting metadata can also fail.
        println!("input schema at {:?}", schema.location().pointer);
    }
    // The evaluator is an explicit optional companion. No resources are fetched.
    let context = proof
        .parsed()
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
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
            ValueOutcome::Fails {
                problems,
                problems_complete,
            } => {
                for problem in problems {
                    println!(
                        "mismatch at {:?} (schema {:?}): {}",
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

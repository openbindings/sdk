//! Run with: cargo run -p openbindings-json-schema-evaluator --example first_use
use openbindings::{Document, JsonValue, ValidationReport, ValueOutcome};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde::Serialize;
use std::error::Error;

#[derive(Serialize)]
struct Input {
    id: u64,
}

fn print_findings(report: &ValidationReport) {
    println!("document conformance: {:?}", report.conclusion);
    for finding in &report.findings {
        // Pointers are caller-controlled text: Debug of the string quotes and
        // escapes it, including control characters.
        let at = match &finding.location {
            Some(location) => format!(
                "pointer {:?}, line {}, UTF-8 byte column {}, byte offset {}",
                location.pointer.as_ref().map(ToString::to_string),
                location.line,
                location.byte_column,
                location.byte_offset
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
    let draft = Document::parse(draft_text)?;
    print_findings(draft.validate()?.report());
    // The application corrects this known fixture's original text, not a normalized value
    // or a parsed diagnostic message. Existing snapshots remain immutable.
    let corrected_text = draft_text.replace("\"inputSchema\":", "\"input\":");
    println!("corrected source:\n{corrected_text}");
    let document = Document::parse(corrected_text)?;
    let validation = document.validate()?;
    let Some(proof) = validation.conformant() else {
        print_findings(validation.report());
        return Err(format!("document conformance: {:?}", validation.report().conclusion).into());
    };
    println!("corrected document is conformant");
    // Primary names and aliases use the same selection API.
    let operation = proof
        .resolve_operation("find")
        .ok_or("operation is missing")?;
    println!("{:?}: {:?}", operation.key(), operation.description());
    let contract = operation.input().ok_or("operation has no input contract")?;
    // Conformant reads preserve presence; schema preparation remains separate.
    let schema = contract.schema().location();
    println!(
        "input schema at {:?}, line {}",
        schema.pointer.as_ref().map(ToString::to_string),
        schema.line
    );
    // Default budgets and no supplied resources.
    let input = contract.prepare(&DefaultEvaluator::new())?;
    // Ordinary values have checked Serde admission. Exact text preserves its tokens.
    let ordinary = JsonValue::from_serializable(&Input {
        id: 9_007_199_254_740_993,
    })?;
    let exact = JsonValue::parse(r#"{"id":9007199254740992}"#)?;
    for value in [&ordinary, &exact] {
        // The cheap check decides; an explanation of the same value says why.
        let outcome = match input.validate(value) {
            ValueOutcome::Fails { .. } => input.explain(value),
            outcome => outcome,
        };
        match outcome {
            ValueOutcome::Satisfies => println!("input satisfies the contract"),
            ValueOutcome::Fails {
                problems,
                problems_complete,
                message,
            } => {
                println!("{message}");
                for problem in problems {
                    // The value and the document each find their side's line
                    // and column when asked. Pointers are caller-controlled
                    // text: Debug of the string quotes and escapes it.
                    let instance = value.locate(&problem.instance).ok_or("instance location")?;
                    let schema = proof.locate(&problem.schema).ok_or("schema location")?;
                    println!(
                        "mismatch at {:?}, line {}, column {} (schema {:?}, line {}, column {}): {}",
                        problem.instance.to_string(),
                        instance.line,
                        instance.byte_column,
                        problem.schema.pointer.to_string(),
                        schema.line,
                        schema.byte_column,
                        problem.message
                    );
                }
                println!("selected diagnostics complete: {problems_complete}");
            }
            ValueOutcome::Undefined { detail } => {
                println!("undefined result: {}", detail.message);
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

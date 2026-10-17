//! Reproduce the cost of an unanchored search with the default regex allowance.
use openbindings::{Document, JsonValue, NoVerdictReason, ValueOutcome};
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"search":{"input":{"type":"string","pattern":"[A-Z]+[0-9]+"}}}}"#,
    )?;
    let conformant = document.validate()?.conformant().ok_or("invalid fixture")?;
    let operation = conformant
        .resolve_operation("search")
        .ok_or("fixture declares search")?;
    let Ok(contract) = operation
        .input()
        .ok_or("fixture declares an input")?
        .prepare(&DefaultEvaluator::new())
    else {
        return Err("pattern did not prepare".into());
    };
    println!("regex_steps={}", Limits::default().regex_steps);
    for bytes in [1_024, 65_536, 1_048_576] {
        let value = JsonValue::string(&"a".repeat(bytes))?;
        for _ in 0..2 {
            let _ = black_box(contract.validate(&value));
        }
        let mut elapsed = Vec::new();
        let mut result = None;
        for _ in 0..5 {
            let start = Instant::now();
            let outcome = black_box(contract.validate(black_box(&value)));
            elapsed.push(start.elapsed());
            result = Some(outcome);
        }
        elapsed.sort_unstable();
        let outcome = result.expect("five samples");
        match &outcome {
            ValueOutcome::Fails { .. } => println!(
                "bytes={bytes} outcome=fails median_ms={:.3}",
                elapsed[2].as_secs_f64() * 1000.0
            ),
            ValueOutcome::NoVerdict { detail } => {
                assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
                assert_eq!(detail.code, "regex-work-limit");
                println!(
                    "bytes={bytes} outcome=no-verdict code={} median_ms={:.3}",
                    detail.code,
                    elapsed[2].as_secs_f64() * 1000.0
                );
            }
            ValueOutcome::Undefined { .. } => return Err("fixture has no undefined result".into()),
            ValueOutcome::Satisfies => return Err("lowercase fixture cannot match".into()),
        }
        if bytes == 1_048_576 {
            assert!(matches!(outcome, ValueOutcome::NoVerdict { .. }));
        }
    }
    Ok(())
}

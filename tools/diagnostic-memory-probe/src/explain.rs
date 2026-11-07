//! Measure actual requested heap bytes for repeated, oversized schema facts.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn hostile_enum_facts_stay_within_the_diagnostic_allowance() {
    const ALLOWANCE: usize = 2 * 1024 * 1024;
    const OCCURRENCES: usize = 96;
    let choices = (0..64)
        .map(|i| format!("{i}:{}", "s".repeat(16_384)))
        .collect::<Vec<_>>();
    let enum_bytes = choices.iter().map(|v| v.len() + 2).sum::<usize>();
    let text = serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":{
        "items":{"enum":choices}
    }}}})
    .to_string();
    let proof = Document::parse(&text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let prepared = proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::with_limits(Limits {
                evaluation_sdk_reservation_bytes: ALLOWANCE,
                ..Limits::default()
            }),
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
        )
        .unwrap();
    let value =
        JsonValue::parse(serde_json::to_string(&vec![false; OCCURRENCES]).unwrap()).unwrap();
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = prepared.evaluate(&value)
    else {
        panic!("established failure");
    };
    assert!(problems.is_empty());
    assert!(!problems_complete);
    // Schema, instance and prepared owner precede measurement. Realloc overlap
    // is included by the probe allocator; no serialization occurs inside it.
    let before = crate::LIVE.load(Relaxed);
    let (outcome, peak, _) = crate::measured(|| prepared.explain(&value));
    let retained = crate::LIVE.load(Relaxed).saturating_sub(before);
    assert!(peak <= ALLOWANCE, "peak {peak} exceeds {ALLOWANCE}");
    assert!(
        retained <= ALLOWANCE,
        "retained {retained} exceeds {ALLOWANCE}"
    );
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = &outcome
    else {
        panic!("diagnostics cannot change the verdict");
    };
    assert_eq!(problems.len(), OCCURRENCES);
    assert!(!problems_complete);
    let mut populated = 0;
    for problem in problems {
        assert_eq!(problem.code, "enum");
        if let Some(ValueProblemDetails::Enum { choices: facts }) = &problem.details {
            populated += 1;
            assert_eq!(facts.len(), choices.len(), "no partial enum list");
            for (fact, choice) in facts.iter().zip(&choices) {
                assert_eq!(fact, &serde_json::to_string(choice).unwrap());
            }
        } else {
            assert!(problem.details.is_none());
        }
    }
    assert_eq!(
        populated, 1,
        "one whole enum fits; subsequent copies do not"
    );
    drop(outcome);
    assert_eq!(
        crate::LIVE.load(Relaxed),
        before,
        "explanation retains no allocation after drop"
    );
    println!(
        "{}",
        serde_json::json!({
            "enum_token_bytes": enum_bytes,
            "occurrences": OCCURRENCES,
            "unbounded_fact_bytes": enum_bytes * OCCURRENCES,
            "allowance_bytes": ALLOWANCE,
            "peak_requested_heap_bytes": peak,
            "retained_requested_heap_bytes": retained,
            "complete_enum_lists": populated,
            "omitted_enum_lists": OCCURRENCES - populated,
        })
    );
}

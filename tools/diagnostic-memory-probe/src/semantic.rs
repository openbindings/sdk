//! Allocation regressions for supported text and omitted oversized origins.
//! Inputs and prepared programs precede the measured baseline.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;
pub(super) fn run() {
    for size in [100_000, 1_000_000] {
        for mode in ["pattern", "origin", "origin-forced"] {
            let force = json!({"not":{"allOf":[false,{"$id":"https://force.test/","$ref":"%"}]}});
            let (document, value) = if mode == "pattern" {
                (
                    json!({"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"x":{"pattern":"a"},"y":force}}}}}),
                    JsonValue::parse(json!({"x":"a".repeat(size),"y":null}).to_string()).unwrap(),
                )
            } else {
                let entry = if mode == "origin-forced" {
                    json!({"allOf":[force,{"$ref":"https://u.test/"}]})
                } else {
                    json!({"$ref":"https://u.test/"})
                };
                (
                    json!({"openbindings":"0.2.0","schemas":{"k".repeat(size):{"$id":"https://u.test/","pattern":"\\a"}},"operations":{"op":{"input":entry}}}),
                    JsonValue::string("x").unwrap(),
                )
            };
            let proof = Document::parse(document.to_string())
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
                        program_bytes: 64 * 1024,
                        ..Limits::default()
                    }),
                    &ExternalSchemas::default(),
                    &PreparationLimits::default(),
                )
                .unwrap();
            let (result, added_peak, nanos) = crate::measured(|| prepared.evaluate(&value));
            println!(
                "{}",
                json!({"phase":"semantic-evaluation", "mode":mode, "source_or_instance_bytes":size,
                "program_bytes":64*1024, "peak_bytes":added_peak, "nanos":nanos})
            );
            assert!(
                added_peak < 128 * 1024,
                "{mode}/{size}: {added_peak} temporary bytes"
            );
            if mode == "pattern" {
                assert!(matches!(result, ValueOutcome::Satisfies));
            } else {
                assert!(
                    matches!(result, ValueOutcome::Undefined { detail } if detail.location.is_none())
                );
            }
        }
    }
}

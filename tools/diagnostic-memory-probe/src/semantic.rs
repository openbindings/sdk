//! Allocation regressions for supported text and omitted oversized origins.
//! Inputs and prepared programs precede the measured baseline.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::json;
pub(super) fn run() {
    for size in [100_000, 1_000_000] {
        for mode in ["pattern", "pattern-limited", "origin", "origin-forced"] {
            let evaluation_sdk_reservation_bytes = if mode == "pattern" {
                8 * 1024 * 1024
            } else {
                64 * 1024
            };
            let force = json!({"not":{"allOf":[false,{"$id":"https://force.test/","$ref":"%"}]}});
            let (document, value) = if mode.starts_with("pattern") {
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
                        evaluation_sdk_reservation_bytes,
                        ..Limits::default()
                    }),
                    &ExternalSchemas::default(),
                )
                .unwrap();
            let (result, added_peak, nanos) = crate::measured(|| prepared.evaluate(&value));
            println!(
                "{}",
                json!({"phase":"semantic-evaluation", "mode":mode, "source_or_instance_bytes":size,
                "evaluation_sdk_reservation_bytes":evaluation_sdk_reservation_bytes, "peak_bytes":added_peak, "nanos":nanos, "outcome": result})
            );
            assert!(
                added_peak < 128 * 1024,
                "{mode}/{size}: {added_peak} temporary bytes"
            );
            // Prepared matching borrows text, so both unchanged workloads now
            // fit even the 64KiB scratch policy. Preserve the physical peak gate.
            if mode.starts_with("pattern") {
                assert!(matches!(result, ValueOutcome::Satisfies));
            } else {
                assert!(
                    matches!(result, ValueOutcome::Undefined { detail } if detail.location.is_none())
                );
            }
        }
    }
}

/// Requested allocation bounds for original recursive graphs, with inputs and
/// prepared owners outside the measurement. Both cold and warm calls retain no
/// refinement attempt in the prepared owner. Realloc overlap remains counted.
pub(super) fn recursive() {
    use std::sync::atomic::Ordering::Relaxed;
    for (mode, width, required, stack_bytes) in [
        ("dense", 4, "U", 256 * 1024),
        ("dense", 8, "U", 256 * 1024),
        ("annotations", 32, "S", 256 * 1024),
        ("annotations", 512, "limit", 256 * 1024),
        ("annotations", 512, "limit", 4 * 1024 * 1024),
    ] {
        let (document, instance) = if mode == "dense" {
            let mut schemas = serde_json::Map::new();
            for node in 0..width {
                schemas.insert(format!("A{node}"), json!({"anyOf":(0..width).map(|next| json!({"$ref":format!("#/schemas/A{next}")})).collect::<Vec<_>>() }));
            }
            (
                json!({"openbindings":"0.2.0","schemas":schemas,"operations":{"op":{"input":{"$ref":"#/schemas/A0"}}}}),
                json!(null),
            )
        } else {
            let properties: serde_json::Map<_, _> =
                (0..width).map(|i| (format!("p{i}"), json!(true))).collect();
            let value: serde_json::Map<_, _> =
                (0..width).map(|i| (format!("p{i}"), json!(i))).collect();
            (
                json!({"openbindings":"0.2.0","schemas":{"A":{"anyOf":[true,{"allOf":[{"$ref":"#/schemas/A"},{"properties":properties}]}]}},"operations":{"op":{"input":{"$ref":"#/schemas/A","unevaluatedProperties":false}}}}),
                json!(value),
            )
        };
        let source = Document::parse(document.to_string())
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let schema = source.operation("op").unwrap().input().unwrap();
        let value = JsonValue::parse(instance.to_string()).unwrap();
        for budget in [64 * 1024, 8 * 1024 * 1024] {
            let prepared = schema
                .prepare(
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_reservation_bytes: budget,
                        ..Limits::default()
                    }),
                    &ExternalSchemas::default(),
                )
                .unwrap();
            for warm in [false, true] {
                let live = crate::LIVE.load(Relaxed);
                let (result, peak, nanos) = crate::measured(|| prepared.evaluate(&value));
                let actual = match &result {
                    ValueOutcome::Satisfies => "S",
                    ValueOutcome::Undefined { .. } => "U",
                    ValueOutcome::NoVerdict { detail }
                        if detail.reason == NoVerdictReason::LimitExceeded =>
                    {
                        "limit"
                    }
                    other => panic!("{mode}/{width}/{budget}: {other:?}"),
                };
                if budget == 8 * 1024 * 1024 {
                    assert_eq!(actual, required);
                } else {
                    assert!(actual == required || actual == "limit");
                }
                drop(result);
                let retained = crate::LIVE.load(Relaxed) as i128 - live as i128;
                println!(
                    "{}",
                    json!({"phase":"recursive-proof", "mode":mode,"width":width,"evaluation_sdk_reservation_bytes":budget,"historical_stack_bytes":stack_bytes,"warm":warm,"outcome":actual,"peak_bytes":peak,"retained_bytes":retained,"nanos":nanos})
                );
                assert!(
                    peak <= budget * 2,
                    "{mode}/{width}: {peak} temporary bytes with {budget} proof budget"
                );
                if warm {
                    assert_eq!(
                        retained, 0,
                        "a completed attempt must not remain in the prepared owner"
                    );
                }
            }
        }
    }
}

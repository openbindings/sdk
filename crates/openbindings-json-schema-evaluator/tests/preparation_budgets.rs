//! Preparation allowances cover original inventory and compiler/fallback work.
//! They are distinct from the evaluator's later per-value work policy.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::atomic::AtomicBool;

fn selected(body: &str) -> Schema {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{body}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
    .schema("entry")
    .unwrap()
}

#[test]
fn neither_compiled_nor_semantic_preparation_can_ignore_an_exhausted_attempt() {
    assert_eq!(PreparationLimits::default().max_sdk_work_units, None);
    for body in [
        r#"{"type":"integer"}"#,
        r#"{"anyOf":[{"type":"integer"},{"$ref":"https://missing.test/schema"}]}"#,
    ] {
        let schema = selected(body);
        let external = ExternalSchemas::default();
        for warm in [false, true] {
            let held = warm.then(|| {
                schema
                    .environment(
                        &external,
                        &PreparationLimits::default(),
                        &AtomicBool::new(false),
                    )
                    .unwrap()
            });
            for (limits, resource) in [
                (
                    PreparationLimits {
                        max_sdk_work_units: Some(0),
                        ..PreparationLimits::default()
                    },
                    "preparation-sdk-work-units",
                ),
                (
                    PreparationLimits {
                        max_sdk_reservation_bytes: 0,
                        ..PreparationLimits::default()
                    },
                    "preparation-sdk-reservation-bytes",
                ),
            ] {
                let failure = schema
                    .prepare_with_control(
                        &DefaultEvaluator::default(),
                        &external,
                        &limits,
                        &std::sync::atomic::AtomicBool::new(false),
                    )
                    .unwrap_err();
                assert_eq!(
                    failure.kind,
                    PreparationFailureKind::Limit {
                        resource: resource.into()
                    }
                );
            }
            let prepared = schema
                .prepare(&DefaultEvaluator::default(), &external)
                .unwrap();
            assert!(matches!(
                prepared.evaluate(&JsonValue::parse("1").unwrap()),
                ValueOutcome::Satisfies
            ));
            drop(held);
        }
    }
}

#[test]
fn preparation_memory_releases_local_checks_and_refuses_large_positions() {
    let evaluator = DefaultEvaluator::default();
    let resources = ExternalSchemas::default();
    // Initialize the immutable fixed compiler before exercising a smaller
    // per-attempt allowance. Its startup storage has a separate lifetime.
    selected(r#"{"type":"string"}"#)
        .prepare(&evaluator, &resources)
        .unwrap();
    let literal = "x".repeat(4096);
    let properties: serde_json::Map<_, _> = (0..64)
        .map(|i| (format!("p{i}"), serde_json::json!({"const":literal})))
        .collect();
    let supplied = |body: serde_json::Value| {
        ExternalSchemas::new([ExternalSchema {
            uri: "https://memory.test/entry".into(),
            document: JsonValue::parse(body.to_string()).unwrap(),
        }])
        .unwrap()
    };
    let resources = supplied(serde_json::json!({"properties":properties}));
    let limits = PreparationLimits {
        max_sdk_reservation_bytes: 8 * 1024 * 1024,
        ..Default::default()
    };
    resources
        .prepare_with_control(
            "https://memory.test/entry",
            &evaluator,
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap();
    let wide = supplied(serde_json::json!({"const": literal.repeat(64)}));
    let failure = wide
        .prepare_with_control(
            "https://memory.test/entry",
            &evaluator,
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::Limit {
            resource: "preparation-sdk-reservation-bytes".into()
        }
    );
}

#[test]
fn a_cached_inventory_does_not_reset_the_remaining_compiler_work() {
    let body = format!(
        r#"{{"allOf":[{}{{"type":"integer"}}]}}"#,
        r#"{"type":"integer"},"#.repeat(200)
    );
    let schema = selected(&body);
    let external = ExternalSchemas::default();
    let held = schema
        .environment(
            &external,
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let limits = PreparationLimits {
        max_sdk_work_units: Some(50),
        ..PreparationLimits::default()
    };
    let failure = schema
        .prepare_with_control(
            &DefaultEvaluator::default(),
            &external,
            &limits,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::Limit {
            resource: "preparation-sdk-work-units".into()
        }
    );
    assert!(
        schema
            .prepare(&DefaultEvaluator::default(), &external)
            .is_ok()
    );
    drop(held);
}

#[test]
fn original_meta_matching_obeys_each_attempt_and_retries_without_poisoning() {
    let schema = selected(r#"{"$ref":"https://budget.test/consumer"}"#);
    let meta = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true},"properties":{"payload":{"items":{"pattern":"^(a|b)+$"}}}}"#;
    for payload in [
        serde_json::json!(vec!["ab"; 1001]),
        serde_json::json!(["ab".repeat(5000)]),
    ] {
        let resources = ExternalSchemas::new([
            ExternalSchema {
                uri: "https://budget.test/meta".into(),
                document: JsonValue::parse(meta).unwrap(),
            },
            ExternalSchema {
                uri: "https://budget.test/consumer".into(),
                document: JsonValue::parse(
                    serde_json::json!({"$schema":"https://budget.test/meta", "payload":payload})
                        .to_string(),
                )
                .unwrap(),
            },
        ])
        .unwrap();
        // Retain the immutable environment, so these attempts cannot pass by
        // assuming a prior environment's preparation budget applies again.
        let environment = schema
            .environment(
                &resources,
                &PreparationLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        // Warm this unchanged job before measuring its scoped SDK boundaries.
        // The old fixed 128 KiB compiler reservation is no longer the contract:
        // programs are prepared once and dependency storage has structural gates.
        schema
            .prepare(&DefaultEvaluator::default(), &resources)
            .unwrap();
        for allocation in [false, true] {
            let defaults = PreparationLimits::default();
            let resource = if allocation {
                "preparation-sdk-reservation-bytes"
            } else {
                "preparation-sdk-work-units"
            };
            let prepare_with = |allowance| {
                let mut limits = defaults;
                if allocation {
                    limits.max_sdk_reservation_bytes = allowance;
                } else {
                    limits.max_sdk_work_units = Some(allowance);
                }
                schema.prepare_with_control(
                    &DefaultEvaluator::default(),
                    &resources,
                    &limits,
                    &std::sync::atomic::AtomicBool::new(false),
                )
            };
            let (mut low, mut high) = (
                0,
                if allocation {
                    defaults.max_sdk_reservation_bytes
                } else {
                    100_000_000
                },
            );
            while low < high {
                let middle = low + (high - low) / 2;
                match prepare_with(middle) {
                    Ok(_) => high = middle,
                    Err(failure) => {
                        assert_eq!(
                            failure.kind,
                            PreparationFailureKind::Limit {
                                resource: resource.into()
                            }
                        );
                        low = middle + 1;
                    }
                }
            }
            assert!(low > 0);
            prepare_with(low).unwrap();
            let failure = prepare_with(low - 1).unwrap_err();
            assert_eq!(
                failure.kind,
                PreparationFailureKind::Limit {
                    resource: resource.into()
                }
            );
            let prepared = schema
                .prepare(&DefaultEvaluator::default(), &resources)
                .unwrap_or_else(|failure| panic!("retry failed: {:?}", failure.kind));
            assert!(matches!(
                prepared.evaluate(&JsonValue::parse("null").unwrap()),
                ValueOutcome::Satisfies
            ));
        }
        drop(environment);
    }
}

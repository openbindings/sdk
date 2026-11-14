//! Compare explicit preparation policies with projection compaction enabled and disabled.
//! Check every public preparation setting at multiple refusal/success points.
use super::*;

#[test]
fn projection_preserves_explicit_preparation_refusals() {
    let text = document(
        r##"{"type":"object","properties":{"s":{"type":"string","pattern":"^(a|b)+$"},"n":{"type":"integer","minimum":0},"array":{"type":"array","items":{"$ref":"#/operations/op/input/$defs/record"}}},"$defs":{"record":{"type":"object","properties":{"s":{"type":"string"}}}}}"##,
    );
    let resources = ExternalSchemas::default();
    let mut checks = 0;
    for boundary in [0, 1, 8, 64, 1024, 65536, 1_000_000] {
        let policies = [
            PreparationLimits {
                max_schema_nodes: boundary,
                ..Default::default()
            },
            PreparationLimits {
                max_identity_bytes: boundary,
                ..Default::default()
            },
            PreparationLimits {
                max_sdk_work_units: Some(boundary),
                ..Default::default()
            },
            PreparationLimits {
                max_sdk_reservation_bytes: boundary,
                ..Default::default()
            },
        ];
        for policy in policies {
            let selected = prepare_document(&text, &resources, &DefaultEvaluator::new(), &policy);
            let original = compiled::without_projection_compaction(|| {
                prepare_document(&text, &resources, &DefaultEvaluator::new(), &policy)
            });
            match (selected, original) {
                (Err(a), Err(b)) => assert_eq!(
                    serde_json::to_value(a).unwrap(),
                    serde_json::to_value(b).unwrap()
                ),
                (Ok(_), Ok(_)) => {}
                _ => panic!("explicit preparation policy changed admission: {policy:?}"),
            }
            checks += 1;
        }
    }
    for boundary in [0, 1, 8, 64, 1024, 65536] {
        let policies = [
            Limits {
                pattern_bytes: boundary,
                ..Default::default()
            },
            Limits {
                pattern_depth: boundary,
                ..Default::default()
            },
            Limits {
                total_pattern_bytes: boundary,
                ..Default::default()
            },
            Limits {
                total_pattern_memory: boundary,
                ..Default::default()
            },
            Limits {
                schema_resources: Some(boundary),
                ..Default::default()
            },
        ];
        for policy in policies {
            let selected = prepare_document(
                &text,
                &resources,
                &DefaultEvaluator::with_limits(policy.clone()),
                &PreparationLimits::default(),
            );
            let original = compiled::without_projection_compaction(|| {
                prepare_document(
                    &text,
                    &resources,
                    &DefaultEvaluator::with_limits(policy),
                    &PreparationLimits::default(),
                )
            });
            match (selected, original) {
                (Err(a), Err(b)) => assert_eq!(
                    serde_json::to_value(a).unwrap(),
                    serde_json::to_value(b).unwrap()
                ),
                (Ok(_), Ok(_)) => {}
                _ => panic!("explicit evaluator preparation policy changed admission"),
            }
            checks += 1;
        }
    }
    eprintln!("projection preparation-policy checks={checks}");
    assert_eq!(checks, 58);
}

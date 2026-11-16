use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn selected(input: serde_json::Value) -> Schema {
    let document = serde_json::json!({
        "openbindings": "0.2.0",
        "operations": {"op": {"input": input}},
        "schemas": {"Any": {"allOf": [true]}, "Item": {"type": "object", "required": ["sku"],
            "properties": {"sku": {"type": "string"}}}}
    });
    Document::parse(document.to_string())
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .operation("op")
        .unwrap()
        .input()
        .unwrap()
}

#[test]
fn own_assertion_failure_survives_reference_item_reservations() {
    let value = JsonValue::parse(format!("[{}0]", "0,".repeat(248_999))).unwrap();
    for items in [
        serde_json::json!({"$ref": "#/schemas/Item"}),
        serde_json::json!({"type": "object", "required": ["sku"],
            "properties": {"sku": {"type": "string"}}}),
        // No child failure can supply evidence in this variant.
        serde_json::json!({"$ref": "#/schemas/Any"}),
    ] {
        for bounded in [false, true] {
            let limits = if bounded {
                Limits {
                    evaluation_sdk_work_units: Some(100_000_000),
                    evaluation_sdk_reservation_bytes: 4 * 1024 * 1024,
                    ..Limits::default()
                }
            } else {
                Limits::default()
            };
            let schema =
                selected(serde_json::json!({"type": "array", "maxItems": 50, "items": items}));
            let prepared = schema
                .prepare(
                    &DefaultEvaluator::with_limits(limits),
                    &ExternalSchemas::default(),
                )
                .unwrap();
            assert!(matches!(
                prepared.evaluate(&value),
                ValueOutcome::Fails { .. }
            ));
            let ValueOutcome::Fails {
                problems,
                problems_complete,
                ..
            } = prepared.explain(&value)
            else {
                panic!("explanation must retain failure");
            };
            if items.get("$ref").and_then(serde_json::Value::as_str) != Some("#/schemas/Any") {
                assert!(!problems_complete);
            }
            assert!(
                problems.iter().any(|p| p.code == "maxItems"
                    && p.instance.as_str() == Some("")
                    && p.schema.pointer.as_str() == Some("/operations/op/input/maxItems")),
                "{problems:?}"
            );
        }
    }
}

#[test]
fn child_failure_is_conjoined_before_later_resource_work() {
    for input in [
        serde_json::json!({"allOf": [false, {"items": {"$ref": "#/schemas/Item"}}]}),
        serde_json::json!({"prefixItems": [false], "items": {"$ref": "#/schemas/Item"}}),
        serde_json::json!({"type": "object", "$ref": "https://missing.test/schema"}),
    ] {
        let schema = selected(input);
        let prepared = schema
            .prepare(
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_reservation_bytes: 32 * 1024,
                    evaluation_sdk_work_units: Some(100_000),
                    ..Limits::default()
                }),
                &ExternalSchemas::default(),
            )
            .unwrap();
        let value = JsonValue::parse(format!("[{}0]", "0,".repeat(4095))).unwrap();
        assert!(matches!(
            prepared.evaluate(&value),
            ValueOutcome::Fails { .. }
        ));
        assert!(matches!(
            prepared.explain(&value),
            ValueOutcome::Fails { .. }
        ));
    }
}

#[test]
fn explanation_cap_distinguishes_completed_and_remaining_children() {
    let schema = selected(serde_json::json!({"items": {"type": "string"}}));
    let prepared = schema
        .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())
        .unwrap();
    for count in [100, 101] {
        let value = JsonValue::parse(format!("[{}0]", "0,".repeat(count - 1))).unwrap();
        let ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } = prepared.explain(&value)
        else {
            panic!("expected item failures");
        };
        assert_eq!(problems.len(), 100);
        assert_eq!(problems_complete, count == 100);
    }
}

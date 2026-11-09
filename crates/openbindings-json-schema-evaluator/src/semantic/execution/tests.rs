//! These control inputs were independently fixed before implementing frames.
use super::*;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
fn original(row: &Value) -> Plan {
    let mut resources = Vec::new();
    if let Some(entries) = row["resources"].as_object() {
        resources.extend(entries.iter().map(|(uri, text)| ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse(text.as_str().unwrap()).unwrap(),
        }));
    }
    resources.push(ExternalSchema {
        uri: "https://benchmark.invalid/root".into(),
        document: JsonValue::parse(row["schema"].as_str().unwrap()).unwrap(),
    });
    let proof = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://benchmark.invalid/root"}}}}"#).unwrap().validate().unwrap().conformant().unwrap();
    let resources = ExternalSchemas::new(resources).unwrap();
    let schema = proof.operation("op").unwrap().input().unwrap();
    let preparation = PreparationLimits::default();
    let control = AtomicBool::new(false);
    let environment = schema
        .environment(&resources, &preparation, &control)
        .unwrap();
    let mut limits = Limits::default();
    if let Some(bytes) = row["evaluation_sdk_reservation_bytes"].as_u64() {
        limits.evaluation_sdk_reservation_bytes = bytes as usize;
    }
    openbindings_internal_schema::preparation::Attempt::run(
        preparation.max_sdk_work_units,
        preparation.max_sdk_reservation_bytes,
        &control,
        |attempt| Plan::new(environment, limits, preparation, attempt),
    )
    .unwrap()
    .unwrap()
}
fn observed(outcome: ValueOutcome) -> String {
    match outcome {
        ValueOutcome::Satisfies => "S".into(),
        ValueOutcome::Fails { .. } => "F".into(),
        ValueOutcome::Undefined { .. } => "U".into(),
        ValueOutcome::NoVerdict { detail } => format!("N:{:?}", detail.reason),
    }
}
fn rows() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("tests/controls.json")).unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone()
}
#[test]
fn independently_fixed_applicator_scope_and_depth_controls() {
    let control = AtomicBool::new(false);
    for row in rows() {
        let plan = original(&row);
        if row["required_pattern_state"] == "limited" {
            assert!(
                plan.patterns.values().any(|program| matches!(
                    program.as_ref(),
                    patterns::Program::Limited("pattern-size-limit")
                )),
                "{} must actually prepare a local limitation",
                row["id"]
            );
        }
        let value = JsonValue::parse(row["value"].as_str().unwrap()).unwrap();
        for detail in [Detail::Outcome, Detail::Explain] {
            let outcome = plan.evaluate(&value, detail, &control);
            assert_eq!(
                observed(outcome),
                row["required"].as_str().unwrap(),
                "{}",
                row["id"]
            );
        }
    }
}
struct Stop {
    calls: AtomicUsize,
    at: usize,
}
impl WorkControl for Stop {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed) + 1 >= self.at
    }
}
#[test]
fn interruption_at_entry_return_and_retry_boundaries_does_not_poison_reuse() {
    for row in rows().into_iter().filter(|row| {
        matches!(
            row["id"].as_str(),
            Some("same-instance-diamond" | "negating-cycle" | "valid-branch-annotation")
        )
    }) {
        let plan = original(&row);
        let value = JsonValue::parse(row["value"].as_str().unwrap()).unwrap();
        let count = Stop {
            calls: AtomicUsize::new(0),
            at: usize::MAX,
        };
        let expected = observed(plan.evaluate(&value, Detail::Outcome, &count));
        for at in 1..=count.calls.load(Ordering::Relaxed) {
            let stop = Stop {
                calls: AtomicUsize::new(0),
                at,
            };
            assert_eq!(
                observed(plan.evaluate(&value, Detail::Outcome, &stop)),
                "N:Cancelled",
                "{} at {at}",
                row["id"]
            );
        }
        assert_eq!(
            observed(plan.evaluate(&value, Detail::Outcome, &AtomicBool::new(false))),
            expected
        );
    }
}

#[test]
fn alias_leaves_do_not_retain_per_item_frames_or_memo_entries() {
    let row = serde_json::json!({"schema":r##"{"$defs":{"item":{"type":"integer"}},"items":{"$ref":"#/$defs/item"}}"##});
    let plan = original(&row);
    let control = AtomicBool::new(false);
    for suffix in ["1", "false"] {
        let value = JsonValue::parse(format!("[{}{suffix}]", "1,".repeat(50_000))).unwrap();
        assert_eq!(
            observed(plan.evaluate(&value, Detail::Outcome, &control)),
            if suffix == "1" { "S" } else { "F" }
        );
    }
}
#[test]
fn alias_leaf_patterns_keep_equal_text_correlation_across_distinct_scopes() {
    for (operator, expected) in [("allOf", "F"), ("anyOf", "S")] {
        let row = serde_json::json!({
            "schema":format!(r#"{{"{operator}":[{{"$ref":"https://leaf.test/a"}},{{"not":{{"$ref":"https://leaf.test/b"}}}}]}}"#),
            "resources": {
                "https://leaf.test/a":r#"{"$dynamicAnchor":"node","$ref":"https://leaf.test/p"}"#,
                "https://leaf.test/b":r#"{"$dynamicAnchor":"node","$ref":"https://leaf.test/p"}"#,
                "https://leaf.test/p":r#"{"pattern":"["}"#
            }
        });
        let plan = original(&row);
        let value = JsonValue::parse(r#""equal""#).unwrap();
        assert_eq!(
            observed(plan.evaluate(&value, Detail::Outcome, &AtomicBool::new(false))),
            expected
        );
    }
}

#[test]
#[ignore = "release-only small-thread-stack qualification"]
fn original_frames_run_deep_values_on_a_small_native_stack() {
    let row = rows()
        .into_iter()
        .find(|row| row["id"] == "mixed-64-S")
        .unwrap();
    let plan = original(&row);
    let values = [("1", "S"), ("false", "F")].map(|(leaf, expected)| {
        (
            JsonValue::parse(format!("{}{leaf}{}", "[".repeat(256), "]".repeat(256))).unwrap(),
            expected,
        )
    });
    let result = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || {
            for (value, expected) in values {
                for detail in [Detail::Outcome, Detail::Explain] {
                    assert_eq!(
                        observed(plan.evaluate(&value, detail, &AtomicBool::new(false))),
                        expected
                    );
                }
            }
        })
        .unwrap()
        .join();
    assert!(result.is_ok());
}

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
    if let Some(bytes) = row["program_bytes"].as_u64() {
        limits.program_bytes = bytes as usize;
    }
    if let Some(bytes) = row["stack_bytes"].as_u64() {
        limits.stack_bytes = bytes as usize;
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
fn shared_scope_identity_is_content_based_and_preserves_ambiguity() {
    let a = Arc::new(vec![("a".into(), Some(2)), ("b".into(), None)]);
    let b = Arc::new(vec![("a".into(), Some(2)), ("b".into(), None)]);
    assert!(!Arc::ptr_eq(&a, &b));
    let key = |context| Visit {
        schema: 5,
        instance: 7,
        context,
    };
    let mut map = HashMap::new();
    map.insert(key(a), 1);
    assert_eq!(map.get(&key(b)), Some(&1));
    assert_eq!(map.get(&key(Arc::new(vec![("a".into(), Some(2))]))), None);
    assert_eq!(
        map.get(&key(Arc::new(vec![
            ("a".into(), Some(2)),
            ("b".into(), Some(3))
        ]))),
        None
    );
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
fn frame_backing_growth_has_an_exact_requested_byte_gate() {
    let control = AtomicBool::new(false);
    let mut frames: Vec<Frame<'static>> = Vec::new();
    let bytes = size_of::<Frame<'_>>() * 4;
    assert!(
        storage::reserve(
            &mut frames,
            1,
            &mut Graph::new(bytes - 1, None, 0, &control)
        )
        .is_err()
    );
    assert_eq!(frames.capacity(), 0);
    storage::reserve(&mut frames, 1, &mut Graph::new(bytes, None, 0, &control)).unwrap();
    eprintln!(
        "frame_bytes={} phase_bytes={} state_bytes={} witness_bytes={} initial_backing_bytes={bytes}",
        size_of::<Frame<'_>>(),
        size_of::<Phase>(),
        size_of::<State>(),
        size_of::<explain::WitnessSeed>(),
    );
    assert!(size_of::<State>() <= 256, "State::copied base reserve");
    assert_eq!(
        size_of::<Option<Arc<explain::WitnessSeed>>>(),
        size_of::<usize>()
    );
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

#[test]
#[ignore = "release-only stack observations, separate from allocation and timing"]
fn original_frame_stack_observations() {
    struct Observe {
        origin: usize,
        maximum: AtomicUsize,
    }
    impl WorkControl for Observe {
        #[inline(never)]
        fn is_cancelled(&self) -> bool {
            let marker = 0u8;
            let here = std::hint::black_box(std::ptr::addr_of!(marker)) as usize;
            self.maximum
                .fetch_max(self.origin.abs_diff(here), Ordering::Relaxed);
            false
        }
    }
    let row = rows()
        .into_iter()
        .find(|row| row["id"] == "mixed-64-S")
        .unwrap();
    let mut plan = original(&row);
    let value = JsonValue::parse(row["value"].as_str().unwrap()).unwrap();
    for bytes in [2048, 8192, 16_384, 32_768, 65_536, 131_072, 262_144] {
        plan.limits.stack_bytes = bytes;
        let marker = 0u8;
        let origin = std::hint::black_box(std::ptr::addr_of!(marker)) as usize;
        let observe = Observe {
            origin,
            maximum: AtomicUsize::new(0),
        };
        let outcome = plan.evaluate(&value, Detail::Outcome, &observe);
        std::hint::black_box(&marker);
        eprintln!(
            "stack_budget={bytes} sampled_bytes={} outcome={}",
            observe.maximum.load(Ordering::Relaxed),
            observed(outcome)
        );
    }
}

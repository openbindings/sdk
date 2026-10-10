use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};
use std::sync::Arc;

fn prepared(schema: Value, limits: Limits, resources: ResourceSet) -> PreparedContract {
    let doc = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let context = doc
        .value_contracts(Arc::new(DefaultEvaluator::with_limits(limits)), resources)
        .unwrap();
    match context.prepare("op", Side::Input) {
        ContractPreparation::Ready(contract) => contract,
        other => panic!("{other:?}"),
    }
}
fn run(schema: Value, value: Value, bytes: usize, count: usize) -> ValueOutcome {
    prepared(
        schema,
        Limits {
            diagnostic_bytes: bytes,
            max_problems: count,
            ..Limits::default()
        },
        ResourceSet::default(),
    )
    .validate(&JsonValue::parse(value.to_string()).unwrap())
}
fn strings(problem: &ValueProblem) -> usize {
    problem.instance_pointer.len()
        + problem.code.len()
        + problem.message.len()
        + problem.schema_location.as_ref().map_or(0, |at| {
            at.pointer.len() + at.resource.as_ref().map_or(0, String::len)
        })
}
fn check(result: &ValueOutcome, budget: usize, count: usize) -> (&[ValueProblem], bool) {
    let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = result
    else {
        panic!("expected failure: {result:?}")
    };
    let bytes: usize = problems.iter().map(strings).sum();
    assert!(bytes <= budget);
    assert!(problems.len() <= count.max(1));
    // Compact Rust JSON worst case: 59-byte envelope; <=96 bytes/problem
    // including a separator and null resource; <=6 wire bytes/string UTF-8 byte.
    assert!(serde_json::to_string(result).unwrap().len() <= 59 + 96 * problems.len() + 6 * bytes);
    (problems, *problems_complete)
}
#[test]
fn maintained_amplification_witness() {
    let key = "k".repeat(32768);
    let schema =
        json!({"type":"object","properties":{&key:{"type":"object","additionalProperties":false}}});
    let fields: serde_json::Map<_, _> = (0..257).map(|i| (format!("f{i}"), json!(0))).collect();
    let value = json!({&key:fields});
    // Raising the budget reproduces the old amplification from the maintained
    // fixture; lowering it keeps the same deterministic prefix and original locations.
    let full = run(schema.clone(), value.clone(), 32 * 1024 * 1024, 256);
    let (all, complete) = check(&full, 32 * 1024 * 1024, 256);
    assert_eq!(all.len(), 256);
    assert!(!complete);
    assert!(serde_json::to_string(&full).unwrap().len() > 16_000_000);
    for count in [1, 256] {
        let result = run(schema.clone(), value.clone(), 1_048_576, count);
        let (problems, complete) = check(&result, 1_048_576, count);
        assert!(!problems.is_empty());
        assert!(!complete);
        for (i, p) in problems.iter().enumerate() {
            assert_eq!(p.code, "falseSchema");
            assert_eq!(p.instance_pointer, all[i].instance_pointer);
            assert_eq!(p.schema_location, all[i].schema_location);
            assert!(
                JsonValue::parse(value.to_string())
                    .unwrap()
                    .at(&p.instance_pointer)
                    .is_some()
            );
        }
    }
}
#[test]
fn byte_count_boundaries_unicode_escaping_and_single_oversized_location() {
    for key in ["plain", "~/\u{0}\n\t\"\\", "é東京🦀"] {
        let schema = json!({"properties":{key:{"type":"string"}}});
        let value = json!({key:7});
        let full = run(schema.clone(), value.clone(), 1_048_576, 256);
        let (all, complete) = check(&full, 1_048_576, 256);
        assert_eq!(all.len(), 1);
        assert!(complete);
        let size = strings(&all[0]);
        for budget in [0, 1, size - 1, size, size + 1, 1_048_576] {
            let result = run(schema.clone(), value.clone(), budget, 0);
            let (problems, complete) = check(&result, budget, 0);
            assert_eq!(problems.len(), usize::from(budget >= size));
            assert_eq!(complete, budget >= size);
            if let Some(p) = problems.first() {
                assert_eq!(
                    p.instance_pointer,
                    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
                );
                assert_eq!(p.schema_location, all[0].schema_location);
            }
        }
    }
    let schema = json!({"allOf":[false,false,false]});
    let full = run(schema.clone(), json!(0), 1_048_576, 256);
    let (all, _) = check(&full, 1_048_576, 256);
    let size = strings(&all[0]);
    for budget in [size - 1, size, size + 1, 2 * size - 1, 2 * size, 3 * size] {
        for count in [0, 1, 2, 3, 4] {
            let result = run(schema.clone(), json!(0), budget, count);
            let (problems, complete) = check(&result, budget, count);
            let expected = (budget / size).min(count.max(1)).min(3);
            assert_eq!(problems.len(), expected);
            assert_eq!(complete, expected == 3);
        }
    }
}
#[test]
fn external_uri_oversize_and_verdict_cancellation_reuse() {
    let uri = format!("https://example.test/{}", "u".repeat(32768));
    let resources = ResourceSet::new([SchemaResource {
        uri: uri.clone(),
        document: JsonValue::parse(r#"{"type":"string"}"#).unwrap(),
    }])
    .unwrap();
    let contract = prepared(
        json!({"$ref":uri}),
        Limits {
            diagnostic_bytes: 512,
            ..Limits::default()
        },
        resources,
    );
    for _ in 0..3 {
        let result = contract.validate(&JsonValue::parse("3").unwrap());
        let (problems, complete) = check(&result, 512, 256);
        assert!(problems.is_empty());
        assert!(!complete);
        assert!(matches!(
            contract.validate(&JsonValue::parse(r#""ok""#).unwrap()),
            ValueOutcome::Satisfies
        ));
    }
    for budget in [0, 1, 1_048_576] {
        let contract = prepared(
            json!(false),
            Limits {
                diagnostic_bytes: budget,
                ..Limits::default()
            },
            ResourceSet::default(),
        );
        let control = WorkControl::new();
        control.cancel();
        assert!(matches!(
            contract.validate_with_control(&JsonValue::parse("0").unwrap(), &control),
            ValueOutcome::NoVerdict {
                detail: NoVerdict {
                    reason: NoVerdictReason::Cancelled,
                    ..
                }
            }
        ));
        check(
            &contract.validate(&JsonValue::parse("0").unwrap()),
            budget,
            256,
        );
        assert!(matches!(
            run(json!(true), json!(0), budget, 1),
            ValueOutcome::Satisfies
        ));
        let result = prepared(
            json!(false),
            Limits {
                diagnostic_bytes: budget,
                evaluation_steps: 0,
                ..Limits::default()
            },
            ResourceSet::default(),
        )
        .validate(&JsonValue::parse("0").unwrap());
        assert!(matches!(result, ValueOutcome::NoVerdict { .. }));
    }
}
#[test]
fn deterministic_mutations_seed_0x5eed_cafe() {
    let mut seed = 0x5eed_cafeu32;
    for _ in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let key = format!(
            "{}{}",
            ["~", "/", "\n", "é", "🦀"][seed as usize % 5].repeat(seed as usize % 31 + 1),
            seed
        );
        let schema = json!({"properties":{&key:{"allOf":[{"type":"string"},false]}}});
        let value = json!({&key:7});
        let full = run(schema.clone(), value.clone(), 1_048_576, 256);
        let (all, _) = check(&full, 1_048_576, 256);
        let budgets = [
            0,
            1,
            strings(&all[0]) - 1,
            strings(&all[0]),
            all.iter().map(strings).sum(),
        ];
        for budget in budgets {
            let one = run(schema.clone(), value.clone(), budget, 256);
            let two = run(schema.clone(), value.clone(), budget, 256);
            let (problems, _) = check(&one, budget, 256);
            assert_eq!(
                serde_json::to_string(&one).unwrap(),
                serde_json::to_string(&two).unwrap()
            );
            for (p, expected) in problems.iter().zip(all) {
                assert_eq!(p.instance_pointer, expected.instance_pointer);
                assert_eq!(p.schema_location, expected.schema_location);
                assert_eq!(p.code, expected.code);
            }
        }
    }
}
#[test]
fn vendor_rejects_paths_before_copying_and_bounds_collections() {
    let key = "~/🦀".repeat(8192);
    let validator =
        jsonschema::validator_for(&json!({"properties":{&key:{"type":"string"}}})).unwrap();
    let value = json!({&key:1});
    let (errors, usage) = jsonschema::ob_work::diagnostic_metadata(128, 8, || {
        validator.iter_errors(&value).collect::<Vec<_>>()
    });
    assert!(errors.is_empty());
    assert_eq!(usage.copied_bytes, 0);
    assert!(usage.rejected_copies > 0);
    let validator = jsonschema::validator_for(
        &json!({"properties":{"known":true},"additionalProperties":false}),
    )
    .unwrap();
    let value = Value::Object(
        (0..10_000)
            .map(|i| (format!("f{i:05}"), json!(null)))
            .collect(),
    );
    let (errors, usage) = jsonschema::ob_work::diagnostic_metadata(128, 8, || {
        validator.iter_errors(&value).collect::<Vec<_>>()
    });
    assert!(usage.copied_bytes <= 128);
    assert_eq!(usage.collection_items, 8);
    assert!(usage.collection_truncated);
    assert_eq!(errors.len(), 1);
    let validator = jsonschema::validator_for(
        &json!({"anyOf":[{"properties":{&key:false}},{"type":"string"}]}),
    )
    .unwrap();
    let (_, usage) = jsonschema::ob_work::diagnostic_metadata(128, 8, || {
        validator.iter_errors(&json!({&key:1})).count()
    });
    assert_eq!(usage.copied_bytes, 0);
    assert_eq!(usage.rejected_copies, 0); // no discarded branch path copies
}

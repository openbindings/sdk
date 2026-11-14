//! Independent generated cases for private projection compaction. The oracle
//! uses original locations and is prepared without constructing a compiled lane.
use super::*;

#[test]
fn projection_compaction_agrees_on_twelve_thousand_generated_pairs() {
    let (mut pairs, mut compiled, mut passes, mut failures) = (0, 0, 0, 0);
    for seed in 0..2_000u64 {
        let low = (seed.wrapping_mul(6364136223846793005).rotate_left(17) % 1000) as i64;
        let width = (seed % 7 + 1) as i64;
        let key = format!("field{seed}");
        let leaf = json!({"type":"integer","minimum":low,"maximum":low+width});
        let mut properties = serde_json::Map::new();
        properties.insert(key.clone(), leaf.clone());
        properties.insert(
            "name".into(),
            json!({"type":"string","pattern":"^[A-Za-z]+$"}),
        );
        properties.insert(
            "items".into(),
            json!({"type":"array","items":leaf,"maxItems":4}),
        );
        let record = json!({"type":"object","properties":properties,"required":[key,"name"],"additionalProperties":false});
        let reference = if seed % 2 == 0 {
            "#/operations/op/input/$defs/record"
        } else {
            "#/$defs/record"
        };
        let schema = match seed % 5 {
            0 => record.clone(),
            1 => json!({"$defs":{"record":record},"$ref":reference}),
            2 => json!({"allOf":[record,{"properties":{"name":{"maxLength":8}}}]}),
            3 => json!({"anyOf":[record,{"type":"null"}]}),
            _ => {
                json!({"$defs":{"record":record},"type":"object","properties":{"left":{"$ref":reference},"right":{"$ref":reference}},"required":["left","right"],"additionalProperties":false})
            }
        };
        let resources = if seed % 2 == 0 {
            ExternalSchemas::default()
        } else {
            ExternalSchemas::new([ExternalSchema {
                uri: "https://projection.test/record".into(),
                document: JsonValue::parse(schema.to_string()).unwrap(),
            }])
            .unwrap()
        };
        let text = if seed % 2 == 0 {
            document(&schema.to_string())
        } else {
            document(r#"{"$ref":"https://projection.test/record"}"#)
        };
        let selected = prepare_document(
            &text,
            &resources,
            &DefaultEvaluator::new(),
            &PreparationLimits::default(),
        )
        .unwrap();
        let original = prepare_document(
            &text,
            &resources,
            &Original(Limits::default()),
            &PreparationLimits::default(),
        )
        .unwrap();
        assert!(
            matches!(selected.compiled, compiled::Lane::Compiled(_)),
            "seed {seed} must exercise the compiled route"
        );
        let valid = json!({key.clone():low,"name":"Record","items":[low,low+width]});
        let mut values = vec![
            valid.clone(),
            json!({key.clone():low-1,"name":"Record"}),
            json!({key.clone():low+width+1,"name":"Record"}),
            json!({key.clone():low,"name":"123"}),
            json!({"name":"Record"}),
            json!({key.clone():low,"name":"Record","extra":true}),
        ];
        if seed % 5 == 4 {
            values = values
                .into_iter()
                .map(|value| json!({"left":value,"right":valid}))
                .collect();
        }
        for value in values {
            let value = JsonValue::parse(value.to_string()).unwrap();
            compiled::entries::take();
            let actual = selected.evaluate(&value, Detail::Outcome, &HEALTHY);
            compiled += compiled::entries::take()[2];
            let expected = original.evaluate(&value, Detail::Outcome, &HEALTHY);
            match (&actual, &expected) {
                (ValueOutcome::Satisfies, ValueOutcome::Satisfies) => passes += 1,
                (ValueOutcome::Fails { .. }, ValueOutcome::Fails { .. }) => failures += 1,
                _ => panic!("verdict mismatch/refusal at seed {seed}: {actual:?} / {expected:?}"),
            }
            pairs += 1;
        }
    }
    eprintln!(
        "projection generated schemas=2000 pairs={pairs} compiled={compiled} passes={passes} failures={failures}"
    );
    assert_eq!(pairs, 12_000);
    assert_eq!(compiled, pairs);
    assert!(passes >= 2_000 && failures >= 8_000);
}

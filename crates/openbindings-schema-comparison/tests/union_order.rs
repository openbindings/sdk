use openbindings::JsonValue;
use openbindings_schema_comparison::{ComparisonLimits, Direction, SchemaInput, Verdict, compare};
use serde_json::{Value, json};
#[allow(dead_code)]
#[path = "support/property_generator.rs"]
mod generator;
fn schema(value: &Value) -> SchemaInput {
    let value = JsonValue::parse(value.to_string()).unwrap();
    SchemaInput::new(value.clone(), value)
}
fn verdict(
    direction: Direction,
    a: &Value,
    b: &Value,
) -> Result<Verdict, openbindings_schema_comparison::ComparisonFailure> {
    compare(
        direction,
        &schema(a),
        &schema(b),
        &ComparisonLimits::default(),
    )
    .map(|r| r.verdict())
}
#[test]
fn minimized_definite_failure_dominates_uncertainty_and_keeps_authored_index() {
    for key in ["anyOf", "oneOf"] {
        for direction in [Direction::Input, Direction::Output] {
            for reverse in [false, true] {
                let variants = if reverse {
                    json!([false, {}])
                } else {
                    json!([{}, false])
                };
                let flow = json!({key:variants});
                let choices = json!({"oneOf":[{"type":"string"}]});
                let (a, b) = if direction == Direction::Input {
                    (&flow, &choices)
                } else {
                    (&choices, &flow)
                };
                let result = compare(
                    direction,
                    &schema(a),
                    &schema(b),
                    &ComparisonLimits::default(),
                )
                .unwrap();
                assert_eq!(result.verdict(), Verdict::Incompatible);
                let index = usize::from(reverse);
                let side = if direction == Direction::Input {
                    "target"
                } else {
                    "candidate"
                };
                let other = if direction == Direction::Input {
                    "candidate"
                } else {
                    "target"
                };
                assert_eq!(
                    result.reason(),
                    Some(
                        format!("{key}: {side} variant {index} has no compatible {other} variant")
                            .as_str()
                    )
                );
            }
        }
    }
    assert_eq!(
        verdict(
            Direction::Input,
            &json!({"anyOf":[false]}),
            &json!({"anyOf":[{"type":"string"}]})
        ),
        Ok(Verdict::Indeterminate)
    );
    assert_eq!(
        verdict(
            Direction::Input,
            &json!({"anyOf":[{}]}),
            &json!({"anyOf":[{}]})
        ),
        Ok(Verdict::Compatible)
    );
}
#[test]
fn fixed_seed_union_permutations_preserve_every_directional_verdict() {
    let seed = 82143744275_u64;
    let mut rng = generator::Rng(seed);
    for case in 0..5_000 {
        let a = generator::schema(&mut rng, 2);
        let b = generator::schema(&mut rng, 1);
        for key in ["anyOf", "oneOf"] {
            let p = generator::permute(&a, key);
            for direction in [Direction::Input, Direction::Output] {
                assert_eq!(
                    verdict(direction, &a, &b),
                    verdict(direction, &p, &b),
                    "seed={seed} case={case} key={key} target"
                );
                assert_eq!(
                    verdict(direction, &b, &a),
                    verdict(direction, &b, &p),
                    "seed={seed} case={case} key={key} candidate"
                );
            }
        }
    }
}

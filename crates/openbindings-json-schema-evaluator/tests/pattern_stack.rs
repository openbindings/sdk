//! Patterns nested as deeply as the default limits admit prepare on a small
//! stack. The only test in this executable, so nothing else shares its thread.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(schema: &str) -> Result<PreparedContract, NoVerdict> {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation.input().unwrap().prepare(&DefaultEvaluator::new())
}

#[test]
fn patterns_at_the_default_depth_prepare_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = Limits::default().pattern_depth;
            // (opening, closing, whether a value is checked against it)
            for (open, close, checked) in [
                ("(", ")", true),
                ("(?:", ")*", true),
                ("(?:a|", ")", true),
                ("(?:[a-z]", ")+", true),
                ("(?=", ")", false),
                ("(?!", ")", false),
                ("(?<=", ")", false),
            ] {
                let pattern = format!("{}a{}", open.repeat(depth), close.repeat(depth));
                let quoted = serde_json::to_string(&pattern).unwrap();
                for schema in [
                    format!(r#"{{"pattern":{quoted}}}"#),
                    format!(r#"{{"patternProperties":{{{quoted}:true}}}}"#),
                ] {
                    let contract = prepared(&schema).unwrap_or_else(|d| panic!("{pattern}: {d:?}"));
                    if checked {
                        let value = JsonValue::parse(r#""a""#).unwrap();
                        assert!(
                            !matches!(contract.validate(&value), ValueOutcome::NoVerdict { .. }),
                            "{pattern}"
                        );
                    }
                }
                // One level deeper is refused before any regular expression is built.
                let deeper = format!("{}a{}", open.repeat(depth + 1), close.repeat(depth + 1));
                let schema = format!(
                    r#"{{"pattern":{}}}"#,
                    serde_json::to_string(&deeper).unwrap()
                );
                assert!(matches!(
                    prepared(&schema),
                    Err(detail) if detail.code == "pattern-depth-limit"
                ));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

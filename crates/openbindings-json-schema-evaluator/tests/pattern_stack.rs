//! Patterns nested as deeply as the default limits admit prepare, and values
//! are checked against them, on a small stack. The only test in this
//! executable, so nothing else shares its thread.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(schema: &str) -> Result<PreparedValueContract, NoVerdict> {
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

// Release builds need at most 96 KiB here (macOS arm64). Unoptimized code gives
// the regular-expression compiler frames large enough that 32 nested quantified
// groups need up to 1 MiB, so tools/verify.py runs this test in release builds.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "unoptimized pattern compilation exceeds 256 KiB at the default depth; verified in release"
)]
fn patterns_at_the_default_depth_prepare_and_check_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = Limits::default().pattern_depth;
            // (opening, closing, whether the evaluator matches it)
            for (open, close, matched) in [
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
                // Each keyword is checked against a value it applies to. A
                // lookaround needs a backtracking engine: a string that reaches
                // the pattern gets no verdict, and a `patternProperties` name
                // holding one refuses preparation.
                for (keyword, schema, value) in [
                    ("pattern", format!(r#"{{"pattern":{quoted}}}"#), r#""a""#),
                    (
                        "patternProperties",
                        format!(r#"{{"patternProperties":{{{quoted}:false}}}}"#),
                        r#"{"a":true}"#,
                    ),
                ] {
                    let value = JsonValue::parse(value).unwrap();
                    match prepared(&schema) {
                        Ok(contract) => {
                            let outcome = contract.validate(&value);
                            if matched {
                                assert!(!matches!(outcome, ValueOutcome::NoVerdict { .. }), "{pattern}");
                            } else {
                                assert!(
                                    matches!(&outcome, ValueOutcome::NoVerdict { detail } if detail.code == "pattern-backtracking"),
                                    "{pattern}: {outcome:?}"
                                );
                            }
                        }
                        Err(detail) => {
                            assert!(!matched && keyword == "patternProperties", "{pattern}: {detail:?}");
                            assert_eq!(detail.code, "pattern-backtracking");
                        }
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

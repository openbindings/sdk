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

// Unoptimized Windows code gives the regular-expression matcher frames that
// alone exceed this stack in the meta-schema check every preparation runs,
// whatever the pattern. tools/verify.py also runs this test in release builds.
#[test]
#[cfg_attr(
    all(windows, debug_assertions),
    ignore = "Windows debug preparation exceeds 256 KiB at any pattern depth; verified in release"
)]
fn patterns_at_the_default_depth_prepare_and_check_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = Limits::default().pattern_depth;
            // Unoptimized matching takes a frame of about 96 KiB per nested
            // lookaround, so only optimized builds check values against them.
            let lookarounds = !cfg!(debug_assertions);
            // (opening, closing, whether a value is checked against it)
            for (open, close, checked) in [
                ("(", ")", true),
                ("(?:", ")*", true),
                ("(?:a|", ")", true),
                ("(?:[a-z]", ")+", true),
                ("(?=", ")", lookarounds),
                ("(?!", ")", lookarounds),
                ("(?<=", ")", lookarounds),
            ] {
                let pattern = format!("{}a{}", open.repeat(depth), close.repeat(depth));
                let quoted = serde_json::to_string(&pattern).unwrap();
                // Each keyword is checked against a value it applies to.
                for (schema, value) in [
                    (format!(r#"{{"pattern":{quoted}}}"#), r#""a""#),
                    (
                        format!(r#"{{"patternProperties":{{{quoted}:false}}}}"#),
                        r#"{"a":true}"#,
                    ),
                ] {
                    let contract = prepared(&schema).unwrap_or_else(|d| panic!("{pattern}: {d:?}"));
                    if checked {
                        let value = JsonValue::parse(value).unwrap();
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

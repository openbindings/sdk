//! Patterns nested as deeply as the default limits admit prepare, and values
//! are checked against them, on a small stack. The only test in this
//! executable, so nothing else shares its thread.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

fn prepared(
    schema: &str,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let proof = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = proof.resolve_operation("op").unwrap();
    operation.input().unwrap().prepare(
        &DefaultEvaluator::new(),
        &openbindings::ExternalSchemas::default(),
        &openbindings::PreparationLimits::default(),
    )
}

// This qualifies the concrete release build on a 256 KiB thread. It does not
// promise a universal physical-stack bound. tools/verify.py runs the release
// build because dependency parser frames are larger without optimization.
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
                // Both keywords retain local capability limits until a value
                // actually demands the unsupported match.
                for (keyword, schema, value) in [
                    ("pattern", format!(r#"{{"pattern":{quoted}}}"#), r#""a""#),
                    (
                        "patternProperties",
                        format!(r#"{{"patternProperties":{{{quoted}:false}}}}"#),
                        r#"{"a":true}"#,
                    ),
                ] {
                    let value = JsonValue::parse(value).unwrap();
                    let contract = prepared(&schema).unwrap();
                    for outcome in [contract.evaluate(&value), contract.explain(&value)] {
                        let accepts_a = open != "(?:[a-z]";
                        if matched && ((keyword == "pattern") == accepts_a) {
                            assert!(
                                matches!(outcome, ValueOutcome::Satisfies),
                                "{pattern}: {outcome:?}"
                            );
                        } else if matched {
                            assert!(
                                matches!(outcome, ValueOutcome::Fails { .. }),
                                "{pattern}: {outcome:?}"
                            );
                        } else {
                            assert!(
                                matches!(&outcome, ValueOutcome::NoVerdict { detail }
                                    if detail.reason == NoVerdictReason::UnsupportedCapability
                                    && detail.code == "pattern-backtracking"),
                                "{pattern}: {outcome:?}"
                            );
                        }
                    }
                }
                // One deeper is retained as a local limit before construction;
                // values outside the string domain still have a definite result.
                let deeper = format!("{}a{}", open.repeat(depth + 1), close.repeat(depth + 1));
                let schema = format!(
                    r#"{{"pattern":{}}}"#,
                    serde_json::to_string(&deeper).unwrap()
                );
                let contract = prepared(&schema).unwrap();
                assert!(matches!(
                    contract.evaluate_json("null").unwrap(),
                    ValueOutcome::Satisfies
                ));
                assert!(matches!(
                    contract.evaluate_json(r#""a""#).unwrap(),
                    ValueOutcome::NoVerdict { detail }
                        if detail.reason == NoVerdictReason::LimitExceeded
                        && detail.code == "pattern-depth-limit"
                ));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

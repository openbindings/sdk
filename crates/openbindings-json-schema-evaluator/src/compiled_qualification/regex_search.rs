use super::*;
use regex_automata::nfa::thompson::{self, WhichCaptures};

fn nfa(form: &str) -> thompson::NFA {
    // regex 1.13.1 builders.rs build_one_string and meta::Config's
    // to_thompson_config: forward, UTF-8, all captures, no shrinking.
    thompson::Compiler::new()
        .configure(
            thompson::Config::new()
                .utf8(true)
                .reverse(false)
                .shrink(false)
                .which_captures(WhichCaptures::All)
                .nfa_size_limit(Some(32 * 1024 * 1024)),
        )
        .syntax(regex_automata::util::syntax::Config::new().utf8(true))
        .build(form)
        .unwrap()
}
fn form(pattern: &str) -> String {
    let crate::pattern::Reading::Linear(reading) =
        crate::pattern::read(pattern, 32, 1024 * 1024).unwrap()
    else {
        panic!("linear")
    };
    reading.form
}
// Literal Unicode endpoints avoid counted repetitions and property-escape braces.
const ATOM: &str = "[\u{81}-\u{80001}]";
const CAPACITY: usize = 2_097_152;

#[test]
fn dense_customer_patterns_keep_empty_search_below_backtracker_capacity() {
    for (copies, nullable, expected_states) in [(16_382, false, 180_332), (16_379, true, 180_300)] {
        let pattern = format!("{}{}", ATOM.repeat(copies), if nullable { "|" } else { "" });
        let normalized = form(&pattern);
        let built = nfa(&normalized);
        assert_eq!(built.states().len(), expected_states);
        assert!(built.states().len() < CAPACITY);
        // Force the affected search engine too, independent of meta optimizations.
        let backtracker = thompson::backtrack::BoundedBacktracker::new_from_nfa(built).unwrap();
        assert_eq!(
            backtracker
                .try_is_match(&mut backtracker.create_cache(), "")
                .unwrap(),
            nullable
        );
        for property_name in [false, true] {
            let schema = |pattern: &str| {
                if property_name {
                    json!({"patternProperties":{pattern:false}})
                } else {
                    json!({"pattern":pattern})
                }
            };
            compiled::entries::take();
            let validator = prepare_document(
                &document(&schema(&pattern).to_string()),
                &ExternalSchemas::default(),
                &DefaultEvaluator::new(),
                &PreparationLimits::default(),
            )
            .unwrap();
            let construction = compiled::entries::take();
            assert!(!matches!(validator.compiled, compiled::Lane::Original(_)));
            assert_eq!(construction[0], 1, "one compiled selection");
            assert!(construction[1] > 0, "Regex compilation was entered");
            let value =
                JsonValue::parse(if property_name { r#"{"":0}"# } else { r#""""# }).unwrap();
            let outcome = validator.evaluate(&value, Detail::Outcome, &HEALTHY);
            let satisfies = if property_name { !nullable } else { nullable };
            assert_eq!(
                serialized(outcome)["status"],
                if satisfies { "satisfies" } else { "fails" }
            );
            assert_eq!(
                compiled::entries::take(),
                [0, 0, 1],
                "compiled value evaluation"
            );
            // A profile change admitting another class needs the argument rechecked.
            let larger = format!(
                "{}{}",
                ATOM.repeat(copies + 1),
                if nullable { "|" } else { "" }
            );
            let refused = prepare_document(
                &document(&schema(&larger).to_string()),
                &ExternalSchemas::default(),
                &DefaultEvaluator::new(),
                &PreparationLimits::default(),
            )
            .unwrap();
            assert!(matches!(refused.compiled, compiled::Lane::Original(_)));
            assert_eq!(compiled::entries::take(), [0, 0, 0]);
        }
    }
}

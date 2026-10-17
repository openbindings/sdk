use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
use serde_json::json;
#[test]
fn cheap_property_name_failures_do_not_request_diagnostics() {
    let key = "x".repeat(65_536);
    let value = JsonValue::parse(json!({key:0}).to_string()).unwrap();
    for (name, schema) in [
        ("direct", json!({"propertyNames":{"maxLength":1}})),
        (
            "anyOf",
            json!({"anyOf":[{"propertyNames":{"maxLength":1}},false]}),
        ),
        (
            "oneOf",
            json!({"oneOf":[{"propertyNames":{"maxLength":1}},false]}),
        ),
        (
            "allOf",
            json!({"allOf":[{"propertyNames":{"maxLength":1}}]}),
        ),
        ("not", json!({"not":{"propertyNames":{"maxLength":1}}})),
        (
            "if",
            json!({"if":{"propertyNames":{"maxLength":1}},"then":false,"else":false}),
        ),
    ] {
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .unwrap();
        assert_eq!(validator.is_valid(backend::view(&value)), name == "not");
        let document = Document::parse(
            serde_json::json!({
                "openbindings":"0.2.0", "operations":{"op":{"input":schema}}
            })
            .to_string(),
        )
        .unwrap();
        let document = document
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant document");
        let operation = document
            .resolve_operation("op")
            .expect("declared operation");
        let evaluator = openbindings_json_schema_evaluator::DefaultEvaluator::with_limits(
            openbindings_json_schema_evaluator::Limits {
                ..Default::default()
            },
        );
        let Ok(contract) = operation
            .input()
            .expect("declared input")
            .prepare(&evaluator)
        else {
            panic!()
        };
        let outcome = contract.validate(&value);
        if name == "not" {
            assert!(matches!(outcome, ValueOutcome::Satisfies));
        } else {
            assert!(matches!(
                outcome,
                ValueOutcome::Fails {
                    problems_complete: false,
                    ..
                }
            ));
        }
    }
}

use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
use serde_json::json;
#[test]
fn property_name_scratch_is_not_admitted_inside_applicator_validity_recheck() {
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
        let (result, usage) = jsonschema::ob_work::diagnostic_metadata(128, 8, || {
            jsonschema::ob_work::diagnostics(8, || {
                validator.iter_errors(backend::view(&value)).count()
            })
        });
        let count = result.unwrap_or(0);
        println!(
            "{name}: errors={count} copied_bytes={} rejected_copies={}",
            usage.copied_bytes, usage.rejected_copies
        );
        assert_eq!(usage.copied_bytes, 0);
        assert!(usage.rejected_copies > 0);
        assert_eq!(count, 0);
        // The public evaluator preserves its established verdict while marking
        // scratch-refused diagnostics incomplete, including nested applicators.
        let document = ParsedDocument::parse(
            serde_json::json!({
                "openbindings":"0.2.0", "operations":{"op":{"input":schema}}
            })
            .to_string(),
        )
        .unwrap();
        let context = document
            .value_contracts(
                std::sync::Arc::new(
                    openbindings_json_schema_evaluator::DefaultEvaluator::with_limits(
                        openbindings_json_schema_evaluator::Limits {
                            diagnostic_bytes: 128,
                            ..Default::default()
                        },
                    ),
                ),
                ResourceSet::default(),
            )
            .unwrap();
        let ContractPreparation::Ready(contract) = context.prepare("op", Side::Input) else {
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

use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde_json::{Value, json};

fn prepared(
    schema: Value,
    limits: Limits,
    resources: ExternalSchemas,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    let doc = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let doc = doc
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document");
    let operation = doc.resolve_operation("op").expect("declared operation");
    let contract = operation.input().expect("declared input");
    match contract.prepare(
        &DefaultEvaluator::with_limits(limits),
        &resources,
        &openbindings::PreparationLimits::default(),
    ) {
        Ok(contract) => contract,
        Err(detail) => panic!("{detail:?}"),
    }
}
/// Prepare operation `op` input with the default evaluator and no supplied resources.
fn prepare_input(
    doc: &Document,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    doc.validate()
        .unwrap()
        .conformant()
        .unwrap()
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
}
#[test]
fn cheap_checks_are_value_free_and_explanations_include_all_failures() {
    let schema = json!({"allOf":[{"type":"string"},false]});
    let value = JsonValue::parse("42").unwrap();
    let contract = prepared(
        schema.clone(),
        Limits::default(),
        ExternalSchemas::default(),
    );
    let outcome = contract.evaluate(&value);
    let ValueOutcome::Fails {
        message,
        problems,
        problems_complete,
    } = outcome
    else {
        panic!()
    };
    assert_eq!(message, "value does not satisfy the schema");
    assert!(problems.is_empty());
    assert!(!problems_complete);
    let doc = Document::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    let Ok(contract) = prepare_input(&doc) else {
        panic!()
    };
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = contract.explain(&value)
    else {
        panic!()
    };
    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0].code, "type");
    assert_eq!(
        problems[0].schema.pointer,
        "/operations/op/input/allOf/0/type"
    );
    assert_eq!(problems[1].code, "falseSchema");
    assert_eq!(problems[1].schema.pointer, "/operations/op/input/allOf/1");
    assert!(problems_complete);
}
#[test]
fn removed_limits_and_evaluator_detail_settings_are_rejected() {
    for field in ["diagnostic_bytes", "max_problems"] {
        assert!(serde_json::from_str::<Limits>(&format!("{{\"{field}\":1}}")).is_err());
    }
    // Detail is chosen per call, never in the evaluator's configuration.
    for setting in [
        r#"{"diagnostics":"upstream"}"#,
        r#"{"diagnostics":"coarse"}"#,
    ] {
        assert!(serde_json::from_str::<Limits>(setting).is_err());
    }
}

#[test]
fn interrupted_explanation_cannot_weaken_an_established_failure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAfterVerdict {
        calls: AtomicUsize,
        healthy: usize,
    }
    impl WorkControl for CancelAfterVerdict {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.healthy
        }
    }
    let doc =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":false}}}"#).unwrap();
    let Ok(contract) = prepare_input(&doc) else {
        panic!()
    };
    let baseline = CancelAfterVerdict {
        calls: AtomicUsize::new(0),
        healthy: usize::MAX,
    };
    assert!(matches!(
        contract.explain_with_control(&JsonValue::null(), &baseline),
        ValueOutcome::Fails { .. }
    ));
    // Outcome and Explain may use different qualified execution lanes. Exhaust
    // Explain's own boundaries; once F is established, diagnostics cannot erase it.
    let completed = baseline.calls.load(Ordering::Relaxed);
    let mut established = false;
    for healthy in 0..completed {
        let result = contract.explain_with_control(
            &JsonValue::null(),
            &CancelAfterVerdict {
                calls: AtomicUsize::new(0),
                healthy,
            },
        );
        match result {
            ValueOutcome::NoVerdict { detail } => {
                assert!(!established);
                assert_eq!(detail.reason, NoVerdictReason::Cancelled);
            }
            ValueOutcome::Fails {
                problems,
                problems_complete: false,
                ..
            } => {
                assert!(problems.is_empty());
                established = true;
            }
            other => panic!("unexpected interrupted explanation: {other:?}"),
        }
    }
    assert!(
        established,
        "exercise interruption after F and before diagnostic completion"
    );
    assert!(
        matches!(contract.explain(&JsonValue::null()), ValueOutcome::Fails { problems, .. } if !problems.is_empty())
    );
}

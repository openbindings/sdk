use openbindings::*;
use std::sync::atomic::AtomicBool;

const SECRET: &str = "PRIVATE-SOURCE-CREDENTIAL";
fn redacted(value: &dyn std::fmt::Debug) {
    for text in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(!text.contains(SECRET), "{text}");
    }
}

#[test]
fn exact_views_and_authoring_owners_do_not_print_source_data() {
    let value = JsonValue::parse(format!(r#"{{"{SECRET}":"{SECRET}"}}"#)).unwrap();
    let member = value.view().members().unwrap().next().unwrap();
    redacted(&value);
    redacted(&value.view());
    redacted(&member);
    redacted(&member.name.to_owned());
    redacted(&member.value.location());
    redacted(&member.value.as_string().unwrap());
    let parsed = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-{SECRET}":"{SECRET}"}}"#
    ))
    .unwrap();
    redacted(&parsed);
    redacted(&parsed.clone());
    redacted(&Source {
        kind: SECRET.into(),
        data: Some(value),
        ..Source::default()
    });
}

// No Debug requirement is imposed on an application evaluator or work control.
struct PrivateEvaluator;
impl SchemaEvaluator for PrivateEvaluator {
    type Validator = NeverReady;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        redacted(&request);
        redacted(&control);
        redacted(request.external_schemas);
        redacted(&request.environment(limits, control)?);
        Err(
            PreparationFailure::new(PreparationFailureKind::UnsupportedCapability {
                capability: "test-only".into(),
            })
            .with_origin(SchemaOrigin::new(None, "".parse().unwrap())),
        )
    }
}
struct NeverReady;
impl SchemaValidator for NeverReady {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        unreachable!()
    }
}

#[test]
fn evaluator_requests_and_resources_redact_original_snapshots_and_identifiers() {
    let parsed = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"enum":["{SECRET}"]}}}}}}}}"#
    ))
    .unwrap();
    let document = parsed.validate().unwrap().conformant().unwrap();
    let resource = ExternalSchema {
        uri: format!("https://example.invalid/{SECRET}"),
        document: JsonValue::parse(format!(r#"{{"const":"{SECRET}"}}"#)).unwrap(),
    };
    redacted(&resource);
    let resources = ExternalSchemas::new([resource]).unwrap();
    redacted(&resources);
    let operation = document.resolve_operation("op").unwrap();
    let contract = operation.input().unwrap();
    redacted(&contract);
    assert!(
        contract
            .prepare_with_control(
                &PrivateEvaluator,
                &resources,
                &openbindings::PreparationLimits::default(),
                &AtomicBool::new(false)
            )
            .is_err()
    );
    redacted(&ValueProblemDetails::Enum {
        choices: vec![format!("\"{SECRET}\"")],
    });
    redacted(&ValueProblemDetails::Required {
        member: SECRET.into(),
    });
}

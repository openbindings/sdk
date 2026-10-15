use openbindings::*;
use std::sync::{Arc, atomic::AtomicBool};

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
    let parsed = ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-{SECRET}":"{SECRET}"}}"#
    ))
    .unwrap();
    redacted(&parsed);
    redacted(&parsed.to_authoring().unwrap());
    redacted(&Source {
        kind: SECRET.into(),
        content: Some(value),
        ..Source::default()
    });
}

// No Debug requirement is imposed on an application evaluator or work control.
struct PrivateEvaluator;
impl SchemaEvaluator for PrivateEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        redacted(request);
        redacted(&control);
        redacted(request.supplied_resources());
        redacted(&request.evaluation_program(control)?);
        // Public records remain constructible by a downstream evaluator.
        Err(NoVerdict {
            reason: NoVerdictReason::UnsupportedCapability,
            code: "test-only".into(),
            message: "test evaluator".into(),
            location: Some(SchemaLocation {
                resource: None,
                pointer: String::new(),
            }),
        })
    }
}

#[test]
fn evaluator_requests_and_resources_redact_original_snapshots_and_identifiers() {
    let parsed = ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"enum":["{SECRET}"]}}}}}}}}"#
    ))
    .unwrap();
    let document = parsed.validate().unwrap().validated().unwrap();
    let resource = SchemaResource {
        uri: format!("https://example.invalid/{SECRET}"),
        document: JsonValue::parse(format!(r#"{{"const":"{SECRET}"}}"#)).unwrap(),
    };
    redacted(&resource);
    let contracts = document.value_contracts(
        Arc::new(PrivateEvaluator),
        ResourceSet::new([resource]).unwrap(),
    );
    redacted(&contracts);
    assert!(matches!(
        contracts.prepare_with_control("op", Direction::Input, &AtomicBool::new(false)),
        ValueContractPreparation::NoVerdict { .. }
    ));
    redacted(&ValueProblemDetails::Enum {
        choices: vec![format!("\"{SECRET}\"")],
    });
    redacted(&ValueProblemDetails::Required {
        member: SECRET.into(),
    });
}

//! Public consumer coverage: no private SDK modules or evaluator internals.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn document(schema: serde_json::Value) -> Document {
    Document::parse(
        serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}})
            .to_string(),
    )
    .unwrap()
}
fn conformant(document: &Document) -> ConformantDocument {
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
}
/// Prepare operation `op` input with the default evaluator and the given resources.
fn prepare(
    document: &Document,
    resources: &ExternalSchemas,
) -> Result<PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator>, PreparationFailure>
{
    conformant(document)
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::new(),
            resources,
            &openbindings::PreparationLimits::default(),
        )
}
fn ready(
    document: &Document,
) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    match prepare(document, &ExternalSchemas::default()) {
        Ok(contract) => contract,
        Err(detail) => panic!("expected ready: {detail:?}"),
    }
}
fn safe(message: &str) {
    assert!(
        message.len() <= 192,
        "default message must have a small fixed bound"
    );
    assert!(!message.contains("SECRET"));
    assert!(!message.chars().any(char::is_control));
}
#[test]
fn namespace_is_checked_before_preparation_or_any_selection() {
    for (text, code, pointer) in [
        (r#"{"openbindings":"0.2.0"}"#, "missing-operations", ""),
        (
            r#"{"openbindings":"0.2.0","operations":[]}"#,
            "invalid-operations-object",
            "/operations",
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":null}}"#,
            "invalid-operation-object",
            "/operations/other",
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":{"aliases":false}}}"#,
            "invalid-operation-aliases",
            "/operations/other/aliases",
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":true},"other":{"aliases":[7]}}}"#,
            "invalid-operation-alias",
            "/operations/other/aliases/0",
        ),
    ] {
        let doc = Document::parse(text).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let validation = doc.validate().unwrap();
        assert!(validation.conformant().is_none());
        let at = validation
            .report()
            .findings
            .iter()
            .filter_map(|finding| finding.location.as_ref())
            .find(|location| location.pointer.as_ref().and_then(Pointer::as_str) == Some(pointer))
            .unwrap_or_else(|| {
                panic!("missing original conformance location for {code} at {pointer}")
            });
        let raw = doc.value().at(pointer).unwrap();
        assert_eq!(at, &raw.location());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn draft_metadata_and_semantic_setup_partitions_remain_distinct() {
    let doc = Document::parse(r#"{"openbindings":"0.2.0","description":42,"operations":{"op":{"input":false,"aliases":["shared"]},"other":{"aliases":["shared"]}}}"#).unwrap();
    assert_eq!(
        doc.validate().unwrap().report().conclusion,
        Conformance::NonConformant
    );
    assert!(doc.validate().unwrap().conformant().is_none());
    assert_eq!(
        doc.validate().unwrap().report().evidence["OBI-05"],
        Evidence::Violated
    );
    let doc = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":false},"other":{}}}"#,
    )
    .unwrap();
    let proof = conformant(&doc);
    assert!(proof.resolve_operation("missing").is_none());
    assert!(
        proof
            .resolve_operation("other")
            .expect("declared operation")
            .input()
            .is_none()
    );
    let operation = proof.resolve_operation("op").expect("declared operation");
    let Ok(contract) = operation.input().expect("declared input").prepare(
        &DefaultEvaluator::new(),
        &openbindings::ExternalSchemas::default(),
        &openbindings::PreparationLimits::default(),
    ) else {
        panic!()
    };
    let outcome = contract.explain(&JsonValue::parse("null").unwrap());
    assert!(matches!(outcome, ValueOutcome::Fails { .. }));
    assert_eq!(serde_json::to_value(outcome).unwrap()["status"], "fails");
}
#[test]
fn a_cycle_costs_only_values_whose_verdict_depends_on_it() {
    // A schema applying again without consuming the instance is a stand-in:
    // a value whose verdict depends on it gives no verdict naming the schema,
    // and a dominating success branch decides in either order.
    let contract = ready(&document(
        serde_json::json!({"$ref":"#/operations/op/input"}),
    ));
    for outcome in [
        contract.evaluate(&JsonValue::parse("1").unwrap()),
        contract.explain(&JsonValue::parse("1").unwrap()),
    ] {
        let ValueOutcome::Undefined { detail } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(detail.code, "semantic-dependence");
        assert_eq!(
            detail.location,
            Some(SchemaOrigin::new(
                None,
                "/operations/op/input".parse().unwrap()
            ))
        );
        safe(&detail.message);
    }
    for schema in [
        serde_json::json!({"anyOf":[true,{"$ref":"#/operations/op/input"}]}),
        serde_json::json!({"anyOf":[{"$ref":"#/operations/op/input"},true]}),
    ] {
        let contract = ready(&document(schema));
        for value in ["1", "{}", r#""s""#] {
            let value = JsonValue::parse(value).unwrap();
            assert!(matches!(contract.evaluate(&value), ValueOutcome::Satisfies));
            assert!(matches!(contract.explain(&value), ValueOutcome::Satisfies));
        }
    }
}
#[test]
fn invalid_patterns_use_original_locations_and_safe_messages() {
    const URI: &str = "https://SECRET-user:SECRET-password@example.invalid/SECRET-resource";
    for pattern in [
        "[".to_owned(),
        format!("[{}", "SECRET-pattern".repeat(2000)),
    ] {
        // Each value's verdict depends on the pattern's undefined match.
        for (keyword, schema, reaching) in [
            (
                "pattern",
                serde_json::json!({"type":"string", "pattern":pattern}),
                r#""x""#,
            ),
            (
                "patternProperties",
                serde_json::json!({"patternProperties":{pattern:false}}),
                r#"{"x":1}"#,
            ),
        ] {
            for (entry, resources, resource, pointer) in [
                (
                    schema.clone(),
                    ExternalSchemas::default(),
                    None,
                    "/operations/op/input",
                ),
                (
                    serde_json::json!({"$defs":{"target":schema},"$ref":"#/operations/op/input/$defs/target"}),
                    ExternalSchemas::default(),
                    None,
                    "/operations/op/input/$defs/target",
                ),
                (
                    serde_json::json!({"$ref":URI}),
                    ExternalSchemas::new([ExternalSchema {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                    Some(URI.to_owned()),
                    "",
                ),
            ] {
                let contract = match prepare(&document(entry), &resources) {
                    Ok(contract) => contract,
                    Err(detail) => panic!("an invalid pattern stands in: {detail:?}"),
                };
                // A value that never reaches the pattern keeps its verdict.
                assert!(matches!(
                    contract.evaluate(&JsonValue::parse("null").unwrap()),
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ));
                let ValueOutcome::Undefined { detail } =
                    contract.explain(&JsonValue::parse(reaching).unwrap())
                else {
                    panic!("a value whose verdict depends on the pattern was decided")
                };
                assert_eq!(detail.code, "semantic-dependence");
                assert_eq!(
                    detail.location,
                    Some(SchemaOrigin::new(
                        resource,
                        format!("{pointer}/{keyword}").parse().unwrap()
                    ))
                );
                safe(&detail.message);
                let serialized = serde_json::to_string(&detail).unwrap();
                assert!(!serialized.contains("SECRET-pattern"));
                assert!(!serialized.contains("/$defs/n"));
            }
        }
    }
}
#[test]
fn reference_and_resource_messages_do_not_echo_source_identifiers() {
    let secret = "https://SECRET-user:SECRET-password@example.invalid/SECRET-path?token=SECRET-query#SECRET-fragment";
    for reference in [
        secret.to_owned(),
        format!("{secret}\nSECRET-control"),
        format!("https://example.invalid/{}", "SECRET-long".repeat(2000)),
    ] {
        let doc = document(serde_json::json!({"$ref":reference}));
        let references = doc.references();
        let ReferenceResolution::Unresolved { detail } = &references.references[0].resolution
        else {
            panic!()
        };
        safe(&detail.message);
        assert_eq!(
            detail.location.as_ref().unwrap().pointer,
            "/operations/op/input"
        );
        // Source text is still explicitly available through the reference spelling.
        assert_eq!(
            references.references[0].spelling.as_deref(),
            Some(reference.as_str())
        );
        if reference.contains('\n') {
            let validation = doc.validate().unwrap();
            assert_eq!(validation.report().evidence["OBI-11"], Evidence::Violated);
            assert!(validation.conformant().is_none());
            continue;
        }
        // A missing resource has a stand-in, and a value reaching it gives no
        // verdict; a reference to a fragment of it refuses preparation.
        // Privacy applies in both phases.
        match prepare(&doc, &ExternalSchemas::default()) {
            Ok(contract) => {
                let ValueOutcome::NoVerdict { detail } =
                    contract.explain(&JsonValue::parse("null").unwrap())
                else {
                    panic!()
                };
                assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
                safe(&detail.message);
            }
            Err(failure) => {
                assert!(reference.contains('#'));
                assert!(matches!(
                    failure.kind,
                    PreparationFailureKind::Resolution {
                        cause: ResolutionFailureKind::ResourceUnavailable { .. }
                    }
                ));
                safe(&failure.to_string());
                safe(&format!("{failure:?}"));
            }
        }
    }
    let doc = document(
        serde_json::json!({"$ref":"https://example.invalid/SECRET-id", "$defs":{"a":{"$id":"https://example.invalid/SECRET-id"},"b":{"$id":"https://example.invalid/SECRET-id"}}}),
    );
    let refs = doc.references();
    let ReferenceResolution::Unresolved { detail } = &refs.references[0].resolution else {
        panic!()
    };
    assert_eq!(detail.reason, ReferenceRefusalReason::IdentityConflict);
    assert_eq!(detail.code, "resource-identity-conflict");
    safe(&detail.message);
    for uri in [
        secret.to_owned(),
        "SECRET-relative".into(),
        format!(
            "https://example.invalid/{}#SECRET",
            "SECRET-long".repeat(2000)
        ),
    ] {
        let error = ExternalSchemas::new([ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse("true").unwrap(),
        }])
        .unwrap_err();
        safe(&error.message);
        safe(&error.to_string());
        assert_eq!(error.uri, uri);
    }
    let uri = "https://SECRET-user:SECRET-password@example.invalid/?SECRET-query";
    let item = ExternalSchema {
        uri: uri.into(),
        document: JsonValue::parse("true").unwrap(),
    };
    let error = ExternalSchemas::new([item.clone(), item]).unwrap_err();
    safe(&error.to_string());
    assert_eq!(error.message, "duplicate retrieval URI");
}
#[test]
fn messages_use_fixed_text_and_never_instance_or_schema_values() {
    let cases = [
        (serde_json::json!({"type":"integer"}), "\"SECRET-instance\""),
        (serde_json::json!({"type":["string","null"]}), "17"),
        (
            serde_json::json!({"enum":["SECRET-schema"]}),
            "\"SECRET-instance\"",
        ),
        (serde_json::json!({"required":["SECRET-member"]}), "{}"),
    ];
    for (schema, instance) in cases {
        let ValueOutcome::Fails { problems, .. } =
            ready(&document(schema)).explain(&JsonValue::parse(instance).unwrap())
        else {
            panic!()
        };
        assert!(!problems.is_empty());
        assert_eq!(problems[0].message, "value fails this schema assertion");
        for problem in problems {
            safe(&problem.message);
        }
    }
}

// This integration test is an external crate. Custom evaluators can construct
// public diagnostics, exhaustively match semantic partitions, and retain a
// wildcard for cause families that may acquire new diagnostic distinctions.
struct Custom(Arc<AtomicUsize>);
struct CustomPrepared(SchemaOrigin);
impl SchemaEvaluator for Custom {
    type Validator = CustomPrepared;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(CustomPrepared(request.schema.location()))
    }
}
impl SchemaValidator for CustomPrepared {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Fails {
            message: "value does not satisfy the schema".into(),
            problems: vec![ValueProblem::new(
                "".parse().unwrap(),
                self.0.clone(),
                "custom",
                "custom evaluator failure",
            )],
            problems_complete: true,
        }
    }
}
fn outcome_name(value: ValueOutcome) -> &'static str {
    match value {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}
fn reason_name(reason: NoVerdictReason) -> &'static str {
    match reason {
        NoVerdictReason::UnsupportedCapability => "unsupported-capability",
        _ => "other-or-future",
    }
}
fn edit_name(reason: EditError) -> &'static str {
    match reason {
        EditError::DuplicateMembers => "duplicates",
        _ => "other-or-future",
    }
}
#[test]
fn external_evaluator_construction_and_matching_stay_supported() {
    let calls = Arc::new(AtomicUsize::new(0));
    let proof = conformant(&document(serde_json::json!(true)));
    let operation = proof.resolve_operation("op").expect("declared operation");
    let state = operation.input().expect("declared input").prepare(
        &Custom(calls.clone()),
        &openbindings::ExternalSchemas::default(),
        &openbindings::PreparationLimits::default(),
    );
    match state {
        Ok(contract) => assert_eq!(
            outcome_name(contract.evaluate(&JsonValue::parse("null").unwrap())),
            "fails"
        ),
        Err(PreparationFailure { .. }) => panic!(),
    }
    let detail = UndefinedResult::new("proof", "established by a custom evaluator");
    assert_eq!(
        outcome_name(ValueOutcome::Undefined { detail }),
        "undefined"
    );
    let detail = UndefinedResult::new("custom", "established by a custom evaluator").with_location(
        SchemaOrigin::new(None, "/operations/op/input".parse().unwrap()),
    );
    assert_eq!(detail.location.unwrap().pointer, "/operations/op/input");
    assert_eq!(
        reason_name(NoVerdictReason::UnsupportedCapability),
        "unsupported-capability"
    );
    assert_eq!(edit_name(EditError::DuplicateMembers), "duplicates");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn explanations_carry_no_schema_facts_and_redact_debug() {
    let doc = document(serde_json::json!({"enum":["SECRET-schema-choice"]}));
    let contract = ready(&doc);
    for text in [format!("{contract:?}"), format!("{contract:#?}")] {
        assert!(!text.contains("SECRET"));
    }
    let outcome = contract.explain(&JsonValue::string("SECRET-instance").unwrap());
    let ValueOutcome::Fails { problems, .. } = &outcome else {
        panic!("expected failure")
    };
    // Explanations name the keyword's location and carry no schema facts.
    assert!(problems.iter().all(|problem| problem.details.is_none()));
    assert!(!serde_json::to_string(&outcome).unwrap().contains("SECRET"));
    for text in [format!("{outcome:?}"), format!("{outcome:#?}")] {
        assert!(!text.contains("SECRET"));
    }
}

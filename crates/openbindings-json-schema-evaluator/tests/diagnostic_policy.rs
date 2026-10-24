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
fn conformant(document: &Document) -> Conformant<Document> {
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
}
/// Prepare operation `op` input with the default evaluator and the given resources.
fn prepare(
    document: &Document,
    resources: &ResourceSet,
) -> Result<PreparedValueContract, NoVerdict> {
    conformant(document)
        .resolve_operation("op")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare_with(&DefaultEvaluator::new(), resources)
}
fn ready(document: &Document) -> PreparedValueContract {
    match prepare(document, &ResourceSet::default()) {
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
        let error = doc.resolve_operation("op").unwrap_err();
        assert_eq!(error.code(), code);
        let at = error.source_location().unwrap();
        assert_eq!(
            at.pointer.as_ref().and_then(openbindings::Pointer::as_str),
            Some(pointer)
        );
        let raw = doc.value().at(pointer).unwrap();
        assert_eq!(at, &raw.location());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let error = doc.resolve_operation("op").unwrap_err();
        assert_eq!(error.code(), code);
        assert_eq!(error.source_location(), Some(at));
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
    assert!(matches!(
        doc.resolve_operation("shared").unwrap(),
        OperationSelection::Ambiguous { .. }
    ));
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
    let Ok(contract) = operation
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
    else {
        panic!()
    };
    let outcome = contract.explain(&JsonValue::parse("null").unwrap());
    assert!(matches!(outcome, ValueOutcome::Fails { .. }));
    assert_eq!(serde_json::to_value(outcome).unwrap()["outcome"], "fails");
}
#[test]
fn cycles_are_conservative_even_with_a_dominating_success_branch() {
    for schema in [
        serde_json::json!({"$ref":"#/operations/op/input"}),
        serde_json::json!({"anyOf":[true,{"$ref":"#/operations/op/input"}]}),
    ] {
        let Err(detail) = prepare(&document(schema), &ResourceSet::default()) else {
            panic!()
        };
        assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
        assert_eq!(detail.code, "in-place-cycle");
        safe(&detail.message);
    }
}
#[test]
fn pattern_preparation_failures_use_original_locations_and_safe_messages() {
    const URI: &str = "https://SECRET-user:SECRET-password@example.invalid/SECRET-resource";
    for pattern in [
        "[".to_owned(),
        format!("[{}", "SECRET-pattern".repeat(2000)),
    ] {
        for (keyword, schema) in [
            (
                "pattern",
                serde_json::json!({"type":"string", "pattern":pattern}),
            ),
            (
                "patternProperties",
                serde_json::json!({"patternProperties":{pattern:true}}),
            ),
        ] {
            for (entry, resources, resource, pointer) in [
                (
                    schema.clone(),
                    ResourceSet::default(),
                    None,
                    "/operations/op/input",
                ),
                (
                    serde_json::json!({"$defs":{"target":schema},"$ref":"#/operations/op/input/$defs/target"}),
                    ResourceSet::default(),
                    None,
                    "/operations/op/input/$defs/target",
                ),
                (
                    serde_json::json!({"$ref":URI}),
                    ResourceSet::new([SchemaResource {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                    Some(URI.to_owned()),
                    "",
                ),
            ] {
                let Err(detail) = prepare(&document(entry), &resources) else {
                    panic!("an uncompilable pattern must refuse preparation")
                };
                assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
                assert_eq!(detail.code, "schema-pattern-compilation");
                assert_eq!(
                    detail.message,
                    format!("the schema {keyword} regular expression could not be compiled")
                );
                assert_eq!(
                    detail.location,
                    Some(SchemaLocation::new(
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
        let references = doc.references().unwrap();
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
        let detail = match prepare(&doc, &ResourceSet::default()) {
            Ok(contract) => {
                let ValueOutcome::NoVerdict { detail } =
                    contract.explain(&JsonValue::parse("null").unwrap())
                else {
                    panic!()
                };
                detail
            }
            Err(detail) => {
                assert!(reference.contains('#'));
                detail
            }
        };
        assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
        safe(&detail.message);
    }
    let doc = document(
        serde_json::json!({"$ref":"https://example.invalid/SECRET-id", "$defs":{"a":{"$id":"https://example.invalid/SECRET-id"},"b":{"$id":"https://example.invalid/SECRET-id"}}}),
    );
    let refs = doc.references().unwrap();
    let ReferenceResolution::Unresolved { detail } = &refs.references[0].resolution else {
        panic!()
    };
    assert_eq!(detail.code, "ambiguous-resource");
    safe(&detail.message);
    for uri in [
        secret.to_owned(),
        "SECRET-relative".into(),
        format!(
            "https://example.invalid/{}#SECRET",
            "SECRET-long".repeat(2000)
        ),
    ] {
        let error = ResourceSet::new([SchemaResource {
            uri: uri.clone(),
            document: JsonValue::parse("true").unwrap(),
        }])
        .unwrap_err();
        safe(&error.message);
        safe(&error.to_string());
        assert_eq!(error.uri, uri);
    }
    let uri = "https://SECRET-user:SECRET-password@example.invalid/?SECRET-query";
    let item = SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse("true").unwrap(),
    };
    let error = ResourceSet::new([item.clone(), item]).unwrap_err();
    safe(&error.to_string());
    assert_eq!(error.message, "duplicate retrieval URI");
}
#[test]
fn messages_use_only_bounded_fixed_type_names_and_never_instance_or_schema_values() {
    let cases = [
        (
            serde_json::json!({"type":"integer"}),
            "\"SECRET-instance\"",
            "expected JSON type: integer",
        ),
        (
            serde_json::json!({"type":["string","null"]}),
            "17",
            "expected one of JSON types: null, string",
        ),
        (
            serde_json::json!({"enum":["SECRET-schema"]}),
            "\"SECRET-instance\"",
            "value is not one of the allowed values; inspect enum at the schema location",
        ),
        (
            serde_json::json!({"required":["SECRET-member"]}),
            "{}",
            "object is missing a required member; inspect the required keyword at the schema location",
        ),
    ];
    for (schema, instance, expected) in cases {
        let ValueOutcome::Fails { problems, .. } =
            ready(&document(schema)).explain(&JsonValue::parse(instance).unwrap())
        else {
            panic!()
        };
        assert!(!problems.is_empty());
        assert_eq!(problems[0].message, expected);
        for problem in problems {
            safe(&problem.message);
        }
    }
}

// This integration test is an external crate. Custom evaluators can construct
// public diagnostics, exhaustively match semantic partitions, and retain a
// wildcard for cause families that may acquire new diagnostic distinctions.
struct Custom(Arc<AtomicUsize>);
struct CustomPrepared(SchemaLocation);
impl SchemaEvaluator for Custom {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(CustomPrepared(request.entry_location())))
    }
}
impl PreparedSchema for CustomPrepared {
    fn validate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Fails {
            message: "value does not satisfy the value contract".into(),
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
fn interpretation_name(reason: InterpretationError) -> &'static str {
    match reason {
        InterpretationError::DuplicateMembers => "duplicates",
        _ => "other-or-future",
    }
}
#[test]
fn external_evaluator_construction_and_matching_stay_supported() {
    let calls = Arc::new(AtomicUsize::new(0));
    let proof = conformant(&document(serde_json::json!(true)));
    let operation = proof.resolve_operation("op").expect("declared operation");
    let state = operation
        .input()
        .expect("declared input")
        .prepare(&Custom(calls.clone()));
    match state {
        Ok(contract) => assert_eq!(
            outcome_name(contract.validate(&JsonValue::parse("null").unwrap())),
            "fails"
        ),
        Err(NoVerdict { .. }) => panic!(),
    }
    let detail = UndefinedResult::new("proof", "established by a custom evaluator");
    assert_eq!(
        outcome_name(ValueOutcome::Undefined { detail }),
        "undefined"
    );
    let detail = UndefinedResult::new("custom", "established by a custom evaluator").with_location(
        SchemaLocation::new(None, "/operations/op/input".parse().unwrap()),
    );
    assert_eq!(detail.location.unwrap().pointer, "/operations/op/input");
    assert_eq!(
        reason_name(NoVerdictReason::UnsupportedCapability),
        "unsupported-capability"
    );
    assert_eq!(
        interpretation_name(InterpretationError::DuplicateMembers),
        "duplicates"
    );
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

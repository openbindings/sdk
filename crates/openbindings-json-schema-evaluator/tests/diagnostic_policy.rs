//! Public consumer coverage: no private SDK modules or evaluator internals.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn document(schema: serde_json::Value) -> ParsedDocument {
    ParsedDocument::parse(
        serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}})
            .to_string(),
    )
    .unwrap()
}
fn context(document: &ParsedDocument) -> ValueContracts {
    document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap()
}
fn ready(document: &ParsedDocument) -> PreparedContract {
    match context(document).prepare("op", Side::Input) {
        ContractPreparation::Ready(contract) => contract,
        state => panic!("expected ready: {state:?}"),
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
fn namespace_is_checked_before_context_creation_or_any_selection() {
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
        let doc = ParsedDocument::parse(text).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let result = doc.value_contracts(Arc::new(Custom(calls.clone())), ResourceSet::default());
        let Err(error) = result else {
            panic!("namespace must fail before preparation")
        };
        assert_eq!(error.code(), code);
        let at = error.source_location().unwrap();
        assert_eq!(at.pointer.as_deref(), Some(pointer));
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
    let doc = ParsedDocument::parse(r#"{"openbindings":"0.2.0","description":42,"operations":{"op":{"input":false,"aliases":["shared"]},"other":{"aliases":["shared"]}}}"#).unwrap();
    assert_eq!(
        doc.assess().unwrap().report().conclusion,
        Conformance::NonConformant
    );
    let context = context(&doc);
    assert!(matches!(
        context.prepare("missing", Side::Input),
        ContractPreparation::OperationMissing
    ));
    assert!(matches!(
        context.prepare("shared", Side::Input),
        ContractPreparation::OperationAmbiguous { .. }
    ));
    assert!(matches!(
        context.prepare("other", Side::Input),
        ContractPreparation::NoContract
    ));
    let ContractPreparation::Ready(contract) = context.prepare("op", Side::Input) else {
        panic!()
    };
    let outcome = contract.validate(&JsonValue::parse("null").unwrap());
    assert!(matches!(outcome, ValueOutcome::Fails { .. }));
    assert_eq!(serde_json::to_value(outcome).unwrap()["outcome"], "fails");
}
#[test]
fn cycles_are_conservative_even_with_a_dominating_success_branch() {
    for schema in [
        serde_json::json!({"$ref":"#/operations/op/input"}),
        serde_json::json!({"anyOf":[true,{"$ref":"#/operations/op/input"}]}),
    ] {
        let ContractPreparation::NoVerdict { detail } =
            context(&document(schema)).prepare("op", Side::Input)
        else {
            panic!()
        };
        assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
        assert_eq!(detail.code, "in-place-cycle");
        safe(&detail.message);
    }
}
#[test]
fn pattern_preparation_failures_are_actionable_without_source_echo_or_guessed_locations() {
    const URI: &str = "https://SECRET-user:SECRET-password@example.invalid/SECRET-resource";
    for pattern in [
        "[".to_owned(),
        format!("[{}", "SECRET-pattern".repeat(2000)),
    ] {
        for schema in [
            serde_json::json!({"type":"string", "pattern":pattern}),
            serde_json::json!({"patternProperties":{pattern:true}}),
        ] {
            for (entry, resources) in [
                (schema.clone(), ResourceSet::default()),
                (
                    serde_json::json!({"$defs":{"target":schema},"$ref":"#/operations/op/input/$defs/target"}),
                    ResourceSet::default(),
                ),
                (
                    serde_json::json!({"$ref":URI}),
                    ResourceSet::new([SchemaResource {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                ),
            ] {
                let context = document(entry)
                    .value_contracts(Arc::new(DefaultEvaluator::new()), resources)
                    .unwrap();
                let ContractPreparation::NoVerdict { detail } = context.prepare("op", Side::Input)
                else {
                    panic!("an uncompilable pattern must refuse preparation")
                };
                assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
                assert_eq!(detail.code, "schema-pattern-compilation");
                assert_eq!(
                    detail.message,
                    "a schema regular expression could not be compiled; inspect pattern and patternProperties"
                );
                assert_eq!(detail.location, None);
                safe(&detail.message);
                let serialized = serde_json::to_string(&detail).unwrap();
                assert!(!serialized.contains("SECRET"));
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
        // Valid absent carriers now defer refusal to validation; malformed URI
        // syntax still refuses during preparation. Privacy applies in both phases.
        let detail = match context(&doc).prepare("op", Side::Input) {
            ContractPreparation::Ready(contract) => {
                assert!(!reference.contains('\n'));
                let ValueOutcome::NoVerdict { detail } =
                    contract.validate(&JsonValue::parse("null").unwrap())
                else {
                    panic!()
                };
                assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
                detail
            }
            ContractPreparation::NoVerdict { detail } => {
                assert!(reference.contains('\n'));
                detail
            }
            other => panic!("unexpected setup: {other:?}"),
        };
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
            ready(&document(schema)).validate(&JsonValue::parse(instance).unwrap())
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
struct CustomPrepared;
impl SchemaEvaluator for Custom {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(CustomPrepared))
    }
}
impl PreparedSchema for CustomPrepared {
    fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
        ValueOutcome::Fails {
            problems: vec![ValueProblem {
                details: None,
                instance_pointer: "".into(),
                schema_location: None,
                code: "custom".into(),
                message: "custom evaluator failure".into(),
            }],
            problems_complete: true,
        }
    }
}
fn outcome_name(value: ValueOutcome) -> &'static str {
    match value {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    }
}
fn reason_name(reason: NoVerdictReason) -> &'static str {
    match reason {
        NoVerdictReason::Undefined => "undefined",
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
    let context = document(serde_json::json!(true))
        .value_contracts(Arc::new(Custom(calls.clone())), ResourceSet::default())
        .unwrap();
    let state = context.prepare("op", Side::Input);
    match state {
        ContractPreparation::Ready(contract) => assert_eq!(
            outcome_name(contract.validate(&JsonValue::parse("null").unwrap())),
            "fails"
        ),
        ContractPreparation::NoContract
        | ContractPreparation::OperationMissing
        | ContractPreparation::OperationAmbiguous { .. }
        | ContractPreparation::NoVerdict { .. } => panic!(),
    }
    let detail = NoVerdict {
        reason: NoVerdictReason::Undefined,
        code: "proof".into(),
        message: "established by a custom evaluator".into(),
        location: None,
    };
    assert_eq!(reason_name(detail.reason), "undefined");
    assert_eq!(
        interpretation_name(InterpretationError::DuplicateMembers),
        "duplicates"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

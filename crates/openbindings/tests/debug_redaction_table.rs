//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use std::fmt::Debug;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
// Keep the independent review's exact credential and pointer probe value.
const SECRET: &str = "REVIEW-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}
fn evidence() -> BTreeMap<String, JsonValue> {
    BTreeMap::from([(
        format!("credential-{SECRET}"),
        JsonValue::string(SECRET).unwrap(),
    )])
}
fn location() -> SchemaLocation {
    SchemaLocation {
        resource: Some(SECRET.into()),
        pointer: format!("/{SECRET}"),
    }
}
fn refusal() -> NoVerdict {
    NoVerdict {
        reason: NoVerdictReason::EvaluatorFailure,
        code: SECRET.into(),
        message: format!("provider refused schema operand {SECRET}"),
        location: Some(location()),
    }
}
#[derive(Default)]
struct Capture(Mutex<Option<SchemaRequest>>);
impl SchemaEvaluator for Capture {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        *self.0.lock().unwrap() = Some(request.clone());
        Err(refusal())
    }
}
fn authoring_rows(source: &Source, operation: &Operation, context: &JsonValue) -> Vec<Row> {
    let binding = Binding {
        operation: SECRET.into(),
        source: SECRET.into(),
        content: Some(context.clone()),
        description: Some(SECRET.into()),
        additional_fields: evidence(),
        ..Default::default()
    };
    let dependency = Dependency {
        operation: SECRET.into(),
        kinds: Some(vec![SECRET.into()]),
        description: Some(SECRET.into()),
        additional_fields: evidence(),
    };
    let document = Document {
        openbindings: SECRET.into(),
        name: Some(SECRET.into()),
        version: Some(SECRET.into()),
        description: Some(SECRET.into()),
        operations: BTreeMap::from([(SECRET.into(), operation.clone())]),
        schemas: Some(evidence()),
        sources: Some(BTreeMap::from([(SECRET.into(), source.clone())])),
        bindings: Some(BTreeMap::from([(SECRET.into(), binding.clone())])),
        dependencies: Some(BTreeMap::from([(SECRET.into(), dependency.clone())])),
        additional_fields: evidence(),
    };
    vec![
        row("Document", &document),
        row("Source", source),
        row("Operation", operation),
        row("Example", &operation.examples.as_ref().unwrap()[SECRET]),
        row("Binding", &binding),
        row("Dependency", &dependency),
    ]
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let context = JsonValue::parse(
        serde_json::json!({"authorization":format!("Bearer {SECRET}"), SECRET: SECRET}).to_string(),
    )
    .unwrap();
    let reference = format!("https://user:{SECRET}@example.invalid/schema?access_token={SECRET}");
    let parsed_ref = ParsedDocument::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"$ref":reference}}}}).to_string()).unwrap();
    let parsed = ParsedDocument::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
    let validation = parsed.validate().unwrap();
    let validated = validation.validated().unwrap();
    let source_location = SourceLocation {
        pointer: Some(format!("/{SECRET}")),
        byte_offset: 42,
        line: 2,
        byte_column: 3,
    };
    let finding = Finding {
        rule: SECRET,
        status: Evidence::Violated,
        code: SECRET,
        location: Some(source_location.clone()),
        message: SECRET.into(),
    };
    let version = VersionRefusal {
        declared: SECRET.into(),
        supported: SECRET,
    };
    let reference_refusal = ReferenceRefusal {
        reason: ReferenceRefusalReason::ResourceUnavailable,
        code: SECRET.into(),
        message: SECRET.into(),
        location: Some(location()),
    };
    let reference_item = Reference {
        location: location(),
        keyword: SECRET.into(),
        spelling: Some(reference.clone()),
        resolution: ReferenceResolution::Unresolved {
            detail: reference_refusal.clone(),
        },
    };
    let problem = ValueProblem {
        instance_pointer: format!("/{SECRET}"),
        schema_location: Some(location()),
        code: SECRET.into(),
        message: SECRET.into(),
        details: Some(ValueProblemDetails::Required {
            member: SECRET.into(),
        }),
    };
    let outcome = ValueOutcome::Fails {
        message: SECRET.into(),
        problems: vec![problem.clone()],
        problems_complete: true,
    };
    let mut rows = vec![
        row("SchemaLocation", &location()),
        row("Finding", &finding),
        row(
            "ValidationReport",
            &ValidationReport {
                release: SECRET,
                revision: SECRET,
                conclusion: Conformance::NonConformant,
                evidence: BTreeMap::from([(SECRET, Evidence::Violated)]),
                findings: vec![finding],
                findings_truncated: false,
            },
        ),
        row("VersionRefusal", &version),
        row(
            "InterpretationError::Version",
            &InterpretationError::Version(version.clone()),
        ),
        row(
            "InterpretationError::InvalidField",
            &InterpretationError::InvalidField {
                code: SECRET,
                location: source_location,
            },
        ),
        row("ParsedDocument", &parsed),
        row("DocumentValidation", &validation),
        row("ValidatedDocument", &validated),
        row(
            "OperationSelection",
            &OperationSelection::Ambiguous {
                candidates: vec![SECRET.into()],
            },
        ),
        row(
            "ValueContractPreparation::OperationAmbiguous",
            &ValueContractPreparation::OperationAmbiguous {
                candidates: vec![SECRET.into()],
            },
        ),
        row("ReferenceRefusal", &reference_refusal),
        row("Reference", &reference_item),
        row(
            "ReferenceResolution::Located",
            &ReferenceResolution::Located {
                target: location(),
                dynamic_lookup: true,
            },
        ),
        row(
            "ReferenceReport",
            &ReferenceReport {
                references: vec![reference_item],
                complete: false,
                limitation: Some(reference_refusal),
            },
        ),
        row("actual reference report", &parsed_ref.references().unwrap()),
        row("NoVerdict", &refusal()),
        row("ValueProblem", &problem),
        row("ValueOutcome::Fails", &outcome),
        row(
            "ValueOutcome::NoVerdict",
            &ValueOutcome::NoVerdict { detail: refusal() },
        ),
        row(
            "ValueContractPreparation::NoVerdict",
            &ValueContractPreparation::NoVerdict { detail: refusal() },
        ),
    ];
    assert!(serde_json::to_string(&outcome).unwrap().contains(SECRET));
    assert!(refusal().to_string().contains(SECRET));
    assert!(version.to_string().contains(SECRET));
    assert!(
        parsed_ref.references().unwrap().references[0]
            .spelling
            .as_ref()
            .unwrap()
            .contains(SECRET)
    );
    assert!(format!("{problem:?}").contains("schema_location"));
    assert!(format!("{:?}", refusal()).contains("EvaluatorFailure"));
    for detail in [
        ValueProblemDetails::Type {
            expected: vec![SECRET.into()],
        },
        ValueProblemDetails::Required {
            member: SECRET.into(),
        },
        ValueProblemDetails::NumericBound {
            bound: SECRET.into(),
        },
        ValueProblemDetails::SizeBound {
            bound: SECRET.into(),
        },
        ValueProblemDetails::Enum {
            choices: vec![format!("\"{SECRET}\"")],
        },
    ] {
        rows.push(row("ValueProblemDetails", &detail));
    }
    let resource = SchemaResource {
        uri: reference.clone(),
        document: context.clone(),
    };
    let resources = ResourceSet::new([resource.clone()]).unwrap();
    rows.extend([
        row("SchemaResource", &resource),
        row("ResourceSet", &resources),
    ]);
    let capture = Arc::new(Capture::default());
    let captured = validated
        .value_contracts(capture.clone(), resources)
        .prepare("op", Direction::Input);
    rows.push(row("capturing preparation", &captured));
    let request = capture.0.lock().unwrap().take().unwrap();
    rows.push(row("SchemaRequest", &request));
    rows.push(row(
        "EvaluationProgram",
        &request
            .evaluation_program(&std::sync::atomic::AtomicBool::new(false))
            .unwrap(),
    ));

    for (uri, texts, reason) in [
        (
            SECRET,
            vec!["{}".into()],
            "retrieval URI must be absolute with no nonempty fragment",
        ),
        (
            reference.as_str(),
            vec!["{}".into(), "{}".into()],
            "duplicate retrieval URI",
        ),
        (
            reference.as_str(),
            vec![format!("{{\"{SECRET}\":1,\"{SECRET}\":2}}")],
            "resource repeats JSON member names",
        ),
    ] {
        let error = ResourceSet::new(texts.into_iter().map(|text| SchemaResource {
            uri: uri.into(),
            document: JsonValue::parse(text).unwrap(),
        }))
        .unwrap_err();
        assert_eq!(error.message, reason);
        assert_eq!(error.to_string(), reason);
        assert!(error.uri.contains(SECRET));
        let item = row("actual ResourceError", &error);
        assert!(item.text.iter().all(|text| text.contains(reason)));
        rows.push(item);
    }
    let error = ResourceError {
        uri: SECRET.into(),
        message: "invalid retrieval URI".into(),
    };
    assert!(format!("{error:?}").contains("invalid retrieval URI"));
    rows.push(row("ResourceError::invalid", &error));
    rows.push(row(
        "ResourceError::custom",
        &ResourceError {
            uri: SECRET.into(),
            message: SECRET.into(),
        },
    ));

    let mut draft = Document::new();
    let mut operation = Operation::default();
    operation
        .additional_fields
        .insert("description".into(), JsonValue::null());
    draft.operations.insert(SECRET.into(), operation);
    let authoring = draft.build().unwrap_err();
    assert!(authoring.draft_pointer().unwrap().contains(SECRET));
    rows.push(row("AuthoringError", &authoring));
    let source = Source {
        kind: SECRET.into(),
        content: Some(context.clone()),
        description: Some(SECRET.into()),
        additional_fields: BTreeMap::from([(format!("x-{SECRET}"), context.clone())]),
    };
    let operation = Operation {
        description: Some(SECRET.into()),
        tags: Some(vec![SECRET.into()]),
        aliases: Some(vec![SECRET.into()]),
        input: Some(context.clone()),
        output: Some(context.clone()),
        examples: Some(BTreeMap::from([(
            SECRET.into(),
            Example {
                description: Some(SECRET.into()),
                input: Some(context.clone()),
                output: Some(context.clone()),
                additional_fields: evidence(),
            },
        )])),
        additional_fields: evidence(),
        ..Default::default()
    };
    rows.extend(authoring_rows(&source, &operation, &context));

    // Public records accept custom codes; only exact fixed vocabulary is shown.
    for code in ["type", "required", "enum"] {
        let mut known = problem.clone();
        known.code = code.into();
        rows.push(row_with_fields(
            "ValueProblem fixed code",
            &known,
            &[("code", code)],
        ));
        known.code = format!("{code}-{SECRET}");
        rows.push(row_with_fields(
            "ValueProblem custom code",
            &known,
            &[("code", "<redacted>")],
        ));
    }

    // Keep the reviewer's real missing-binding-reference document and both findings.
    let decision = validate_document(r#"{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"operation":"missing","source":"missing"}}}"#).unwrap();
    assert_eq!(
        decision
            .report()
            .findings
            .iter()
            .map(|f| (f.rule, f.code))
            .collect::<Vec<_>>(),
        [
            ("OBI-06", "unknown-operation"),
            ("OBI-07", "unknown-source")
        ]
    );
    rows.push(row_with_fields(
        "DocumentValidation fixed reasons",
        &decision,
        &[
            ("rule", "OBI-06"),
            ("code", "unknown-operation"),
            ("rule", "OBI-07"),
            ("code", "unknown-source"),
        ],
    ));
    for finding in &decision.report().findings {
        let mut known = finding.clone();
        known.message = SECRET.into();
        known.location.as_mut().unwrap().pointer = Some(format!("/{SECRET}"));
        rows.push(row_with_fields(
            "Finding fixed reason",
            &known,
            &[("rule", known.rule), ("code", known.code)],
        ));
        let custom_code = Finding {
            code: SECRET,
            ..known.clone()
        };
        rows.push(row_with_fields(
            "Finding custom code",
            &custom_code,
            &[("rule", known.rule), ("code", "<redacted>")],
        ));
        let custom_rule = Finding {
            rule: SECRET,
            ..known
        };
        rows.push(row_with_fields(
            "Finding custom rule",
            &custom_rule,
            &[("rule", "<redacted>"), ("code", custom_rule.code)],
        ));
    }
    for rule in DOCUMENT_RULES {
        let finding = Finding {
            rule,
            code: "schema-mismatch",
            status: Evidence::Violated,
            location: Some(SourceLocation {
                pointer: Some(format!("/{SECRET}")),
                byte_offset: 42,
                line: 2,
                byte_column: 3,
            }),
            message: SECRET.into(),
        };
        rows.push(row_with_fields(
            "Finding known OBI rule",
            &finding,
            &[("rule", rule), ("code", "schema-mismatch")],
        ));
    }
    for rule in ["OBI-14", "OBI-01-REVIEW-BEARER-TOKEN-7391"] {
        let finding = Finding {
            rule,
            code: "schema-mismatch",
            status: Evidence::Violated,
            location: None,
            message: SECRET.into(),
        };
        rows.push(row_with_fields(
            "Finding unknown OBI rule",
            &finding,
            &[("rule", "<redacted>"), ("code", "schema-mismatch")],
        ));
    }
    // Parser and admission reasons are also nested in normative assessments.
    for (input, limits, code) in [
        ("[0,]", JsonLimits::default(), "value"),
        (
            "[0]",
            JsonLimits {
                max_bytes: 0,
                ..Default::default()
            },
            "input-limit",
        ),
    ] {
        let decision = validate_document_with_limits(input, limits).unwrap();
        assert_eq!(decision.report().findings[0].code, code);
        rows.push(row_with_fields(
            "DocumentValidation admission reason",
            &decision,
            &[("rule", "OBI-01"), ("code", code)],
        ));
    }

    check_rows(rows, 73);
}

fn row_with_fields(label: &'static str, value: &impl Debug, fields: &[(&str, &str)]) -> Row {
    let item = row(label, value);
    for (field, expected) in fields {
        let expected = format!("{field}: {expected:?}");
        assert!(
            item.text.iter().all(|text| text.contains(&expected)),
            "{label}: missing {expected}"
        );
    }
    item
}

#[test]
fn undefined_result_redacts_text_and_locations_but_shows_the_fixed_code() {
    let custom = UndefinedResult::new(SECRET, SECRET).with_location(location());
    let fixed = UndefinedResult::new("static-reference-defect", SECRET).with_location(location());
    let lookalike = UndefinedResult::new(format!("static-reference-defect-{SECRET}"), SECRET);
    let fixed_row = row("UndefinedResult fixed code", &fixed);
    for text in &fixed_row.text {
        assert!(text.contains("static-reference-defect"));
    }
    let outcome = ValueOutcome::Undefined {
        detail: fixed.clone(),
    };
    let outcome_row = row("ValueOutcome::Undefined fixed code", &outcome);
    for text in &outcome_row.text {
        assert!(text.contains("static-reference-defect"));
    }
    check_rows(
        vec![
            row("UndefinedResult custom code", &custom),
            row("UndefinedResult lookalike code", &lookalike),
            row(
                "ValueOutcome::Undefined custom code",
                &ValueOutcome::Undefined { detail: custom },
            ),
            fixed_row,
            outcome_row,
        ],
        5,
    );
    let wire = serde_json::to_value(&outcome).unwrap();
    assert_eq!(wire["outcome"], "undefined");
    assert!(wire["detail"].get("reason").is_none());
    assert_eq!(wire["detail"]["message"], SECRET);
    assert_eq!(wire["detail"]["location"]["pointer"], format!("/{SECRET}"));
    assert_eq!(
        serde_json::to_value(UndefinedResult::new("custom", "proof")).unwrap()["location"],
        serde_json::Value::Null
    );
}

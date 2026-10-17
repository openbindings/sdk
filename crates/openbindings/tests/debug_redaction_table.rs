//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use std::fmt::Debug;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
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

/// Prepares nothing and refuses with a caller-chosen code.
struct Refuse(&'static str);
impl SchemaEvaluator for Refuse {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let mut detail = refusal();
        detail.code = self.0.into();
        Err(detail)
    }
}
#[test]
fn preparation_refusals_redact_payloads_like_no_verdict() {
    let document =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":true}}}"#)
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
    let operation = document.resolve_operation("op").unwrap();
    let contract = operation.input().unwrap();
    let mut rows = vec![row("ValueContract", &contract)];
    for code in [
        "caller-cancelled",
        "resource-unavailable",
        "schema-pattern-compilation",
        SECRET,
        "caller-cancelled-PROBE-BEARER-TOKEN-7391",
    ] {
        let error = contract.prepare(&Refuse(code)).unwrap_err();
        // A preparation refusal is the evaluator's NoVerdict: same redaction, no code text.
        assert_eq!(error.code, code);
        // Only the broad reason is shown, never the code, even a recognized one.
        assert!(!format!("{error:?}").contains(code));
        rows.push(row("preparation refusal code", &error));
        assert!(error.to_string().contains(SECRET));
        let error: &dyn std::error::Error = &error;
        assert!(error.source().is_none());
    }
    rows.push(row(
        "refused preparation result",
        &contract.prepare(&Refuse(SECRET)),
    ));
    check_rows(rows, 7);
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
        pointer: format!("/{SECRET}").parse().unwrap(),
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
/// Accepts every value, so a prepared contract exists to inspect.
struct Accept;
impl SchemaEvaluator for Accept {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Accept))
    }
}
impl PreparedSchema for Accept {
    fn validate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
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
    let document = Document::parse(
        serde_json::json!({
            "openbindings": SECRET, "name": SECRET, "version": SECRET,
            "description": SECRET, "operations": {SECRET: &operation},
            "schemas": {SECRET: &context}, "sources": {SECRET: &source},
            "bindings": {SECRET: &binding}, "dependencies": {SECRET: &dependency},
            "x-private": &context,
        })
        .to_string(),
    )
    .unwrap();
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
    let parsed_ref = Document::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"$ref":reference}}}}).to_string()).unwrap();
    let parsed = Document::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{"op":{"input":{"properties":{SECRET:{"type":"string"}}}}}}).to_string()).unwrap();
    let validation = parsed.validate().unwrap();
    let validated = validation.conformant().unwrap();
    let source_location = SourceLocation {
        pointer: Some(format!("/{SECRET}").parse().unwrap()),
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
        instance: format!("/{SECRET}").parse().unwrap(),
        schema: location(),
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
        row("Document", &parsed),
        row("DocumentValidation", &validation),
        row("Conformant<Document>", &validated),
        row(
            "OperationSelection",
            &OperationSelection::Ambiguous {
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
    // Both locations are shown structurally, without their pointers.
    assert!(format!("{problem:?}").contains("instance: Pointer"));
    assert!(format!("{problem:?}").contains("schema: SchemaLocation"));
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
    let capture = Capture::default();
    let operation = validated.resolve_operation("op").unwrap();
    let contract = operation.input().unwrap();
    rows.push(row("ValueContract", &contract));
    let captured = contract.prepare_with(&capture, &resources);
    rows.push(row("capturing preparation", &captured));
    let prepared = contract.prepare(&Accept).unwrap();
    rows.push(row("PreparedContract", &prepared));
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
    let EditError::Authoring(authoring) = draft.set_operation(SECRET, &operation).unwrap_err()
    else {
        panic!()
    };
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

    // A binding whose operation and source are both missing yields both findings.
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
        known.location.as_mut().unwrap().pointer = Some(format!("/{SECRET}").parse().unwrap());
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
                pointer: Some(format!("/{SECRET}").parse().unwrap()),
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
    for rule in ["OBI-14", "OBI-01-PROBE-BEARER-TOKEN-7391"] {
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

//! Diagnostic Display and Debug hold fixed SDK text and SDK constants only.
//! Document, caller, provider and evaluator values, including derived values,
//! remain reachable through deliberate fields, accessors and serialization.
//! Escaping or truncating a value does not make it value-free. Wrappers print
//! their own layer and expose their cause through Error::source(). Value-type
//! Display is deliberate rendering; diagnostic locations show coordinates only.
use openbindings::*;
use std::fmt::{Debug, Display};
use std::{collections::BTreeMap, sync::Mutex};
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: Vec<String>,
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ]
        .to_vec(),
    }
}

fn diagnostic_row(label: &'static str, value: &(impl Debug + Display), retained: &str) -> Row {
    assert!(
        retained.contains(SECRET),
        "{label} retains the probe through explicit access"
    );
    let mut item = row(label, value);
    item.text.extend([format!("{value}"), format!("{value:#}")]);
    item
}

/// Prepares nothing and refuses with a caller-chosen code.
struct Refuse(&'static str);
impl SchemaEvaluator for Refuse {
    type Validator = Accept;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Err(
            PreparationFailure::new(PreparationFailureKind::UnsupportedCapability {
                capability: self.0.into(),
            })
            .with_origin(location()),
        )
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
    let mut rows = vec![row("Schema", &contract)];
    for code in [
        "caller-cancelled",
        "resource-unavailable",
        "schema-pattern-compilation",
        SECRET,
        "caller-cancelled-PROBE-BEARER-TOKEN-7391",
    ] {
        let error = contract
            .prepare(&Refuse(code), &openbindings::ExternalSchemas::default())
            .unwrap_err();
        // Preparation failure retains its structured cause without exposing it in Debug.
        assert_eq!(
            error.kind,
            PreparationFailureKind::UnsupportedCapability {
                capability: code.into()
            }
        );
        // Only the broad reason is shown, never the code, even a recognized one.
        assert!(!format!("{error:?}").contains(code));
        rows.push(row("preparation refusal code", &error));
        assert!(!error.to_string().contains(SECRET));
        let error: &dyn std::error::Error = &error;
        assert!(error.source().is_none());
    }
    rows.push(row(
        "refused preparation result",
        &contract.prepare(&Refuse(SECRET), &openbindings::ExternalSchemas::default()),
    ));
    for kind in [
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable { uri: SECRET.into() },
        },
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::UnsupportedDialect { uri: SECRET.into() },
        },
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::Limit {
                resource: SECRET.into(),
            },
        },
        PreparationFailureKind::Limit {
            resource: SECRET.into(),
        },
        PreparationFailureKind::Implementation {
            message: SECRET.into(),
        },
    ] {
        let error = PreparationFailure::new(kind).with_origin(location());
        let retained = serde_json::to_string(&error).unwrap();
        rows.push(diagnostic_row("PreparationFailure", &error, &retained));
        rows.push(row("PreparationFailureKind", &error.kind));
        if let PreparationFailureKind::Resolution { cause } = &error.kind {
            assert!(!format!("{cause:?}").contains(SECRET));
        }
    }
    let at = probe_location();
    assert_eq!(at.to_string(), "line 2, column 3, byte 42");
    rows.push(diagnostic_row(
        "SourceLocation",
        &at,
        &at.pointer().unwrap().to_string(),
    ));
    check_rows(rows, 18);
}
fn location() -> SchemaOrigin {
    SchemaOrigin::new(Some(SECRET.into()), format!("/{SECRET}").parse().unwrap())
}
fn refusal() -> NoVerdict {
    NoVerdict::new(
        NoVerdictReason::EvaluatorFailure,
        SECRET,
        format!("provider refused schema operand {SECRET}"),
    )
    .with_location(location())
}
// Validation builds findings, locations and reports; a caller receives them and
// can reassign their public fields, so these probes fill actual records with
// caller text.
const UNKNOWN_TARGETS: &str = r#"{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"operation":"missing","source":"missing"}}}"#;
fn probe_location() -> SourceLocation {
    let mut at = JsonValue::null().location();
    at.pointer = Some(format!("/{SECRET}").parse().unwrap());
    at.byte_offset = 42;
    at.line = 2;
    at.byte_column = 3;
    at
}
fn probe_finding(
    rule: &'static str,
    code: &'static str,
    location: Option<SourceLocation>,
) -> Finding {
    let mut finding = validate_document(UNKNOWN_TARGETS)
        .unwrap()
        .report()
        .findings[0]
        .clone();
    finding.rule = rule;
    finding.status = Evidence::Violated;
    finding.code = code;
    finding.location = location;
    finding.message = SECRET.into();
    finding.keyword = Some(SECRET);
    finding.schema = Some(SchemaOrigin::new(
        Some(SECRET.into()),
        format!("/{SECRET}").parse().unwrap(),
    ));
    finding.details = Some(ValueProblemDetails::Required {
        member: SECRET.into(),
    });
    finding
}
/// Accepts every value, so a prepared schema exists to inspect.
struct Accept;
impl SchemaEvaluator for Accept {
    type Validator = Accept;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Ok(Accept)
    }
}
impl SchemaValidator for Accept {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
#[derive(Default)]
struct Capture(Mutex<Option<(Row, SchemaEnvironment)>>);
impl SchemaEvaluator for Capture {
    type Validator = Accept;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        *self.0.lock().unwrap() = Some((
            row("EvaluationRequest", &request),
            request.environment(limits, &std::sync::atomic::AtomicBool::new(false))?,
        ));
        Err(
            PreparationFailure::new(PreparationFailureKind::Implementation {
                message: SECRET.into(),
            })
            .with_origin(location()),
        )
    }
}
fn domain_rows(context: &JsonValue) -> Vec<Row> {
    let document = Document::parse(serde_json::json!({
        "openbindings":"0.2.0", "name":SECRET, "version":SECRET, "description":SECRET,
        "operations":{SECRET:{"description":SECRET,"tags":[SECRET],"aliases":[format!("alias-{SECRET}")],
            "input":{"const":context},"output":{"const":context},
            "examples":{SECRET:{"description":SECRET,"input":context,"output":context,"x-private":context}},"x-private":context}},
        "sources":{SECRET:{"kind":SECRET,"data":context,"description":SECRET,"x-private":context}},
        "bindings":{SECRET:{"operation":SECRET,"source":SECRET,"data":context,"description":SECRET,"x-private":context}},
        "dependencies":{SECRET:{"operation":SECRET,"kinds":[SECRET],"description":SECRET,"x-private":context}},
        "schemas":{SECRET:{"const":context}}, "x-private":context,
    }).to_string()).unwrap().validate().unwrap().conformant().unwrap();
    let operation = document.operation(SECRET).unwrap();
    vec![
        row("Document", document.document()),
        row("Source", &document.source(SECRET).unwrap()),
        row("Operation", &operation),
        row("Example", &operation.example(SECRET).unwrap()),
        row("Binding", &document.binding(SECRET).unwrap()),
        row("Dependency", &document.dependency(SECRET).unwrap()),
    ]
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in [
            "ordinary",
            "pretty",
            "nested",
            "nested pretty",
            "display",
            "alternate display",
        ]
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
    let source_location = probe_location();
    let finding = probe_finding(SECRET, SECRET, Some(source_location.clone()));
    let mut report = validation.report().clone();
    report.release = SECRET;
    report.revision = SECRET;
    report.conclusion = Conformance::NonConformant;
    report.evidence = BTreeMap::from([(SECRET, Evidence::Violated)]);
    report.findings = vec![finding.clone()];
    report.findings_truncated = false;
    let mut version = Document::parse(r#"{"openbindings":"0.3.0","operations":{}}"#)
        .unwrap()
        .validate()
        .unwrap_err();
    version.declared = SECRET.into();
    version.supported = SECRET;
    let mut references = parsed_ref.references();
    let mut reference_item = references.references[0].clone();
    let ReferenceResolution::Unresolved { detail } = &reference_item.resolution else {
        panic!("an unsupplied resource leaves its reference unresolved")
    };
    let mut reference_refusal = detail.clone();
    reference_refusal.reason = ReferenceRefusalReason::ResourceUnavailable;
    reference_refusal.code = SECRET.into();
    reference_refusal.message = SECRET.into();
    reference_refusal.location = Some(location());
    reference_item.location = location();
    reference_item.resource_root = Some(location());
    reference_item.keyword = SECRET.into();
    reference_item.spelling = Some(reference.clone());
    reference_item.resolution = ReferenceResolution::Unresolved {
        detail: reference_refusal.clone(),
    };
    references.references = vec![reference_item.clone()];
    references.complete = false;
    references.limitation = Some(reference_refusal.clone());
    let problem = ValueProblem::new(
        format!("/{SECRET}").parse().unwrap(),
        location(),
        SECRET,
        SECRET,
    )
    .with_details(ValueProblemDetails::Required {
        member: SECRET.into(),
    });
    let outcome = ValueOutcome::Fails {
        message: SECRET.into(),
        problems: vec![problem.clone()],
        problems_complete: true,
    };
    let mut rows = vec![
        row("SchemaOrigin", &location()),
        row("Finding", &finding),
        row("ValidationReport", &report),
        diagnostic_row("VersionRefusal", &version, version.declared()),
        row("Document", &parsed),
        row("DocumentValidation", &validation),
        row("ConformantDocument", &validated),
        diagnostic_row(
            "ReferenceRefusal",
            &reference_refusal,
            &reference_refusal.message,
        ),
        row("Reference", &reference_item),
        row(
            "ReferenceResolution::Located",
            &ReferenceResolution::Located {
                target: location(),
                dynamic_lookup: true,
            },
        ),
        row("ReferenceReport", &references),
        row("actual reference report", &parsed_ref.references()),
        diagnostic_row("NoVerdict", &refusal(), &refusal().message),
        row("ValueProblem", &problem),
        row("ValueOutcome::Fails", &outcome),
        row(
            "ValueOutcome::NoVerdict",
            &ValueOutcome::NoVerdict { detail: refusal() },
        ),
    ];
    assert!(serde_json::to_string(&outcome).unwrap().contains(SECRET));
    assert!(!refusal().to_string().contains(SECRET));
    assert!(!version.to_string().contains(SECRET));
    assert!(
        parsed_ref.references().references[0]
            .spelling
            .as_ref()
            .unwrap()
            .contains(SECRET)
    );
    // Both locations are shown structurally, without their pointers.
    assert!(format!("{problem:?}").contains("instance: Pointer"));
    assert!(format!("{problem:?}").contains("schema: SchemaOrigin"));
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
    let resource = ExternalSchema {
        uri: reference.clone(),
        document: context.clone(),
    };
    let resources = ExternalSchemas::new([resource.clone()]).unwrap();
    rows.extend([
        row("ExternalSchema", &resource),
        row("ExternalSchemas", &resources),
    ]);
    let capture = Capture::default();
    let operation = validated.resolve_operation("op").unwrap();
    let contract = operation.input().unwrap();
    rows.push(row("Schema", &contract));
    let captured = contract.prepare(&capture, &resources);
    rows.push(row("capturing preparation", &captured));
    let prepared = contract
        .prepare(&Accept, &openbindings::ExternalSchemas::default())
        .unwrap();
    rows.push(row("PreparedSchema", &prepared));
    let (request, environment) = capture.0.lock().unwrap().take().unwrap();
    rows.push(request);
    rows.push(row("SchemaEnvironment", &environment));

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
        let error = ExternalSchemas::new(texts.into_iter().map(|text| ExternalSchema {
            uri: uri.into(),
            document: JsonValue::parse(text).unwrap(),
        }))
        .unwrap_err();
        assert_eq!(error.message, reason);
        assert!(!error.to_string().contains(SECRET));
        assert!(error.uri.contains(SECRET));
        let item = diagnostic_row("actual ExternalSchemaError", &error, &error.uri);
        rows.push(item);
    }
    let mut error = ExternalSchemas::new([ExternalSchema {
        uri: SECRET.into(),
        document: JsonValue::null(),
    }])
    .unwrap_err();
    error.message = "invalid retrieval URI".into();
    assert!(format!("{error:?}").contains("invalid retrieval URI"));
    rows.push(row("ExternalSchemaError::invalid", &error));
    error.message = SECRET.into();
    rows.push(diagnostic_row(
        "ExternalSchemaError::custom",
        &error,
        &error.message,
    ));

    let missing = Document::new()
        .remove(format!("/{SECRET}").as_str())
        .unwrap_err();
    rows.push(row("EditError::MissingValue", &missing));
    rows.extend(domain_rows(&context));

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
    let decision = validate_document(UNKNOWN_TARGETS).unwrap();
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
        let mut custom_code = known.clone();
        custom_code.code = SECRET;
        rows.push(row_with_fields(
            "Finding custom code",
            &custom_code,
            &[("rule", known.rule), ("code", "<redacted>")],
        ));
        let mut custom_rule = known;
        custom_rule.rule = SECRET;
        rows.push(row_with_fields(
            "Finding custom rule",
            &custom_rule,
            &[("rule", "<redacted>"), ("code", custom_rule.code)],
        ));
    }
    for rule in DOCUMENT_RULES {
        let finding = probe_finding(rule, "schema-mismatch", Some(probe_location()));
        rows.push(row_with_fields(
            "Finding known OBI rule",
            &finding,
            &[("rule", rule), ("code", "schema-mismatch")],
        ));
    }
    for rule in ["OBI-15", "OBI-01-PROBE-BEARER-TOKEN-7391"] {
        let finding = probe_finding(rule, "schema-mismatch", None);
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
            "byte-limit",
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

    check_rows(rows, 71);
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
    assert_eq!(wire["status"], "undefined");
    assert!(wire["detail"].get("reason").is_none());
    assert_eq!(wire["detail"]["message"], SECRET);
    assert_eq!(wire["detail"]["location"]["pointer"], format!("/{SECRET}"));
    assert_eq!(
        serde_json::to_value(UndefinedResult::new("custom", "proof")).unwrap()["location"],
        serde_json::Value::Null
    );
}

#[test]
fn debug_and_display_write_their_structure() {
    // Debug is structural, not empty: it names the type, the variant and the
    // fixed fields it may show. Display names the SDK category without values.
    let refusal = Document::parse(r#"{"openbindings":"0.3.0","operations":{}}"#)
        .unwrap()
        .validate()
        .unwrap_err();
    assert_eq!(
        format!("{refusal:?}"),
        r#"VersionRefusal { supported: "0.2.x", .. }"#
    );
    assert_eq!(refusal.to_string(), "declared version is outside 0.2.x");
    for (kind, name) in [
        (
            PreparationFailureKind::Resolution {
                cause: ResolutionFailureKind::ResourceUnavailable { uri: SECRET.into() },
            },
            "Resolution",
        ),
        (
            PreparationFailureKind::UnsupportedCapability {
                capability: SECRET.into(),
            },
            "UnsupportedCapability",
        ),
        (
            PreparationFailureKind::Limit {
                resource: SECRET.into(),
            },
            "Limit",
        ),
        (PreparationFailureKind::Cancelled, "Cancelled"),
        (
            PreparationFailureKind::InvalidExternalSchema,
            "InvalidExternalSchema",
        ),
        (
            PreparationFailureKind::Implementation {
                message: SECRET.into(),
            },
            "Implementation",
        ),
    ] {
        for origin in [None, Some(location())] {
            let mut failure = PreparationFailure::new(kind.clone());
            failure.origin = origin;
            let item = row_with_fields("PreparationFailure", &failure, &[("kind", name)]);
            assert!(item.text.iter().all(|text| text.contains(&format!("has_origin: {}", failure.origin.is_some()))));
            check_rows(vec![item], 1);
        }
    }
    let text = r#"{"openbindings":"0.2.0","operations":{}}"#;
    let document = Document::parse(text).unwrap();
    assert_eq!(
        format!("{document:?}"),
        format!(
            "Document {{ value: JsonValue {{ kind: Object, bytes: {} }} }}",
            text.len()
        )
    );
    for (error, debug, code) in [
        (EditError::MissingParent, "MissingParent", "missing-parent"),
        (EditError::MissingValue, "MissingValue", "missing-value"),
        (
            EditError::InvalidPointer,
            "InvalidPointer",
            "invalid-pointer",
        ),
        (
            EditError::DuplicateMembers,
            "DuplicateMembers",
            "duplicate-members",
        ),
        (EditError::NotContainer, "NotContainer", "not-container"),
        (EditError::Overlap, "Overlap", "overlapping-edits"),
    ] {
        assert_eq!(format!("{error:?}"), debug);
        assert_eq!(error.code(), code);
        assert_eq!(error.to_string(), format!("document edit refused: {code}"));
    }
}

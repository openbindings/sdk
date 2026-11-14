use openbindings::*;
use std::sync::atomic::AtomicBool;

fn complete(report: ReferenceReport) -> Vec<Reference> {
    let ReferenceReport::Complete { references } = report else {
        panic!("complete inventory required")
    };
    references
}
fn incomplete(report: ReferenceReport) -> (Vec<Reference>, ReferenceRefusal) {
    let ReferenceReport::Incomplete {
        retained,
        limitation,
    } = report
    else {
        panic!("expected an incomplete inventory")
    };
    (retained, limitation)
}

#[test]
fn reference_allowance_charges_whole_entries_at_the_exact_boundary() {
    assert_eq!(ReferenceLimits::default().max_report_bytes, 8 * 1024 * 1024);
    let document =
        Document::parse(r##"{"operations":{},"schemas":{"x":{"$ref":"#/schemas/y"},"y":true}}"##)
            .unwrap();
    let expected = complete(document.references());
    let bytes = "/schemas/x/$ref".len() + "$ref".len() + "#/schemas/y".len() + "/schemas/y".len();
    for allowance in [0, bytes - 1, bytes] {
        let report = document.references_with_limits(ReferenceLimits {
            max_report_bytes: allowance,
        });
        if allowance == bytes {
            assert_eq!(complete(report), expected);
        } else {
            let (retained, limitation) = incomplete(report);
            assert!(retained.is_empty());
            assert_eq!(limitation.code, "reference-report-bytes");
            assert!(limitation.location.is_none());
        }
    }
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://example.test/schema".into(),
        document: JsonValue::parse(r##"{"$ref":"#"}"##).unwrap(),
    }])
    .unwrap();
    let document = Document::parse(
        r#"{"operations":{},"schemas":{"x":{"$ref":"https://example.test/schema"}}}"#,
    )
    .unwrap();
    let all =
        complete(document.references_with_resources(resources.clone(), &AtomicBool::new(false)));
    assert_eq!(all.len(), 2);
    let (_, refusal) = incomplete(document.references_with_resources_and_limits(
        resources.clone(),
        ReferenceLimits {
            max_report_bytes: 0,
        },
        &AtomicBool::new(false),
    ));
    assert_eq!(refusal.code, "reference-report-bytes");
    assert_eq!(
        complete(document.references_with_resources_and_limits(
            resources.clone(),
            ReferenceLimits {
                max_report_bytes: 1024
            },
            &AtomicBool::new(false)
        )),
        all
    );
    let (_, refusal) = incomplete(document.references_with_resources_and_limits(
        resources,
        ReferenceLimits {
            max_report_bytes: usize::MAX,
        },
        &AtomicBool::new(true),
    ));
    assert_eq!(refusal.reason, ReferenceRefusalReason::Cancelled);
}

#[test]
fn the_forty_rename_sites_require_an_explicit_completion_choice() {
    let key = "a".repeat(256 * 1024);
    let entries = vec![r##"{"$ref":"#/schemas/target"}"##; 40].join(",");
    let document = Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"{key}":{{"allOf":[{entries}]}},"target":true}}}}"#)).unwrap();
    let proof = document.validate().unwrap().conformant().unwrap();
    for owner in [&document, proof.document()] {
        let (retained, limitation) = incomplete(owner.references());
        assert_eq!(retained.len(), 31);
        assert_eq!(limitation.code, "reference-report-bytes");
        // An editor can obtain a full inventory only from the Complete arm.
        let references = complete(owner.references_with_limits(ReferenceLimits {
            max_report_bytes: 16 * 1024 * 1024,
        }));
        assert_eq!(references.len(), 40);
        assert!(
            references
                .iter()
                .all(|r| r.spelling.as_deref() == Some("#/schemas/target"))
        );
        assert_eq!(incomplete(owner.references()).0.len(), 31);
    }
}

fn report(document: &Document, limits: ValidationLimits) -> ValidationReport {
    document
        .validate_with_limits(limits)
        .unwrap()
        .report()
        .clone()
}

#[test]
fn validation_count_can_be_raised_or_zero_without_changing_any_rule_evidence() {
    let schemas = (0..4200)
        .map(|i| format!(r#""s{i}":{{"type":42}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{{schemas}}}}}"#
    ))
    .unwrap();
    let baseline = document.validate().unwrap().report().clone();
    assert_eq!(baseline.findings.len(), 4096);
    assert!(baseline.findings_truncated);
    for count in [0, 1, 4095, 4096, 4097, 4200] {
        let actual = report(
            &document,
            ValidationLimits {
                max_findings: count,
                ..ValidationLimits::default()
            },
        );
        assert_eq!(actual.evidence, baseline.evidence);
        assert_eq!(actual.conclusion, Conformance::NonConformant);
        assert_eq!(actual.findings.len(), count);
        assert_eq!(actual.findings_truncated, count < 4200);
    }
    assert_eq!(document.validate().unwrap().report(), &baseline);
    assert_eq!(ValidationLimits::default().max_findings, 4096);
    assert_eq!(
        ValidationLimits::default().max_pointer_bytes,
        8 * 1024 * 1024
    );
}

#[test]
fn validation_pointer_allowance_keeps_exact_locations_and_independent_evidence() {
    let document =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"x":{"type":42}}}"#)
            .unwrap();
    let baseline = document.validate().unwrap().report().clone();
    assert_eq!(baseline.findings.len(), 1);
    let bytes = baseline.findings[0]
        .location
        .as_ref()
        .unwrap()
        .pointer
        .as_ref()
        .unwrap()
        .as_str()
        .unwrap()
        .len();
    for max_pointer_bytes in [0, bytes - 1, bytes, usize::MAX] {
        let actual = report(
            &document,
            ValidationLimits {
                max_pointer_bytes,
                ..ValidationLimits::default()
            },
        );
        assert_eq!(actual.evidence, baseline.evidence);
        assert_eq!(actual.conclusion, baseline.conclusion);
        assert_eq!(actual.findings_truncated, max_pointer_bytes < bytes);
        assert_eq!(
            actual.findings.len(),
            usize::from(max_pointer_bytes >= bytes)
        );
        if !actual.findings.is_empty() {
            assert_eq!(actual.findings, baseline.findings);
        }
    }
}

#[test]
fn zero_retention_preserves_json_failures_version_refusal_and_conformance_proof() {
    let limits = ValidationLimits {
        max_findings: 0,
        max_pointer_bytes: 0,
    };
    for text in [r#"{"x":1,"x":2}"#, r#"{"x":"\ud800"}"#] {
        let document = Document::parse(text).unwrap();
        let baseline = document.validate().unwrap();
        let actual = document.validate_with_limits(limits).unwrap();
        assert_eq!(actual.report().evidence, baseline.report().evidence);
        assert_eq!(actual.report().conclusion, Conformance::NonConformant);
        assert!(actual.report().findings.is_empty());
        assert!(actual.report().findings_truncated);
        assert!(actual.conformant().is_none());
    }
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{}}"#).unwrap();
    let validation = document.validate_with_limits(limits).unwrap();
    assert!(validation.conformant().is_some());
    assert!(!validation.report().findings_truncated);
    let document = Document::parse(r#"{"openbindings":"0.3.0","operations":{}}"#).unwrap();
    assert_eq!(
        document.validate_with_limits(limits).unwrap_err(),
        document.validate().unwrap_err()
    );
}

#[test]
fn validation_pointer_allowance_can_exceed_the_default() {
    let key = "a".repeat(256 * 1024);
    let entries = vec![r#"{"type":42}"#; 40].join(",");
    let document = Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"{key}":{{"allOf":[{entries}]}}}}}}"#)).unwrap();
    let baseline = document.validate().unwrap();
    assert!(baseline.report().findings_truncated);
    assert_eq!(baseline.report().findings.len(), 31);
    let raised = document
        .validate_with_limits(ValidationLimits {
            max_pointer_bytes: 16 * 1024 * 1024,
            ..ValidationLimits::default()
        })
        .unwrap();
    assert_eq!(raised.report().evidence, baseline.report().evidence);
    assert_eq!(raised.report().conclusion, baseline.report().conclusion);
    assert_eq!(raised.report().findings.len(), 40);
    assert!(!raised.report().findings_truncated);
}

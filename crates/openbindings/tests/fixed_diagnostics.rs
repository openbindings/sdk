use openbindings::*;

#[test]
fn direct_type_explanation_preserves_rule_location_and_rejected_value_privacy() {
    let text = "{\n  \"openbindings\":\"0.2.0\",\n  \"name\":\"café\",\n  \"operations\":{\"lookup\":{\"description\":{\"private\":\"rejected-secret\"}}}\n}";
    let assessment = assess_document(text).unwrap();
    let report = assessment.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
    assert_eq!(report.findings.len(), 1);
    assert!(!report.findings_truncated);
    let finding = &report.findings[0];
    assert_eq!(finding.rule, "OBI-02");
    assert_eq!(finding.code, "schema-mismatch");
    assert_eq!(finding.status, Evidence::Violated);
    assert_eq!(finding.message, "expected JSON type: string");
    let at = finding.location.as_ref().unwrap();
    assert_eq!(
        at.pointer.as_deref(),
        Some("/operations/lookup/description")
    );
    assert_eq!(at.byte_offset, text.find("{\"private\"").unwrap());
    assert_eq!(at.line, 4);
    assert_eq!(at.byte_column, 41);
    assert!(!finding.message.contains("private"));
    assert!(!finding.message.contains("rejected-secret"));
    let corrected = text.replace("{\"private\":\"rejected-secret\"}", "\"Find an item\"");
    let corrected = assess_document(corrected).unwrap();
    assert_eq!(corrected.report().conclusion, Conformance::Conformant);
    assert!(corrected.report().findings.is_empty());
}

#[test]
fn required_explanation_points_at_the_original_containing_object() {
    let text = " \n {\"openbindings\":\"0.2.0\"}";
    let assessment = assess_document(text).unwrap();
    assert_eq!(assessment.report().findings.len(), 1);
    let finding = &assessment.report().findings[0];
    assert_eq!(finding.rule, "OBI-02");
    assert_eq!(finding.code, "schema-mismatch");
    assert_eq!(
        finding.message,
        "object is missing required field \"operations\""
    );
    let at = finding.location.as_ref().unwrap();
    assert_eq!(at.pointer.as_deref(), Some(""));
    assert_eq!((at.byte_offset, at.line, at.byte_column), (3, 2, 2));

    let nested = assess_document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"content":"private-payload"}}}"#,
    )
    .unwrap();
    let finding = nested
        .report()
        .findings
        .iter()
        .find(|f| f.code == "schema-mismatch")
        .unwrap();
    assert_eq!(finding.message, "object is missing required field \"kind\"");
    assert_eq!(
        finding.location.as_ref().unwrap().pointer.as_deref(),
        Some("/sources/s")
    );
    assert!(!finding.message.contains("private-payload"));
}

#[test]
fn complex_failure_stays_generic_while_explicit_type_union_is_explained() {
    let assessment =
        assess_document(r#"{"openbindings":"0.2.0","operations":{"lookup":{"input":null}}}"#)
            .unwrap();
    let findings = &assessment.report().findings;
    let normative = findings.iter().find(|f| f.rule == "OBI-02").unwrap();
    assert_eq!(normative.code, "schema-mismatch");
    assert_eq!(
        normative.message,
        "value violates the fixed normative schema"
    );
    assert_eq!(
        normative.location.as_ref().unwrap().pointer.as_deref(),
        Some("/operations/lookup/input")
    );
    let meta = findings.iter().find(|f| f.rule == "OBI-10").unwrap();
    assert_eq!(
        meta.message,
        "expected one of these JSON types: boolean, object"
    );
    assert_eq!(meta.location, normative.location);
    assert!(findings.iter().all(|f| f.message.len() <= 512));
    assert!(
        findings
            .iter()
            .all(|f| !f.message.contains("internal-meta"))
    );
}

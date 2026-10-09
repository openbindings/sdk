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

const UNEXPECTED: &str = "this member is not permitted here; extension member names begin with x-";

#[test]
fn unexpected_fields_locate_original_key_tokens_in_source_order() {
    let text = "{\r\n\"openbindings\":\"0.2.0\",\"name\":\"café 😀\",\"operations\":{},\n\"z/~\\n😀\":{\"secret\":42},\"a\":false}";
    let assessment = assess_document(text).unwrap();
    let report = assessment.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    assert!(!report.findings_truncated);
    assert_eq!(report.findings.len(), 2);
    for (finding, (token, pointer)) in report
        .findings
        .iter()
        .zip([("\"z/~\\n😀\"", "/z~1~0\n😀"), ("\"a\"", "/a")])
    {
        assert_eq!(finding.rule, "OBI-02");
        assert_eq!(finding.code, "schema-mismatch");
        assert_eq!(finding.message, UNEXPECTED);
        let at = finding.location.as_ref().unwrap();
        let offset = text.find(token).unwrap();
        let prefix = &text[..offset];
        assert_eq!(at.byte_offset, offset);
        assert_eq!(at.pointer.as_deref(), Some(pointer));
        assert_eq!(at.line, prefix.bytes().filter(|&b| b == b'\n').count() + 1);
        assert_eq!(
            at.byte_column,
            prefix.rfind('\n').map_or(offset + 1, |n| offset - n)
        );
        assert!(!finding.message.contains("secret"));
        assert!(!finding.message.contains("😀"));
    }
}

#[test]
fn all_closed_objects_are_diagnosed_without_entering_opaque_or_schema_values() {
    let text = r#"{"openbindings":"0.2.0","operations":{"run":{"badOperation":0,"input":{"customKeyword":{"inputs":1}},"examples":{"sample":{"badExample":0,"input":{"inputs":1},"x-extra":{"inputs":1}}},"x-extra":{"inputs":1}}},"dependencies":{"d":{"operation":"run","badDependency":0}},"sources":{"s":{"kind":"custom","badSource":0,"content":{"inputs":1}}},"bindings":{"b":{"operation":"run","source":"s","badBinding":0,"content":{"inputs":1}}},"badRoot":0,"x-extra":{"inputs":1}}"#;
    let assessment = assess_document(text).unwrap();
    let mut paths: Vec<_> = assessment
        .report()
        .findings
        .iter()
        .filter(|f| f.message == UNEXPECTED)
        .map(|f| f.location.as_ref().unwrap().pointer.as_deref().unwrap())
        .collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "/badRoot",
            "/bindings/b/badBinding",
            "/dependencies/d/badDependency",
            "/operations/run/badOperation",
            "/operations/run/examples/sample/badExample",
            "/sources/s/badSource"
        ]
    );
    assert_eq!(assessment.report().findings.len(), 6);
    let mut corrected = text.to_owned();
    for name in [
        "badRoot",
        "badBinding",
        "badDependency",
        "badOperation",
        "badExample",
        "badSource",
    ] {
        corrected = corrected.replace(&format!("\"{name}\""), &format!("\"x-{name}\""));
    }
    assert_eq!(
        assess_document(corrected).unwrap().report().conclusion,
        Conformance::Conformant
    );
}

#[test]
fn name_guidance_describes_the_existing_grammar_for_every_name_position() {
    let text = r#"{"openbindings":"0.2.0","operations":{"/private":{"aliases":[".private",false],"examples":{"-private":{}}}}}"#;
    let assessment = assess_document(text).unwrap();
    let names: Vec<_> = assessment
        .report()
        .findings
        .iter()
        .filter(|f| f.code == "name-grammar")
        .collect();
    assert_eq!(names.len(), 4);
    for finding in names {
        assert!(finding.message.contains("nonempty ASCII string"));
        assert!(
            finding
                .message
                .contains("start with a letter, digit or underscore")
        );
        assert!(finding.message.contains("dots or hyphens"));
        assert!(!finding.message.contains("private"));
    }
    for name in ["1", "_", "A", "a-b.c_1"] {
        assert!(valid_name(name));
    }
    for name in ["", ".a", "-a", "é", "a/b"] {
        assert!(!valid_name(name));
    }
}

#[test]
fn expansion_saturation_preserves_all_rule_evidence_and_truthful_truncation() {
    for count in [4096, 4097] {
        let fields = (0..count)
            .map(|i| format!(",\"bad{i}\":0"))
            .collect::<String>();
        let text = format!("{{\"openbindings\":\"0.2.0\",\"operations\":{{}}{fields}}}");
        let assessment = assess_document(&text).unwrap();
        assert_eq!(assessment.report().findings.len(), 4096);
        assert_eq!(assessment.report().findings_truncated, count > 4096);
        assert_eq!(assessment.report().evidence["OBI-02"], Evidence::Violated);
        assert_eq!(assessment.report().evidence.len(), 13);
        assert!(
            assessment
                .report()
                .findings
                .iter()
                .all(|f| f.message == UNEXPECTED)
        );
        let bad_schema = text.replace(
            "\"operations\":{}",
            "\"operations\":{},\"schemas\":{\"s\":{\"type\":7}}",
        );
        let assessment = assess_document(bad_schema).unwrap();
        assert_eq!(assessment.report().findings.len(), 4096);
        assert!(assessment.report().findings_truncated);
        assert_eq!(assessment.report().evidence["OBI-10"], Evidence::Violated);
        if count == 4096 {
            let deep = format!("{}true{}", "{\"not\":".repeat(300), "}".repeat(300));
            let source = text.replace(
                "\"operations\":{}",
                &format!("\"operations\":{{}},\"schemas\":{{\"s\":{deep}}}"),
            );
            let assessment = assess_document(source).unwrap();
            assert_eq!(assessment.report().findings.len(), 4096);
            assert!(assessment.report().findings_truncated);
            assert_eq!(
                assessment.report().evidence["OBI-10"],
                Evidence::Inconclusive
            );
        }
    }
}

#[test]
fn expansion_pointer_budget_prevents_large_ancestor_amplification() {
    let name = "a".repeat(2048);
    let fields = (0..4096)
        .map(|i| format!("\"bad{i}\":0"))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(
        "{{\"openbindings\":\"0.2.0\",\"operations\":{{\"{name}\":{{{fields}}},\"y\":{{\"bad\":0}},\"z\":{{\"description\":42}}}}}}"
    );
    let assessment = assess_document(text).unwrap();
    let report = assessment.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    assert!(report.findings_truncated);
    assert!(report.findings.len() < 4096);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message == "expected JSON type: string"
                && f.location.as_ref().unwrap().pointer.as_deref()
                    == Some("/operations/z/description"))
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.location.as_ref().unwrap().pointer.as_deref() == Some("/operations/y/bad"))
    );
    let bytes: usize = report
        .findings
        .iter()
        .filter(|f| f.message == UNEXPECTED)
        .map(|f| f.location.as_ref().unwrap().pointer.as_ref().unwrap().len())
        .sum();
    assert!(bytes <= 8 * 1024 * 1024);
    assert!(bytes > 8 * 1024 * 1024 - 2100);
    assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
}

#[test]
fn ambiguous_json_refusals_are_not_replaced_by_field_guidance() {
    let duplicates =
        assess_document(r#"{"openbindings":"0.2.0","operations":{},"bad":1,"bad":2}"#).unwrap();
    assert!(
        duplicates
            .report()
            .findings
            .iter()
            .all(|f| f.code == "duplicate-member")
    );
    assert_eq!(
        duplicates.report().evidence["OBI-02"],
        Evidence::NotApplicable
    );
    let unpaired =
        assess_document(r#"{"openbindings":"0.2.0","operations":{},"\ud800":0}"#).unwrap();
    assert_eq!(unpaired.report().evidence["OBI-02"], Evidence::Inconclusive);
    assert!(
        unpaired
            .report()
            .findings
            .iter()
            .all(|f| f.message != UNEXPECTED)
    );
}

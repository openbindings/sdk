use openbindings::*;
const MESSAGE: &str = "value violates the fixed normative schema";

#[test]
fn fixed_message_preserves_rule_location_and_rejected_value_privacy() {
    let text = "{\n  \"openbindings\":\"0.2.0\",\n  \"name\":\"café\",\n  \"operations\":{\"lookup\":{\"description\":{\"private\":\"rejected-secret\"}}}\n}";
    let validation = validate_document(text).unwrap();
    let report = validation.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
    assert_eq!(report.findings.len(), 1);
    assert!(!report.findings_truncated);
    let finding = &report.findings[0];
    assert_eq!(finding.rule, "OBI-02");
    assert_eq!(finding.code, "schema-mismatch");
    assert_eq!(finding.status, Evidence::Violated);
    assert_eq!(finding.message, MESSAGE);
    let at = finding.location.as_ref().unwrap();
    assert_eq!(
        at.pointer.as_ref().and_then(openbindings::Pointer::as_str),
        Some("/operations/lookup/description")
    );
    assert_eq!(at.byte_offset, text.find("{\"private\"").unwrap());
    assert_eq!(at.line, 4);
    assert_eq!(at.byte_column, 41);
    assert!(!finding.message.contains("private"));
    assert!(!finding.message.contains("rejected-secret"));
    let corrected = text.replace("{\"private\":\"rejected-secret\"}", "\"Find an item\"");
    let corrected = validate_document(corrected).unwrap();
    assert_eq!(corrected.report().conclusion, Conformance::Conformant);
    assert!(corrected.report().findings.is_empty());
}

#[test]
fn required_failure_points_at_the_original_containing_object() {
    let text = " \n {\"openbindings\":\"0.2.0\"}";
    let validation = validate_document(text).unwrap();
    assert_eq!(validation.report().findings.len(), 1);
    let finding = &validation.report().findings[0];
    assert_eq!(finding.rule, "OBI-02");
    assert_eq!(finding.code, "schema-mismatch");
    assert_eq!(finding.message, MESSAGE);
    let at = finding.location.as_ref().unwrap();
    assert_eq!(
        at.pointer.as_ref().and_then(openbindings::Pointer::as_str),
        Some("")
    );
    assert_eq!((at.byte_offset, at.line, at.byte_column), (3, 2, 2));

    let nested = validate_document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"content":"private-payload"}}}"#,
    )
    .unwrap();
    let finding = nested
        .report()
        .findings
        .iter()
        .find(|f| f.code == "schema-mismatch")
        .unwrap();
    assert_eq!(finding.message, MESSAGE);
    assert_eq!(
        finding
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("/sources/s")
    );
    assert!(!finding.message.contains("private-payload"));
}

#[test]
fn document_and_meta_failures_use_fixed_messages() {
    let validation =
        validate_document(r#"{"openbindings":"0.2.0","operations":{"lookup":{"input":null}}}"#)
            .unwrap();
    let findings = &validation.report().findings;
    let normative = findings.iter().find(|f| f.rule == "OBI-02").unwrap();
    assert_eq!(normative.code, "schema-mismatch");
    assert_eq!(normative.message, MESSAGE);
    assert_eq!(
        normative
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("/operations/lookup/input")
    );
    let meta = findings.iter().find(|f| f.rule == "OBI-10").unwrap();
    assert_eq!(meta.message, MESSAGE);
    assert_eq!(meta.location, normative.location);
    assert!(findings.iter().all(|f| f.message.len() <= 512));
    assert!(
        findings
            .iter()
            .all(|f| !f.message.contains("internal-meta"))
    );
}

const UNEXPECTED: &str = MESSAGE;

#[test]
fn unexpected_fields_locate_original_key_tokens_in_source_order() {
    let text = "{\r\n\"openbindings\":\"0.2.0\",\"name\":\"café 😀\",\"operations\":{},\n\"z/~\\n😀\":{\"secret\":42},\"a\":false}";
    let validation = validate_document(text).unwrap();
    let report = validation.report();
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
        assert_eq!(
            at.pointer.as_ref().and_then(openbindings::Pointer::as_str),
            Some(pointer)
        );
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
    let validation = validate_document(text).unwrap();
    let mut paths: Vec<_> = validation
        .report()
        .findings
        .iter()
        .filter(|f| f.rule == "OBI-02")
        .map(|f| {
            f.location
                .as_ref()
                .unwrap()
                .pointer
                .as_ref()
                .unwrap()
                .to_string()
        })
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
        validate_document(corrected).unwrap().report().conclusion,
        Conformance::Conformant
    );
}

#[test]
fn name_guidance_describes_the_existing_grammar_for_every_name_position() {
    let text = r#"{"openbindings":"0.2.0","operations":{"/private":{"aliases":[".private",false],"examples":{"-private":{}}}}}"#;
    let validation = validate_document(text).unwrap();
    let names: Vec<_> = validation
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
    // CA-5: the public name predicate keeps the specification grammar.
    for name in ["1", "_", "A", "a-b.c_1"] {
        assert!(is_valid_name(name));
    }
    for name in ["", ".a", "-a", "é", "a/b"] {
        assert!(!is_valid_name(name));
    }
}

#[test]
fn localized_findings_preserve_all_rule_evidence_under_the_findings_cap() {
    for count in [4096, 4097] {
        let schemas = (0..count)
            .map(|i| format!("\"s{i}\":{{\"minLength\":-1}}"))
            .collect::<Vec<_>>()
            .join(",");
        let text =
            format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{{schemas}}}}}"#);
        let validation = validate_document(text).unwrap();
        let report = validation.report();
        assert_eq!(report.findings.len(), 4096);
        assert_eq!(report.findings_truncated, count > 4096);
        assert_eq!(report.evidence["OBI-02"], Evidence::Met);
        assert_eq!(report.evidence["OBI-10"], Evidence::Violated);
        assert_eq!(report.evidence.len(), 13);
        assert!(report.findings.iter().all(|f| f.message == MESSAGE));
    }
}

#[test]
fn findings_respect_aggregate_source_pointer_allowance() {
    let name = "a".repeat(2048);
    let fields = (0..4096)
        .map(|i| format!("\"bad{i}\":0"))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"{name}":{{{fields}}}}}}}"#);
    let validation = validate_document(text).unwrap();
    let report = validation.report();
    assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
    assert!(report.findings.len() > 1);
    assert!(report.findings_truncated);
    let bytes: usize = report
        .findings
        .iter()
        .map(|f| {
            f.location
                .as_ref()
                .unwrap()
                .pointer
                .as_ref()
                .unwrap()
                .as_str()
                .unwrap()
                .len()
        })
        .sum();
    assert!(bytes <= 8 * 1024 * 1024);
    assert_eq!(report.findings[0].message, UNEXPECTED);
    assert_eq!(
        report.findings[0]
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some(format!("/operations/{name}/bad0").as_str())
    );
}

#[test]
fn ambiguous_json_refusals_are_not_replaced_by_field_guidance() {
    let duplicates =
        validate_document(r#"{"openbindings":"0.2.0","operations":{},"bad":1,"bad":2}"#).unwrap();
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
        validate_document(r#"{"openbindings":"0.2.0","operations":{},"\ud800":0}"#).unwrap();
    assert_eq!(unpaired.report().evidence["OBI-01"], Evidence::Violated);
    assert_eq!(
        unpaired.report().evidence["OBI-02"],
        Evidence::NotApplicable
    );
    assert_eq!(unpaired.report().findings[0].code, "unpaired-surrogate");
    assert_eq!(
        unpaired.report().findings[0].message,
        "member name or string value holds an unpaired surrogate escape"
    );
    assert_eq!(
        unpaired.report().findings[0]
            .location
            .as_ref()
            .unwrap()
            .byte_offset,
        40
    );
}

#[test]
fn fixed_messages_are_value_free_and_property_names_locate_original_tokens() {
    // Findings use validity and original tokens, never library error payloads.
    let text = r#"{"openbindings":"0.2.0","operations":{"-secret":{},".private":{},"\ud83d\ude00":{}},"dependencies":{"d":{"operation":"-secret","kinds":[]}},"schemas":{"s":{"$anchor":"1private","minLength":-1}}}"#;
    let validation = validate_document(text).unwrap();
    for (fragment, pointer) in [
        (
            r#""dependencies":{"d":{"operation":"op","kinds":[]}}"#,
            "/dependencies/d/kinds",
        ),
        (r#""schemas":{"s":{"$anchor":"1private"}}"#, "/schemas/s"),
        (r#""schemas":{"s":{"minLength":-1}}"#, "/schemas/s"),
    ] {
        let source = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},{fragment}}}"#);
        let check = validate_document(source).unwrap();
        assert!(
            check
                .report()
                .findings
                .iter()
                .any(|f| f.code == "schema-mismatch"
                    && f.location
                        .as_ref()
                        .unwrap()
                        .pointer
                        .as_ref()
                        .and_then(openbindings::Pointer::as_str)
                        == Some(pointer)
                    && f.message == MESSAGE),
            "{pointer}"
        );
    }
    for token in [r#""-secret""#, r#"".private""#, r#""\ud83d\ude00""#] {
        assert!(
            validation
                .report()
                .findings
                .iter()
                .any(|f| f.rule == "OBI-02"
                    && f.message == MESSAGE
                    && f.location.as_ref().unwrap().byte_offset == text.find(token).unwrap()),
            "{token}"
        );
    }
    assert!(
        validation
            .report()
            .findings
            .iter()
            .all(|f| !f.message.contains("secret") && !f.message.contains("private"))
    );
}

#[test]
fn independent_container_findings_keep_children_and_truthful_omissions() {
    for (text, pointers) in [
        (r#"{"openbindings":"0.2.0","name":42}"#, vec!["/name", ""]),
        (
            r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"description":42}}}"#,
            vec!["/sources/s/description", "/sources/s"],
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"description":42}}}"#,
            vec!["/bindings/b/description", "/bindings/b"],
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{"op":{"aliases":[false,false]}}}"#,
            vec![
                "/operations/op/aliases/0",
                "/operations/op/aliases/1",
                "/operations/op/aliases",
            ],
        ),
    ] {
        let validation = validate_document(text).unwrap();
        let report = validation.report();
        assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
        assert!(!report.findings_truncated);
        let actual: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.rule == "OBI-02")
            .map(|f| {
                assert_eq!(f.message, MESSAGE);
                f.location
                    .as_ref()
                    .unwrap()
                    .pointer
                    .as_ref()
                    .and_then(openbindings::Pointer::as_str)
                    .unwrap()
            })
            .collect();
        assert_eq!(actual, pointers);
    }
}

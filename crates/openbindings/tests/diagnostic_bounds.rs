use openbindings::{Conformance, Evidence, assess_document};

#[test]
fn duplicate_cap_preserves_order_coordinates_and_truthful_truncation() {
    for nested in [false, true] {
        for count in [4095, 4096, 4097] {
            let source = if nested {
                format!(
                    "{{\"openbindings\":\"0.2.0\",\"operations\":{{}},\"x-pad\":\"{}\",\n\"x-duplicates\":{{\"k\":0{}}}}}",
                    "p".repeat(128 * 1024),
                    ",\"k\":0".repeat(count)
                )
            } else {
                format!("{{\"k\":0{}}}", ",\"k\":0".repeat(count))
            };
            let report = assess_document(&source).unwrap();
            let report = report.report();
            assert_eq!(report.conclusion, Conformance::NonConformant);
            assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
            assert_eq!(report.evidence["OBI-02"], Evidence::NotApplicable);
            assert_eq!(report.findings.len(), count.min(4096));
            assert_eq!(report.findings_truncated, count > 4096);
            let offsets = source
                .match_indices("\"k\":0")
                .skip(1)
                .map(|(offset, _)| offset);
            for (finding, offset) in report.findings.iter().zip(offsets) {
                let at = finding.location.as_ref().unwrap();
                assert_eq!(finding.code, "duplicate-member");
                assert_eq!(at.byte_offset, offset);
                assert_eq!(at.line, if nested { 2 } else { 1 });
                assert_eq!(
                    at.byte_column,
                    source[..offset]
                        .rfind('\n')
                        .map_or(offset + 1, |newline| offset - newline)
                );
                assert_eq!(
                    at.pointer.as_deref(),
                    Some(if nested { "/x-duplicates/k" } else { "/k" })
                );
            }
        }
    }
}

#[test]
fn nonfixed_findings_keep_coordinates_and_evidence_after_the_cap() {
    let aliases = (0..4097)
        .map(|i| format!("\"op{i}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let operations = (0..4097)
        .map(|i| format!("\"op{i}\":{{}}",))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "{{\"openbindings\":\"0.2.0\",\"x-pad\":\"{}\",\n\"operations\":{{{operations},\"run\":{{\"aliases\":[{}]}}}},\"dependencies\":{{\"d\":{{\"operation\":\"absent\"}}}}}}",
        "p".repeat(128 * 1024),
        aliases
    );
    let assessment = assess_document(&source).unwrap();
    let report = assessment.report();
    assert_eq!(report.findings.len(), 4096);
    assert!(report.findings_truncated);
    assert_eq!(report.evidence["OBI-05"], Evidence::Violated);
    assert_eq!(report.evidence["OBI-08"], Evidence::Violated);
    for (index, finding) in report.findings.iter().enumerate() {
        assert_eq!(finding.code, "duplicate-operation-name");
        let at = finding.location.as_ref().unwrap();
        assert_eq!(
            at.pointer.as_deref(),
            Some(format!("/operations/run/aliases/{index}").as_str())
        );
        assert_eq!(
            at.byte_offset,
            source.rfind(&format!("\"op{index}\"")).unwrap()
        );
        assert_eq!(at.line, index + 2);
    }
}

#[test]
fn aggregate_pointer_budget_applies_to_duplicates_and_keeps_rule_evidence() {
    let name = "x-".to_owned() + &"a".repeat(4096);
    let source = format!(
        "{{\"openbindings\":\"0.2.0\",\"operations\":{{}},\"{name}\":{{\"k\":0{}}}}}",
        ",\"k\":0".repeat(4096)
    );
    let assessment = assess_document(&source).unwrap();
    let report = assessment.report();
    assert!(report.findings_truncated);
    assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
    let bytes: usize = report
        .findings
        .iter()
        .map(|f| f.location.as_ref().unwrap().pointer.as_ref().unwrap().len())
        .sum();
    assert!(bytes <= 8 * 1024 * 1024);
    assert!(bytes > 8 * 1024 * 1024 - name.len() - 3);
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.location.as_ref().unwrap().pointer.as_deref()
                == Some(format!("/{name}/k").as_str()))
    );
}

#[test]
fn duplicate_under_unpaired_ancestor_keeps_original_byte_coordinates() {
    let source = r#"{"openbindings":"0.2.0","operations":{},"x":{"\ud800":{"k":0,"k":1}}}"#;
    let assessment = assess_document(source).unwrap();
    let report = assessment.report();
    assert_eq!(report.findings.len(), 1);
    assert!(!report.findings_truncated);
    let at = report.findings[0].location.as_ref().unwrap();
    assert_eq!(at.pointer, None);
    assert_eq!(at.byte_offset, source.rfind("\"k\"").unwrap());
}

#[test]
fn deep_duplicate_pointer_retains_its_original_coordinates() {
    let depth = 512;
    let source = format!(
        "{}{{\"k\":0,\"k\":1}}{}",
        "{\"x\":".repeat(depth),
        "}".repeat(depth)
    );
    let assessment = assess_document(&source).unwrap();
    let report = assessment.report();
    assert_eq!(report.findings.len(), 1);
    assert!(!report.findings_truncated);
    let at = report.findings[0].location.as_ref().unwrap();
    assert_eq!(
        at.pointer.as_deref(),
        Some(("/x".repeat(depth) + "/k").as_str())
    );
    assert_eq!(at.byte_offset, source.rfind("\"k\"").unwrap());
}

#[test]
fn identical_invalid_schemas_at_distinct_locations_are_both_reported() {
    let source = r#"{"openbindings":"0.2.0","operations":{"a":{"input":{"type":42}},"b":{"input":{"type":42}}}}"#;
    let assessment = assess_document(source).unwrap();
    let report = assessment.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    let schema: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule == "OBI-10" && f.code == "schema-mismatch")
        .collect();
    let pointers: std::collections::BTreeSet<_> = schema
        .iter()
        .map(|f| f.location.as_ref().unwrap().pointer.as_deref().unwrap())
        .collect();
    assert!(pointers.contains("/operations/a/input/type"), "{schema:?}");
    assert!(pointers.contains("/operations/b/input/type"), "{schema:?}");
    let unique: std::collections::HashSet<_> = schema
        .iter()
        .map(|f| {
            (
                f.rule,
                f.code,
                f.message.as_str(),
                f.location.as_ref().unwrap().byte_offset,
            )
        })
        .collect();
    assert_eq!(unique.len(), schema.len());
    assert!(!report.findings_truncated);
}

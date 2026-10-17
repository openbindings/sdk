use openbindings::*;

// `pointer` is the expected pointer as Display writes it; `exact` says whether
// it is Unicode text, or passes through a name with an unpaired unit.
fn assert_unpaired(text: &str, token: &str, pointer: &str, exact: bool) {
    // All public document entry paths apply the same boundary without changing
    // exact parsing or the retained draft bytes.
    let parsed = Document::parse(text).unwrap();
    let from_value = Document::from_json(JsonValue::parse(text).unwrap());
    for validation in [
        validate_document(text).unwrap(),
        parsed.validate().unwrap(),
        from_value.validate().unwrap(),
    ] {
        let report = validation.report();
        assert_eq!(report.conclusion, Conformance::NonConformant);
        assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
        for rule in &DOCUMENT_RULES[1..] {
            assert_eq!(report.evidence[rule], Evidence::NotApplicable, "{rule}");
        }
        assert!(validation.conformant().is_none());
        assert_eq!(validation.document().unwrap().to_bytes(), text.as_bytes());
        assert_eq!(report.findings.len(), 1);
        assert!(!report.findings_truncated);
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "OBI-01");
        assert_eq!(finding.status, Evidence::Violated);
        assert_eq!(finding.code, "unpaired-surrogate");
        assert!(format!("{finding:?}").contains("unpaired-surrogate"));
        let at = finding.location.as_ref().unwrap();
        let offset = text.find(token).unwrap();
        let at_pointer = at.pointer.as_ref().unwrap();
        assert_eq!(at_pointer.to_string(), pointer);
        assert_eq!(at_pointer.as_str().is_some(), exact);
        assert_eq!(at.byte_offset, offset);
        assert_eq!(
            at.line,
            1 + text[..offset].bytes().filter(|b| *b == b'\n').count()
        );
        assert_eq!(
            at.byte_column,
            offset - text[..offset].rfind('\n').map_or(0, |i| i + 1) + 1
        );
    }
}

#[test]
fn unpaired_high_surrogate_value_violates_only_json_conformance() {
    // OBI-01's high-surrogate fixture and scenario CONFORMANCE-06 carry this text.
    assert_unpaired(
        r#"{"openbindings":"0.2.0","description":"\ud800","operations":{}}"#,
        r#""\ud800""#,
        "/description",
        true,
    );
}

#[test]
fn unpaired_low_surrogate_member_in_schema_locates_the_name_token() {
    assert_unpaired(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"type":"object","properties":{"\udc00":{}}}}}}"#,
        r#""\udc00""#,
        "/operations/op/input/properties/\\udc00",
        false,
    );
}

#[test]
fn reversed_surrogates_are_not_a_pair() {
    assert_unpaired(
        r#"{"openbindings":"0.2.0","description":"\ude00\ud83d","operations":{}}"#,
        r#""\ude00\ud83d""#,
        "/description",
        true,
    );
}

#[test]
fn escaped_pairs_and_noncharacters_remain_conformant() {
    for token in [r#""\ud83d\ude00""#, r#""\uffff""#] {
        let text = format!(r#"{{"openbindings":"0.2.0","description":{token},"operations":{{}}}}"#);
        let validation = validate_document(&text).unwrap();
        assert!(
            validation
                .report()
                .evidence
                .values()
                .all(|e| *e == Evidence::Met)
        );
        assert!(validation.report().findings.is_empty());
        let proof = validation.conformant().unwrap();
        assert_eq!(proof.to_bytes(), text.as_bytes());
        let at = proof.value().get("description").unwrap().location();
        assert_eq!(
            at.pointer.as_ref().and_then(openbindings::Pointer::as_str),
            Some("/description")
        );
        assert_eq!(at.byte_offset, text.find(token).unwrap());
    }
}

#[test]
fn surrogate_exclusion_follows_version_selection_and_precedes_later_rules() {
    for version in ["0.3.0", "0.2.0-rc.1"] {
        let text = format!(r#"{{"openbindings":"{version}","description":"\ud800"}}"#);
        assert_eq!(validate_document(text).unwrap_err().declared, version);
    }
    for declaration in ["", r#""openbindings":7,"#, r#""openbindings":"bad","#] {
        let text = format!("{{\n{declaration}\"description\":\"\\uD800\"\n}}");
        assert_unpaired(&text, r#""\uD800""#, "/description", true);
    }
}

#[test]
fn surrogate_findings_obey_the_document_finding_cap() {
    let values = vec![r#""\ud800""#; 4097].join(",");
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{}},"x-data":[{values}]}}"#);
    let validation = validate_document(&text).unwrap();
    let report = validation.report();
    assert_eq!(report.conclusion, Conformance::NonConformant);
    assert_eq!(report.findings.len(), 4096);
    assert!(report.findings_truncated);
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.rule == "OBI-01" && f.code == "unpaired-surrogate")
    );
    assert_eq!(
        report.findings[4095]
            .location
            .as_ref()
            .unwrap()
            .pointer
            .as_ref()
            .unwrap(),
        "/x-data/4095"
    );
    for rule in &DOCUMENT_RULES[1..] {
        assert_eq!(report.evidence[rule], Evidence::NotApplicable);
    }
}

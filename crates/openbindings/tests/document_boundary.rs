use openbindings::*;

#[test]
fn as_bytes_include_whitespace_and_rebased_children_have_local_locations() {
    let text = " \n {\"openbindings\":\"0.2.0\",\"operations\":{}}\t\n";
    let doc = Document::parse(text).unwrap();
    assert_eq!(doc.as_bytes(), text.as_bytes());
    assert_eq!(
        doc.validate().unwrap().conformant().unwrap().as_bytes(),
        text.as_bytes()
    );
    let parent =
        JsonValue::parse(r#"{"child":{"openbindings":"0.2.0","operations":{},"unknown":true}}"#)
            .unwrap();
    let child = parent.get("child").unwrap().to_owned();
    let expected = child.text().to_owned();
    let doc = Document::from_json(child);
    drop(parent);
    assert_eq!(doc.as_bytes(), expected.as_bytes());
    assert_eq!(doc.value().location().byte_offset, 0);
    assert_eq!(
        doc.value()
            .location()
            .pointer
            .as_ref()
            .and_then(openbindings::Pointer::as_str),
        Some("")
    );
}

fn validation(text: &str) -> DocumentValidation {
    validate_document(text).unwrap()
}
#[test]
fn states_do_not_confuse_empty_findings_with_conformance() {
    let parsed =
        Document::parse(r#"{"openbindings":"0.2.0","operations":{},"unknown":true}"#).unwrap();
    let checked = parsed.validate().unwrap();
    assert_eq!(checked.report().conclusion, Conformance::NonConformant);
    assert!(checked.conformant().is_none());
    assert_eq!(checked.report().evidence.len(), DOCUMENT_RULES.len());
    let invalid = validate_document(b"{broken").unwrap();
    assert!(invalid.document().is_none());
    assert_eq!(invalid.report().evidence["OBI-01"], Evidence::Violated);
    for rule in &DOCUMENT_RULES[1..] {
        assert_eq!(invalid.report().evidence[rule], Evidence::NotApplicable);
    }
}
#[test]
fn version_priority_requires_one_complete_declaration() {
    assert!(validate_document(r#"{"openbindings":"1.0.0","x":1,"x":2}"#).is_err());
    assert!(validate_document(r#"{"openbindings":"1.0.0","openbindings":"1.0.0"}"#).is_ok());
    assert!(validate_document(r#"{"openbindings":"1.0.0","broken":}"#).is_ok());
    let deep = format!(
        r#"{{"openbindings":"1.0.0","x":{}0{}}}"#,
        "[".repeat(10001),
        "]".repeat(10001)
    );
    let report = validate_document(&deep).unwrap();
    assert_eq!(report.report().conclusion, Conformance::Undetermined);
    assert!(
        report
            .report()
            .findings
            .iter()
            .all(|finding| finding.code == "depth-limit")
    );
}
#[test]
fn default_depth_limit_is_undetermined_and_an_explicit_limit_admits_deeper_documents() {
    let depth = JsonLimits::default().max_depth;
    let text = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-deep":{}0{}}}"#,
        "[".repeat(depth),
        "]".repeat(depth)
    );
    for checked in [
        validate_document(&text).unwrap(),
        validate_document_owned(text.as_bytes().to_vec()).unwrap(),
    ] {
        assert_eq!(checked.report().conclusion, Conformance::Undetermined);
        assert!(!checked.report().findings.is_empty());
        assert!(
            checked
                .report()
                .findings
                .iter()
                .all(|finding| finding.code == "depth-limit")
        );
        assert!(checked.conformant().is_none());
    }
    let checked = validate_document_with_limits(
        &text,
        JsonLimits {
            max_depth: depth + 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    assert!(checked.conformant().is_some());
}

#[test]
fn lone_surrogates_remain_exact_but_violate_document_conformance() {
    let text = r#"{"openbindings":"0.2.0","operations":{},"x-data":"\ud800"}"#;
    let checked = validation(text);
    assert_eq!(checked.report().conclusion, Conformance::NonConformant);
    assert_eq!(checked.report().evidence["OBI-01"], Evidence::Violated);
    assert_eq!(checked.document().unwrap().as_bytes(), text.as_bytes());
    let duplicate = validation(r#"{"openbindings":"0.2.0","operations":{},"\ud800":1,"\ud800":2}"#);
    assert_eq!(duplicate.report().evidence["OBI-01"], Evidence::Violated);
}
#[test]
fn opaque_data_is_not_a_schema_but_document_pointers_cross_resource_boundaries() {
    let text = r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/schemas/root/properties/x"}}},"schemas":{"root":{"$id":7,"properties":{"x":true}}},"x-data":{"$ref":"relative"}}"##;
    let r = validation(text);
    assert_eq!(r.report().evidence["OBI-11"], Evidence::Violated);
    assert_eq!(r.report().evidence["OBI-12"], Evidence::Met);
    let text = r##"{"openbindings":"0.2.0","operations":{},"schemas":{"s":{"properties":{"$dynamicRef":{"type":"string"}},"x-opaque":{"$schema":"unknown","$id":"relative"}}}}"##;
    assert_eq!(
        validation(text).report().conclusion,
        Conformance::Conformant
    );
}
#[test]
fn meta_checks_find_local_errors_before_a_depth_limit() {
    let mut schema = r#"{"type":7}"#.to_owned();
    for _ in 0..257 {
        schema = format!(r#"{{"allOf":[{schema}]}}"#);
    }
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let r = validation(&text);
    assert_eq!(r.report().evidence["OBI-10"], Evidence::Violated);
    let text = text.replacen("\"input\":{", "\"input\":{\"type\":7,", 1);
    let r = validation(&text);
    assert_eq!(r.report().evidence["OBI-10"], Evidence::Violated);
}
#[test]
fn immutable_snapshot_is_independent_of_authoring_mutation() {
    let mut builder = Document::new();
    builder = builder
        .set("/operations/first", &JsonValue::parse("{}").unwrap())
        .unwrap();
    let snapshot = builder.clone();
    builder = builder
        .set("/operations", &JsonValue::parse("{}").unwrap())
        .unwrap();
    assert!(
        builder
            .validate()
            .unwrap()
            .conformant()
            .unwrap()
            .operations()
            .is_empty()
    );
    let valid = snapshot.validate().unwrap().conformant().unwrap();
    assert!(valid.resolve_operation("first").is_some());
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Document>();
    send_sync::<ConformantDocument>();
    send_sync::<JsonValue>();
}

#[test]
fn indexed_reference_checks_keep_primary_key_and_diagnostic_semantics() {
    let checked = validation(
        r#"{
      "openbindings":"0.2.0",
      "operations":{"real":{"aliases":["alias"]},"present":null},
      "sources":{"src":null},
      "bindings":{
        "exists":{"operation":"present","source":"src"},
        "aliasOnly":{"operation":"alias","source":"src"},
        "missing":{"operation":"absent","source":"absent"}
      },
      "dependencies":{"dep":{"operation":"alias"}}
    }"#,
    );
    let missing: Vec<_> = checked
        .report()
        .findings
        .iter()
        .filter(|f| matches!(f.code, "unknown-operation" | "unknown-source"))
        .map(|f| {
            (
                f.rule,
                f.code,
                f.location
                    .as_ref()
                    .unwrap()
                    .pointer
                    .as_ref()
                    .and_then(openbindings::Pointer::as_str)
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(
        missing,
        vec![
            (
                "OBI-06",
                "unknown-operation",
                "/bindings/aliasOnly/operation"
            ),
            ("OBI-06", "unknown-operation", "/bindings/missing/operation"),
            ("OBI-07", "unknown-source", "/bindings/missing/source"),
            ("OBI-08", "unknown-operation", "/dependencies/dep/operation"),
        ]
    );
    assert_eq!(checked.report().evidence["OBI-02"], Evidence::Violated);
    // An existing key counts even if the schema separately rejects its value;
    // aliases never count as a primary key. Finding order/location is unchanged.
}

#[test]
fn parse_failures_locate_the_read_prefix_in_utf8_bytes() {
    for (bytes, limits, expected) in [
        (
            b"{\n \"\xc3\xa9\" 0}".as_slice(),
            JsonLimits::default(),
            (8, 2, 7),
        ),
        (b"[\n0,".as_slice(), JsonLimits::default(), (4, 2, 3)),
        (b"[\n\xff]".as_slice(), JsonLimits::default(), (2, 2, 1)),
        (
            b"[\n[0]]".as_slice(),
            JsonLimits {
                max_depth: 1,
                ..JsonLimits::default()
            },
            (2, 2, 1),
        ),
        (
            b"[\n0]".as_slice(),
            JsonLimits {
                max_nodes: 1,
                ..JsonLimits::default()
            },
            (3, 2, 2),
        ),
        (
            b"[\n0]".as_slice(),
            JsonLimits {
                max_bytes: 3,
                ..JsonLimits::default()
            },
            (3, 2, 2),
        ),
    ] {
        for error in [
            Document::parse_with_limits(bytes, limits).unwrap_err(),
            Document::parse_owned_with_limits(bytes.to_vec(), limits).unwrap_err(),
        ] {
            assert_eq!(
                (error.byte_offset(), error.line(), error.byte_column()),
                expected
            );
        }
    }
}

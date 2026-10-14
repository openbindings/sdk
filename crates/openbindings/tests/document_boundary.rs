use openbindings::*;

#[test]
fn original_bytes_include_whitespace_and_rebased_children_have_local_locations() {
    let text = " \n {\"openbindings\":\"0.2.0\",\"operations\":{}}\t\n";
    let doc = ParsedDocument::parse(text).unwrap();
    assert_eq!(doc.original_bytes(), text.as_bytes());
    assert_eq!(
        doc.validate()
            .unwrap()
            .validated()
            .unwrap()
            .original_bytes(),
        text.as_bytes()
    );
    let parent =
        JsonValue::parse(r#"{"child":{"openbindings":"0.2.0","operations":{},"unknown":true}}"#)
            .unwrap();
    let child = parent.get("child").unwrap().to_owned();
    let expected = child.text().to_owned();
    let doc = ParsedDocument::from_json(child);
    drop(parent);
    assert_eq!(doc.original_bytes(), expected.as_bytes());
    assert_eq!(doc.value().location().byte_offset, 0);
    assert_eq!(doc.value().location().pointer.as_deref(), Some(""));
}

fn validation(text: &str) -> DocumentValidation {
    validate_document(text).unwrap()
}
#[test]
fn states_do_not_confuse_empty_findings_with_conformance() {
    let parsed =
        ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{},"unknown":true}"#)
            .unwrap();
    let checked = parsed.validate().unwrap();
    assert_eq!(checked.report().conclusion, Conformance::NonConformant);
    assert!(checked.validated().is_none());
    assert_eq!(checked.report().evidence.len(), 13);
    let invalid = validate_document(b"{broken").unwrap();
    assert!(invalid.parsed().is_none());
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
    assert!(validate_document(deep).is_err());
}
#[test]
fn deep_carriage_and_deeper_grammar_have_distinct_limits() {
    for (nested, want) in [
        (9999, Conformance::Conformant),
        (10000, Conformance::Conformant),
    ] {
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"x-deep":{}0{}}}"#,
            "[".repeat(nested),
            "]".repeat(nested)
        );
        let checked = validation(&text);
        assert_eq!(checked.report().conclusion, want);
        assert_eq!(checked.report().evidence["OBI-01"], Evidence::Met);
        assert_eq!(checked.report().evidence["OBI-03"], Evidence::Met);
    }
}
#[test]
fn lone_surrogates_remain_exact_and_are_not_syntax_errors() {
    let text = r#"{"openbindings":"0.2.0","operations":{},"x-data":"\ud800"}"#;
    let checked = validation(text);
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    assert_eq!(checked.report().evidence["OBI-01"], Evidence::Met);
    assert_eq!(checked.parsed().unwrap().original_bytes(), text.as_bytes());
    let duplicate = validation(r#"{"openbindings":"0.2.0","operations":{},"\ud800":1,"\ud800":2}"#);
    assert_eq!(duplicate.report().evidence["OBI-01"], Evidence::Violated);
}
#[test]
fn opaque_data_and_resource_boundaries_are_not_schema_positions() {
    let text = r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/schemas/root/properties/x"}}},"schemas":{"root":{"$id":7,"properties":{"x":true}}},"x-data":{"$ref":"relative"}}"##;
    let r = validation(text);
    assert_eq!(r.report().evidence["OBI-11"], Evidence::Violated);
    assert_eq!(r.report().evidence["OBI-12"], Evidence::Violated);
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
    builder
        .operations
        .insert("first".into(), Operation::default());
    let snapshot = builder.build().unwrap();
    builder.operations.clear();
    let valid = snapshot.validate().unwrap().validated().unwrap();
    assert!(matches!(
        valid.parsed().resolve_operation("first").unwrap(),
        OperationSelection::Found(_)
    ));
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ParsedDocument>();
    send_sync::<ValidatedDocument>();
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
                f.location.as_ref().unwrap().pointer.as_deref().unwrap(),
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

use openbindings::*;
use std::sync::atomic::AtomicBool;
#[test]
fn diagnostic_inspection_refuses_document_preconditions_without_schema_locations() {
    for (text, code, reason) in [
        (
            r#"{"openbindings":"0.3.0","operations":{}}"#,
            "unsupported-version",
            ReferenceRefusalReason::UnsupportedCapability,
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{},"x":1,"x":2}"#,
            "duplicate-members",
            ReferenceRefusalReason::ResolverFailure,
        ),
        (
            r#"{"openbindings":5,"openbindings":"0.2.0","operations":{}}"#,
            "duplicate-members",
            ReferenceRefusalReason::ResolverFailure,
        ),
    ] {
        let document = Document::parse(text).unwrap();
        let report = document.references();
        assert!(!report.complete);
        assert!(report.references.is_empty());
        let limitation = report.limitation.unwrap();
        assert_eq!(limitation.code, code);
        assert_eq!(limitation.reason, reason);
        assert!(limitation.location.is_none());
        let cancelled =
            document.references_with_resources(ExternalSchemas::default(), &AtomicBool::new(true));
        assert_eq!(
            cancelled.limitation.unwrap().reason,
            ReferenceRefusalReason::Cancelled
        );
        assert_eq!(document.as_bytes(), text.as_bytes());
    }
}

#[test]
fn missing_or_malformed_versions_inspect_under_0_2_without_a_limitation() {
    let body = r##""operations":{"op":{"input":{"allOf":[
        {"$ref":"#/schemas/target"},
        {"$dynamicRef":"#target"},
        {"$ref":"#/schemas/missing"},
        {"$ref":"https://example.test/external"}
    ]}}},"schemas":{"target":{"$dynamicAnchor":"target"}},
    "sources":{"s":{"kind":"k","data":{"$ref":"opaque"}}}"##;
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://example.test/external".into(),
        document: JsonValue::parse(r##"{"$defs":{"target":true},"$ref":"#/$defs/target"}"##)
            .unwrap(),
    }])
    .unwrap();
    let control = AtomicBool::new(false);
    let valid = Document::parse(format!(r#"{{"openbindings":"0.2.0",{body}}}"#)).unwrap();
    let expected = valid.references();
    let supplied = valid.references_with_resources(resources.clone(), &control);
    assert!(expected.complete && supplied.complete);
    assert!(expected.limitation.is_none() && supplied.limitation.is_none());
    assert_eq!(expected.references.len(), 4);
    assert_eq!(supplied.references.len(), 5);
    assert!(matches!(
        &supplied.references[0].resolution,
        ReferenceResolution::Located { target, dynamic_lookup: false }
            if target.pointer == "/schemas/target"
    ));
    assert!(matches!(
        &supplied.references[1].resolution,
        ReferenceResolution::Located { target, dynamic_lookup: true }
            if target.pointer == "/schemas/target"
    ));
    assert!(matches!(
        &supplied.references[2].resolution,
        ReferenceResolution::Unresolved { detail }
            if detail.reason == ReferenceRefusalReason::MissingTarget
    ));
    assert!(matches!(
        &supplied.references[4].resolution,
        ReferenceResolution::Located { target, dynamic_lookup: false }
            if target.source_uri.as_deref() == Some("https://example.test/external")
                && target.pointer == "/$defs/target"
    ));
    for declaration in [None, Some("1"), Some("5"), Some("null"), Some(r#""bad""#)] {
        let member = declaration.map_or(String::new(), |v| format!(r#""openbindings":{v},"#));
        let text = format!("{{{member}{body}}}");
        let draft = Document::parse(&text).unwrap();
        let validation = draft.validate().unwrap();
        assert_eq!(validation.report().evidence["OBI-03"], Evidence::Violated);
        assert_eq!(validation.report().evidence["OBI-12"], Evidence::Violated);
        assert_eq!(draft.references(), expected);
        assert_eq!(
            draft.references_with_resources(resources.clone(), &control),
            supplied
        );
        assert_eq!(
            draft
                .references_with_resources(resources.clone(), &AtomicBool::new(true))
                .limitation
                .unwrap()
                .reason,
            ReferenceRefusalReason::Cancelled
        );
        assert_eq!(draft.as_bytes(), text.as_bytes());
    }
    // Retain the earlier minimal precondition fixtures, now as complete reports.
    for text in [
        r#"{"operations":{}}"#,
        r#"{"openbindings":1,"operations":{}}"#,
    ] {
        let report = Document::parse(text).unwrap().references();
        assert!(report.complete);
        assert!(report.references.is_empty());
        assert!(report.limitation.is_none());
    }
}
#[test]
fn nonconformant_drafts_still_supply_original_reference_diagnostics() {
    let doc = Document::parse(r#"{"openbindings":"0.2.0","description":42,"operations":{"op":{"input":{"$ref":"https://missing.test/schema"}}},"sources":{"s":{"kind":"k","data":{"$ref":"opaque"}}}}"#).unwrap();
    assert!(doc.validate().unwrap().conformant().is_none());
    let report = doc.references();
    assert!(report.complete);
    assert_eq!(report.references.len(), 1);
    assert_eq!(
        report.references[0].location.pointer,
        "/operations/op/input/$ref"
    );
    assert_eq!(
        report.references[0].spelling.as_deref(),
        Some("https://missing.test/schema")
    );
    let ReferenceResolution::Unresolved { detail } = &report.references[0].resolution else {
        panic!()
    };
    assert_eq!(detail.reason, ReferenceRefusalReason::ResourceUnavailable);
}

#[test]
fn supplied_resource_references_are_reported_transitively_once() {
    let draft = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/a"}}}}"#).unwrap();
    let resources = ExternalSchemas::new(
        [
            ("https://example.test/a", r#"{"$ref":"b"}"#),
            (
                "https://example.test/b",
                r#"{"allOf":[{"$ref":"a"},{"$ref":"missing"}],"const":{"$ref":"opaque"}}"#,
            ),
            ("https://example.test/unused", r#"{"$ref":"also-missing"}"#),
        ]
        .map(|(uri, text)| ExternalSchema {
            uri: uri.into(),
            document: JsonValue::parse(text).unwrap(),
        }),
    )
    .unwrap();
    let proof = draft.validate().unwrap().conformant().unwrap();
    let cancelled = AtomicBool::new(false);
    let report = draft.references_with_resources(resources.clone(), &cancelled);
    assert_eq!(
        report,
        proof.references_with_resources(resources, &cancelled)
    );
    assert!(report.complete);
    assert!(report.limitation.is_none());
    assert_eq!(report.references.len(), 4);
    // Freeze every field of this existing small transitive fixture, including
    // traversal order and the separately owned unresolved location.
    let origin = |source: Option<&str>, pointer: &str| serde_json::json!({"source_uri":source,"pointer":pointer});
    let located = |source: Option<&str>, pointer, spelling, target| {
        serde_json::json!({
            "location":origin(source, pointer),"keyword":"$ref","spelling":spelling,
            "resource_root":source.map(|uri| origin(Some(uri), "")),
            "resolution":{"status":"located","target":origin(Some(target), ""),"dynamic_lookup":false}
        })
    };
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::json!({
            "complete":true,"limitation":null,"references":[
                located(None, "/operations/op/input/$ref", "https://example.test/a", "https://example.test/a"),
                located(Some("https://example.test/a"), "/$ref", "b", "https://example.test/b"),
                located(Some("https://example.test/b"), "/allOf/0/$ref", "a", "https://example.test/a"),
                {"location":origin(Some("https://example.test/b"), "/allOf/1/$ref"),"keyword":"$ref","spelling":"missing",
                 "resource_root":origin(Some("https://example.test/b"), ""),
                 "resolution":{"status":"unresolved","detail":{
                    "reason":"resource-unavailable","code":"resource-unavailable",
                    "message":"resolution requires a resource that was not supplied",
                    "location":origin(Some("https://example.test/b"), "/allOf/1"),
                    "resource_uri":"https://example.test/missing"
                 }}}
            ]
        })
    );
    assert!(
        report.references[..3]
            .iter()
            .all(|r| matches!(r.resolution, ReferenceResolution::Located { .. }))
    );
    let missing = &report.references[3];
    assert_eq!(
        missing.location.source_uri.as_deref(),
        Some("https://example.test/b")
    );
    assert_eq!(missing.location.pointer, "/allOf/1/$ref");
    assert_eq!(missing.spelling.as_deref(), Some("missing"));
    let ReferenceResolution::Unresolved { detail } = &missing.resolution else {
        panic!("missing resource")
    };
    assert_eq!(detail.reason, ReferenceRefusalReason::ResourceUnavailable);
    assert_eq!(
        detail.resource_uri.as_deref(),
        Some("https://example.test/missing")
    );
}

#[test]
fn reached_supplied_schemas_report_references_beyond_256_levels() {
    let draft = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/a"}}}}"#).unwrap();
    let text = format!(
        "{}{{\"$ref\":\"https://example.test/missing\"}}{}",
        "{\"not\":".repeat(257),
        "}".repeat(257)
    );
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://example.test/a".into(),
        document: JsonValue::parse(text).unwrap(),
    }])
    .unwrap();
    let report = draft.references_with_resources(resources, &AtomicBool::new(false));
    assert!(report.complete);
    assert!(report.limitation.is_none());
    assert_eq!(report.references.len(), 2);
    let reached = &report.references[1];
    assert_eq!(
        reached.location.source_uri.as_deref(),
        Some("https://example.test/a")
    );
    assert_eq!(
        reached.location.pointer,
        format!("{}/$ref", "/not".repeat(257))
    );
    let ReferenceResolution::Unresolved { detail } = &reached.resolution else {
        panic!("missing resource")
    };
    assert_eq!(detail.reason, ReferenceRefusalReason::ResourceUnavailable);
    assert_eq!(
        detail.resource_uri.as_deref(),
        Some("https://example.test/missing")
    );
}

#[test]
fn reached_supplied_sources_include_nested_identifier_scopes() {
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/a"}}}}"#).unwrap();
    let resources = ExternalSchemas::new([ExternalSchema {
        uri: "https://example.test/a".into(),
        document: JsonValue::parse(
            r#"{"properties":{"p":{"$id":"https://nested.test/base/","$ref":"missing"}}}"#,
        )
        .unwrap(),
    }])
    .unwrap();
    let report = document.references_with_resources(resources, &AtomicBool::new(false));
    assert!(report.complete);
    assert_eq!(report.references.len(), 2);
    let reference = &report.references[1];
    assert_eq!(
        reference.location.source_uri.as_deref(),
        Some("https://example.test/a")
    );
    assert_eq!(reference.location.pointer, "/properties/p/$ref");
    let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
        panic!("missing resource")
    };
    assert_eq!(
        detail.resource_uri.as_deref(),
        Some("https://nested.test/base/missing")
    );
}

#[test]
fn reference_refusals_preserve_the_public_cause_and_safe_message() {
    for (schemas, reason) in [
        (
            r#"{"entry":{"$ref":42}}"#,
            ReferenceRefusalReason::InvalidReference,
        ),
        (
            r##"{"entry":{"$ref":"#%FF"}}"##,
            ReferenceRefusalReason::InvalidReference,
        ),
        (
            r##"{"entry":{"$ref":"#missing"}}"##,
            ReferenceRefusalReason::MissingTarget,
        ),
        (
            r##"{"entry":{"$ref":"#/schemas"}}"##,
            ReferenceRefusalReason::MissingTarget,
        ),
        (
            r##"{"entry":{"$ref":"#same"},"a":{"$anchor":"same"},"b":{"$anchor":"same"}}"##,
            ReferenceRefusalReason::AmbiguousTarget,
        ),
        (
            r#"{"entry":{"$ref":"https://private.test/same"},"a":{"$id":"https://private.test/same"},"b":{"$id":"https://private.test/same"}}"#,
            ReferenceRefusalReason::IdentityConflict,
        ),
        (
            r#"{"entry":{"$ref":"https://private.test/missing"}}"#,
            ReferenceRefusalReason::ResourceUnavailable,
        ),
    ] {
        let document = Document::parse(format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{schemas}}}"#
        ))
        .unwrap();
        let report = document.references();
        assert!(report.complete);
        let ReferenceResolution::Unresolved { detail } = &report.references[0].resolution else {
            panic!("expected reference refusal")
        };
        assert_eq!(detail.reason, reason);
        assert_eq!(report.references[0].location.pointer, "/schemas/entry/$ref");
        assert!(!detail.to_string().is_empty());
        assert_eq!(detail.to_string(), detail.message);
        assert!(!detail.message.contains("private"));
    }
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://example.test/external"}}}"#).unwrap();
    let external = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://example.test/external").into(),
        document: JsonValue::parse(r#"{"$schema":"https://example.test/unknown"}"#).unwrap(),
    }])
    .unwrap();
    let report = document.references_with_resources(external, &AtomicBool::new(false));
    let ReferenceResolution::Unresolved { detail } = &report.references[0].resolution else {
        panic!("expected reference refusal")
    };
    assert_eq!(detail.reason, ReferenceRefusalReason::UnsupportedCapability);
    // A cancellation after admission must retain the same public cancellation cause.
    struct StopAfter(std::sync::atomic::AtomicUsize);
    impl WorkControl for StopAfter {
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 3
        }
    }
    let report = document.references_with_resources(
        ExternalSchemas::default(),
        &StopAfter(std::sync::atomic::AtomicUsize::new(0)),
    );
    assert_eq!(
        report.limitation.unwrap().reason,
        ReferenceRefusalReason::Cancelled
    );
}

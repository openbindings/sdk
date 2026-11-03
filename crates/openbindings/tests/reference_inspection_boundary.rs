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
            r#"{"operations":{}}"#,
            "malformed-version",
            ReferenceRefusalReason::ResolverFailure,
        ),
        (
            r#"{"openbindings":1,"operations":{}}"#,
            "malformed-version",
            ReferenceRefusalReason::ResolverFailure,
        ),
        (
            r#"{"openbindings":"0.2.0","operations":{},"x":1,"x":2}"#,
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

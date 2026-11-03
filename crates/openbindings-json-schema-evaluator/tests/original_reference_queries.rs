use openbindings::*;
use openbindings_internal_schema::{OriginalEnvironment, QueryFailure};
use std::sync::atomic::AtomicBool;
fn env(document: &str, resources: &[(&str, &str)]) -> SchemaEnvironment {
    let doc = Document::parse(document)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let schema = doc.resolve_operation("op").unwrap().input().unwrap();
    let external = ExternalSchemas::new(resources.iter().map(|(uri, s)| ExternalSchema {
        uri: (*uri).into(),
        document: JsonValue::parse(s).unwrap(),
    }))
    .unwrap();
    schema
        .environment(
            &external,
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
}
const DOC: &str = r#"{"openbindings":"0.2.0","operations":{"op":{"input":true}}}"#;
#[test]
fn original_queries_and_reached_checks_isolate_the_two_supplied_false_refusals() {
    for body in [
        r#"{"$id":"https://e.test/r","$defs":{"good":{"$anchor":"task","type":"string"}}}"#,
        r#"{"$id":"https://e.test/r","$defs":{"good":{"$anchor":"task","type":"string"},"bad":{"$id":"%"}}}"#,
    ] {
        let e = env(DOC, &[("https://e.test/r", body)]);
        let space = e.space();
        for reference in [
            "https://e.test/r#task",
            "https://e.test/r#t%61sk",
            "https://e.test/r#/$defs/good",
        ] {
            let target = space.query(e.entry(), reference).unwrap();
            let routed = space.resolve(e.entry(), reference).unwrap();
            assert_eq!(routed.node, target.node);
            assert_eq!(routed.dynamic_name, target.dynamic_name);
            space.check_known_node(target.node).unwrap();
            assert_eq!(
                space.node(target.node).value.get("type").unwrap().as_str(),
                Some("string")
            );
            assert_eq!(
                space.location(target.node).pointer.as_str(),
                Some("/$defs/good")
            );
        }
    }
}
#[test]
fn queries_retain_exact_contained_selection_and_supplied_alias_conflicts() {
    let doc = r#"{"openbindings":"0.2.0","operations":{"op":{"input":true}},"schemas":{"E":{"$id":"https://e.test/r","type":"integer"}}}"#;
    let e = env(doc, &[("https://e.test/r", r#"{"type":"string"}"#)]);
    let space = e.space();
    let t = space.query(e.entry(), "https://e.test/r").unwrap();
    space.check_known_node(t.node).unwrap();
    assert_eq!(
        space.node(t.node).value.get("type").unwrap().as_str(),
        Some("integer")
    );
    let e = env(
        DOC,
        &[
            ("https://E.test/r", r#"{"type":"string"}"#),
            ("https://e.test/r", r#"{"type":"integer"}"#),
        ],
    );
    assert!(matches!(
        e.space().query(e.entry(), "https://e.test/r"),
        Err(QueryFailure::IdentityConflict)
    ));
}
#[test]
fn unknown_dialect_roots_are_visible_but_default_interpretation_still_refuses() {
    let e = env(
        DOC,
        &[(
            "https://e.test/r",
            r#"{"$schema":"https://e.test/custom","$defs":{"child":{"type":"string"}}}"#,
        )],
    );
    let space = e.space();
    let t = space.query(e.entry(), "https://e.test/r").unwrap();
    assert!(matches!(
        space
            .check_known_node(t.node)
            .unwrap_err()
            .query_failure
            .as_deref(),
        Some(QueryFailure::UnsupportedDialect(_))
    ));
    assert!(matches!(
        space.query(e.entry(), "https://e.test/r#/$defs/child"),
        Err(QueryFailure::UnsupportedDialect(_))
    ));
}
#[test]
fn dynamic_lookup_flags_and_anchor_ambiguity_are_resource_local() {
    let e = env(
        DOC,
        &[(
            "https://e.test/r",
            r#"{"$defs":{"good":{"$dynamicAnchor":"task","type":"string"}}}"#,
        )],
    );
    let s = e.space();
    let t = s.query(e.entry(), "https://e.test/r#t%61sk").unwrap();
    assert_eq!(t.dynamic_name.as_deref(), Some("task"));
    s.check_known_node(t.node).unwrap();
    assert!(
        s.query(e.entry(), "https://e.test/r#/$defs/good")
            .unwrap()
            .dynamic_name
            .is_none()
    );
    let e = env(
        DOC,
        &[(
            "https://e.test/r",
            r#"{"$defs":{"a":{"$anchor":"task"},"b":{"$dynamicAnchor":"task"}}}"#,
        )],
    );
    assert!(matches!(
        e.space().query(e.entry(), "https://e.test/r#t%61sk"),
        Err(QueryFailure::AmbiguousAnchor)
    ));
}

#[test]
fn missing_callbacks_report_only_unavailable_resource_identities() {
    let e = env(DOC, &[("https://e.test/r", r#"{"$defs":{"known":true}}"#)]);
    let space = e.space();
    let mut missing = Vec::new();
    for reference in [
        "https://e.test/absent#anchor",
        "https://e.test/r#absent",
        "https://e.test/r#/$defs/absent",
        "%",
    ] {
        let failure = space
            .resolve_with_missing(e.entry(), reference, &mut |uri| {
                missing.push(uri.to_owned())
            })
            .unwrap_err();
        assert!(failure.query_failure.is_some());
    }
    assert_eq!(missing, ["https://e.test/absent"]);
}

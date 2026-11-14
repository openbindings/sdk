use openbindings::*;
#[test]
fn dynamic_reference_target_debug_does_not_disclose_its_anchor() {
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"S":{"$id":"https://example.test/schema","$dynamicAnchor":"SENTINEL-secret-7391"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
    let schema = document.schema("S").unwrap();
    let environment = schema
        .environment(
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let target = environment
        .resolve(&environment.root(), "#SENTINEL-secret-7391")
        .unwrap();
    assert_eq!(
        target.dynamic_anchor.as_deref(),
        Some("SENTINEL-secret-7391")
    );
    let debug = format!("{target:?}");
    assert!(!debug.contains("SENTINEL-secret-7391"), "{debug}");
}

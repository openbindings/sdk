use openbindings::{DocumentBuilder, JsonValue, Source};
use openbindings_source_inspection::{InspectionErrorKind, Inspector};
use source_inspection_consumer::{CATALOG_KIND, CatalogInspector, author_selected};

fn source(content: &str) -> Source {
    Source {
        kind: CATALOG_KIND.into(),
        content: Some(JsonValue::parse(content).unwrap()),
        ..Default::default()
    }
}

#[tokio::test]
async fn inventory_authoring_and_retained_values() {
    let inspector = Inspector::new(CatalogInspector);
    // An empty advisory list is not a negative support verdict.
    assert!(inspector.list_supported_kinds().unwrap().is_empty());
    let support = inspector
        .check_kind_support([
            CATALOG_KIND,
            "URN:example:inspection-catalog:v1",
            CATALOG_KIND,
        ])
        .unwrap();
    assert_eq!(support.len(), 2);
    assert!(support[0].supported);
    assert!(!support[1].supported);
    let source = source(
        r#"[
        {"id":"空.🚲","status":"admitted","binding":{"整数":9007199254740993},"input":true,"idempotent":false},
        {"id":"null","status":"admitted","binding":null},
        {"id":"absent","status":"admitted"},
        {"id":"excluded","status":"excluded"}
    ]"#,
    );
    fn assert_send<T: Send>(_: &T) {}
    let future = inspector.inspect(&source);
    assert_send(&future);
    let first = future.await.unwrap();
    let second = inspector.inspect(&source).await.unwrap();
    assert_eq!(
        first.to_json().unwrap().text(),
        second.to_json().unwrap().text()
    );
    assert!(first.exhaustive);
    assert_eq!(first.targets.len(), 3);
    assert_eq!(first.targets[0].source_ref, "空.🚲");
    assert_eq!(first.targets[0].binding.idempotent, Some(false));
    assert!(first.targets[1].operation.is_none());
    assert_eq!(
        first.targets[1].binding.content.as_ref().unwrap().text(),
        "null"
    );
    assert!(first.targets[2].binding.content.is_none());
    let authored = author_selected(source.clone()).await.unwrap();
    let draft = DocumentBuilder::from_json(&authored).unwrap();
    let binding = &draft.bindings.unwrap()["chosen.binding"];
    assert_eq!(binding.operation, "chosen.operation");
    assert_eq!(binding.source, "chosen.source");
    drop(source);
    assert_eq!(
        first.targets[0]
            .binding
            .content
            .as_ref()
            .unwrap()
            .view()
            .get("整数")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
}

#[tokio::test]
async fn incomplete_excluded_empty_and_unreadable_are_distinct() {
    let inspector = Inspector::new(CatalogInspector);
    for content in ["[]", r#"[{"id":"excluded","status":"excluded"}]"#] {
        let result = inspector.inspect(&source(content)).await.unwrap();
        assert!(result.exhaustive);
        assert!(result.targets.is_empty());
    }
    let partial = inspector
        .inspect(&source(r#"[{"id":"missing","status":"unavailable"}]"#))
        .await
        .unwrap();
    assert!(!partial.exhaustive);
    assert!(partial.targets.is_empty());
    assert_eq!(
        partial.limitation.unwrap().code,
        "example.catalog.unavailable"
    );
    assert_eq!(
        inspector.inspect(&source("null")).await.unwrap_err().kind(),
        InspectionErrorKind::UnreadableSource
    );
    let unsupported = Source {
        kind: format!("{CATALOG_KIND}/future"),
        ..Default::default()
    };
    assert_eq!(
        inspector.inspect(&unsupported).await.unwrap_err().kind(),
        InspectionErrorKind::UnsupportedKind
    );
}

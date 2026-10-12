use dynamic_openapi_client::{Document, Limits, Method};
use openapi_inspection_consumer::{author, inspect_and_author, source};
use openbindings::{Binding, DocumentBuilder, JsonValue};

#[tokio::test]
async fn public_inspection_manual_authoring_and_exact_reload() {
    let content = r#"{"document":{"openapi":"3.1.2","x-exact":9007199254740993,"components":{"schemas":{"Opaque":{"$schema":"urn:example:unknown-dialect","const":9007199254740993}}},"paths":{"/a~b/c":{"get":{"operationId":"totally.different-É"}}}},"location":"https://fixture.invalid/openapi.json"}"#;
    let result = inspect_and_author(content, "/paths/~1a~0b~1c/get")
        .await
        .unwrap();
    assert!(result.text().contains("9007199254740993"));
    assert!(result.text().contains("urn:example:unknown-dialect"));
    let reloaded = DocumentBuilder::from_json(&result).unwrap();
    let source = &reloaded.sources.as_ref().unwrap()["catalog-source"];
    let document = source.content.as_ref().unwrap().get("document").unwrap();
    let client = Document::parse(
        document.text(),
        Some("https://fixture.invalid/openapi.json"),
        Limits::default(),
    )
    .unwrap();
    let operation = client.operation("/a~b/c", Method::Get).unwrap();
    assert_eq!(operation.id().as_deref(), Some("totally.different-É"));
    assert_eq!(
        client.root().get("x-exact").unwrap().raw(),
        "9007199254740993"
    );
    let binding = &reloaded.bindings.as_ref().unwrap()["catalog-binding"];
    assert_eq!(
        binding
            .content
            .as_ref()
            .unwrap()
            .get("target")
            .unwrap()
            .as_str(),
        Some("/paths/~1a~0b~1c/get")
    );
    // Independent manual OBI construction does not use inspection suggestions.
    let manual = author(
        source.clone(),
        Binding {
            operation: "catalog.lookup".into(),
            source: "catalog-source".into(),
            content: Some(JsonValue::parse(r#"{"target":"/paths/~1a~0b~1c/get"}"#).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        DocumentBuilder::from_json(&manual)
            .unwrap()
            .build()
            .unwrap()
            .assess()
            .unwrap()
            .validated()
            .is_some()
    );
}
#[tokio::test]
async fn real_resource_recovery_from_public_packages() {
    use openbindings_openapi::{OpenApi, Options, ResourceInput};
    use openbindings_source_inspection::Inspector;
    let source=source(r#"{"document":{"openapi":"3.1.2","paths":{"/mounted":{"$ref":"./paths.json#/components/pathItems/P"}}},"location":"https://fixture.invalid/api.json"}"#).unwrap();
    let partial = Inspector::new(OpenApi::default())
        .inspect(&source)
        .await
        .unwrap();
    assert!(!partial.exhaustive);
    let mut options = Options::default();
    options.resources.push(
        ResourceInput::at(
            "https://fixture.invalid/paths.json",
            &br#"{"openapi":"3.1.2","components":{"pathItems":{"P":{"get":{}}}}}"#[..],
        )
        .unwrap(),
    );
    let full = Inspector::new(OpenApi::new(options).unwrap())
        .inspect(&source)
        .await
        .unwrap();
    assert!(full.exhaustive);
    assert_eq!(full.targets[0].source_ref, "/paths/~1mounted/get");
    let authored = openapi_inspection_consumer::author_selected(
        source,
        full.targets.into_iter().next().unwrap(),
    )
    .unwrap();
    let reloaded = DocumentBuilder::from_json(&authored).unwrap();
    let source = &reloaded.sources.as_ref().unwrap()["catalog-source"];
    let content = source.content.as_ref().unwrap();
    let mut graph = dynamic_openapi_client::DocumentBuilder::new(
        Limits::default(),
        dynamic_openapi_client::GraphLimits::default(),
    );
    let entry = graph
        .add(
            ResourceInput::at(
                content.get("location").unwrap().as_str().unwrap(),
                content.get("document").unwrap().text().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    graph
        .add(
            ResourceInput::at(
                "https://fixture.invalid/paths.json",
                &br#"{"openapi":"3.1.2","components":{"pathItems":{"P":{"get":{}}}}}"#[..],
            )
            .unwrap(),
        )
        .unwrap();
    let document = graph.build(entry).unwrap();
    let operation = document.operation("/mounted", Method::Get).unwrap();
    assert_eq!(operation.path(), "/mounted");
    assert_eq!(operation.value().raw(), "{}");
    assert_eq!(
        reloaded.bindings.as_ref().unwrap()["catalog-binding"]
            .content
            .as_ref()
            .unwrap()
            .get("target")
            .unwrap()
            .as_str(),
        Some("/paths/~1mounted/get")
    );
}

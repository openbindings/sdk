use openbindings::{JsonValue, Source, WorkControl};
use openbindings_openapi::{OPENAPI_31, OpenApi, Options, ResourceInput};
use openbindings_source_inspection::{InspectionErrorKind, Inspector, SourceInspection};
use std::{collections::BTreeSet, sync::Arc};

fn content(text: &str) -> Source {
    Source {
        kind: OPENAPI_31.into(),
        content: Some(JsonValue::parse(text).unwrap()),
        ..Default::default()
    }
}
fn source(document: &str) -> Source {
    content(&format!(r#"{{"document":{document}}}"#))
}
fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn refs(result: &SourceInspection) -> BTreeSet<&str> {
    result
        .targets
        .iter()
        .map(|t| t.source_ref.as_str())
        .collect()
}

#[tokio::test]
async fn ai01_ai02_ai03_ai05_ai06_ai07_targets_and_carriers() {
    let inspector = Inspector::new(OpenApi::default());
    for (name, expected) in [
        ("minimal-31.json", vec!["/paths/~1items/get"]),
        (
            "identity-31.json",
            vec![
                "/paths/~1a~0b~1c/get",
                "/paths/~1unnamed/post",
                "/paths/~1unicode/get",
            ],
        ),
        ("mounted-31.json", vec!["/paths/~1mounted/get"]),
        ("malformed-neighbor-31.json", vec!["/paths/~1ready/get"]),
        ("schema-boundary-31.json", vec!["/paths/~1exact/get"]),
        ("subordinate-defects.json", vec!["/paths/~1usable/get"]),
    ] {
        let result = inspector.inspect(&source(&fixture(name))).await.unwrap();
        assert!(result.exhaustive, "{name}");
        assert_eq!(refs(&result), expected.into_iter().collect(), "{name}");
        for target in &result.targets {
            assert!(target.operation.is_none());
            assert!(target.binding.idempotent.is_none());
            assert_eq!(
                target
                    .binding
                    .content
                    .as_ref()
                    .unwrap()
                    .get("target")
                    .unwrap()
                    .as_str(),
                Some(target.source_ref.as_str())
            );
        }
    }
    for name in ["minimal-31.json", "minimal-31.yaml"] {
        let text = serde_json_compat::to_string(&fixture(name)).unwrap();
        let result = inspector.inspect(&source(&text)).await.unwrap();
        assert!(result.exhaustive);
        assert_eq!(refs(&result), BTreeSet::from(["/paths/~1items/get"]));
    }
    // Empty URI fragment is allowed and removed; it does not become an OBI base.
    assert!(inspector.inspect(&content(r#"{"document":{"openapi":"3.1.0","paths":{}},"location":"https://fixture.invalid/api#"}"#)).await.unwrap().exhaustive);
}

#[tokio::test]
async fn ai04_missing_resource_recovery_and_real_base() {
    let doc = fixture("missing-resource-31.json");
    let with_base = content(&format!(
        r#"{{"document":{doc},"location":"https://fixture.invalid/openapi.json"}}"#
    ));
    let original = Inspector::new(OpenApi::default());
    let partial = original.inspect(&with_base).await.unwrap();
    assert!(!partial.exhaustive);
    assert_eq!(refs(&partial), BTreeSet::from(["/paths/~1ready/get"]));
    let evidence = partial.to_json().unwrap();
    assert!(
        evidence
            .text()
            .contains("https://fixture.invalid/paths.json")
    );
    let mut options = Options::default();
    options.resources.push(
        ResourceInput::at(
            "https://fixture.invalid/paths.json",
            fixture("supplied-paths-31.json").into_bytes(),
        )
        .unwrap(),
    );
    let recovered = Inspector::new(OpenApi::new(options).unwrap());
    let result = recovered.inspect(&with_base).await.unwrap();
    assert!(result.exhaustive);
    assert_eq!(
        refs(&result),
        BTreeSet::from(["/paths/~1ready/get", "/paths/~1pending/get"])
    );
    assert!(!original.inspect(&with_base).await.unwrap().exhaustive);
    // Supplied bytes cannot invent a base for an anonymous entry document.
    assert!(!recovered.inspect(&source(&doc)).await.unwrap().exhaustive);
}

#[tokio::test]
async fn ai08_to_ai11_refusals_and_empty_inventory() {
    let inspector = Inspector::new(OpenApi::default());
    for (kind, name) in [
        ("openbindings.openapi-2.0@1", "contrast-20.json"),
        ("openbindings.openapi-3.0@1", "contrast-30.json"),
        ("openbindings.openapi-3.2@1", "contrast-32.json"),
    ] {
        let mut source = source(&fixture(name));
        assert_eq!(
            inspector.inspect(&source).await.unwrap_err().code(),
            "openbindings.openapi.unsupported-version"
        );
        source.kind = kind.into();
        assert_eq!(
            inspector.inspect(&source).await.unwrap_err().kind(),
            InspectionErrorKind::UnsupportedKind
        );
    }
    for text in [
        "null",
        "{}",
        r#"{"document":42}"#,
        r#"{"document":null}"#,
        r#"{"location":null}"#,
        r#"{"document":{},"extra":true}"#,
        r#"{"document":{},"document":{}}"#,
        r#"{"document":{},"location":"relative"}"#,
        r#"{"document":{},"location":"https://fixture.invalid/#fragment"}"#,
    ] {
        assert_eq!(
            inspector.inspect(&content(text)).await.unwrap_err().kind(),
            InspectionErrorKind::InvalidInput,
            "{text}"
        );
    }
    let mut absent = content("null");
    absent.content = None;
    assert_eq!(
        inspector.inspect(&absent).await.unwrap_err().kind(),
        InspectionErrorKind::InvalidInput
    );
    assert_eq!(
        inspector
            .inspect(&content(r#"{"location":"https://fixture.invalid/api"}"#))
            .await
            .unwrap_err()
            .code(),
        "openbindings.openapi.source-unavailable"
    );
    assert_eq!(
        inspector
            .inspect(&source(&fixture("future-patch.json")))
            .await
            .unwrap_err()
            .code(),
        "openbindings.openapi.unsupported-version"
    );
    let mut wrong = source(&fixture("minimal-31.json"));
    wrong.kind = "OpenBindings.openapi-3.1@1".into();
    assert_eq!(
        inspector.inspect(&wrong).await.unwrap_err().kind(),
        InspectionErrorKind::UnsupportedKind
    );
    for name in ["empty.json", "malformed-only.json"] {
        let result = inspector.inspect(&source(&fixture(name))).await.unwrap();
        assert!(result.exhaustive && result.targets.is_empty());
    }
}

#[tokio::test]
async fn ai12_identity_cancellation_limits_ownership() {
    let provider = Arc::new(OpenApi::default());
    let inspector = Inspector::new(&provider);
    let source = source(&fixture("duplicate-ids.json"));
    let future = inspector.inspect(&source);
    fn send<T: Send>(_: &T) {}
    send(&future);
    let result = future.await.unwrap();
    assert_eq!(
        refs(&result),
        BTreeSet::from(["/paths/~1first/get", "/paths/~1second/get"])
    );
    assert!(result.exhaustive);
    let control = WorkControl::new();
    control.cancel();
    assert_eq!(
        inspector
            .inspect_with_control(&source, &control)
            .await
            .unwrap_err()
            .kind(),
        InspectionErrorKind::Cancelled
    );
    let mut options = Options::default();
    options.limits.operations = 1;
    let limited = Inspector::new(OpenApi::new(options).unwrap());
    assert_eq!(
        limited.inspect(&source).await.unwrap_err().code(),
        "openbindings.openapi.limit"
    );
    let mut options = Options::default();
    options.result_bytes = 1;
    assert_eq!(
        Inspector::new(OpenApi::new(options).unwrap())
            .inspect(&source)
            .await
            .unwrap_err()
            .code(),
        "openbindings.openapi.limit"
    );
    let mut options = Options::default();
    options.source_bytes = 1;
    assert_eq!(
        Inspector::new(OpenApi::new(options).unwrap())
            .inspect(&source)
            .await
            .unwrap_err()
            .code(),
        "openbindings.openapi.limit"
    );
    drop(provider);
    drop(source);
    assert!(result.to_json().unwrap().text().contains("~1second"));
}

#[tokio::test]
async fn client_limits_do_not_become_kind_exclusions() {
    let inspector = Inspector::new(OpenApi::default());
    for (name, reason) in [
        ("unknown-path-field.json", "WrongReferenceKind"),
        ("method-conflict.json", "AmbiguousReference"),
        ("invalid-paths.json", "InvalidPathsShape"),
    ] {
        let result = inspector.inspect(&source(&fixture(name))).await.unwrap();
        assert!(!result.exhaustive, "{name}");
        assert!(result.to_json().unwrap().text().contains(reason));
        if name != "invalid-paths.json" {
            assert_eq!(refs(&result), BTreeSet::from(["/paths/~1ready/get"]));
        }
    }
}

#[tokio::test]
async fn later_edition_fields_cannot_invent_or_duplicate_targets() {
    let inspector = Inspector::new(OpenApi::default());
    for (path_item, count) in [
        (r#"{"additionalOperations":{"GET":{}}}"#, 0),
        (r#"{"get":{},"additionalOperations":{"GET":{}}}"#, 1),
    ] {
        let source = source(&format!(
            r#"{{"openapi":"3.1.2","paths":{{"/x":{path_item}}}}}"#
        ));
        let result = inspector.inspect(&source).await.unwrap();
        assert!(result.exhaustive);
        assert_eq!(result.targets.len(), count);
    }
    let malformed = source(r#"{"swagger":"3.1.2","paths":{}}"#);
    assert_eq!(
        inspector.inspect(&malformed).await.unwrap_err().code(),
        "openbindings.openapi.unsupported-version"
    );
}

#[tokio::test]
async fn admission_and_partial_budget_regressions() {
    let inspector = Inspector::new(OpenApi::default());
    for document in [
        r#"{}"#,
        r#"{"openapi":null}"#,
        r#"{"openapi":31}"#,
        r#"{"swagger":"3.1.2","paths":{}}"#,
    ] {
        assert_eq!(
            inspector
                .inspect(&source(document))
                .await
                .unwrap_err()
                .code(),
            "openbindings.openapi.unsupported-version"
        );
    }
    let unsupported = content(
        r#"{"document":{"openapi":"3.1.2","paths":{}},"location":"https://reader@example.invalid/api"}"#,
    );
    let error = inspector.inspect(&unsupported).await.unwrap_err();
    assert_eq!(error.kind(), InspectionErrorKind::UnreadableSource);
    assert_eq!(error.code(), "openbindings.openapi.unsupported-location");
    assert!(!error.message().contains("reader"));
    let mut options = Options::default();
    options.result_bytes = 0;
    assert_eq!(
        Inspector::new(OpenApi::new(options).unwrap())
            .inspect(&source(r#"{"openapi":"3.1.2","paths":[]}"#))
            .await
            .unwrap_err()
            .code(),
        "openbindings.openapi.limit"
    );
    // Exact target-text boundary and one-byte-under rejection.
    let source = source(r#"{"openapi":"3.1.2","paths":{"/x":{"get":{}}}}"#);
    let text_bytes = "/paths/~1x/get".len() + r#"{"target":"/paths/~1x/get"}"#.len();
    for budget in [text_bytes - 1, text_bytes] {
        let mut options = Options::default();
        options.result_bytes = budget;
        let result = Inspector::new(OpenApi::new(options).unwrap())
            .inspect(&source)
            .await;
        assert_eq!(result.is_ok(), budget == text_bytes);
    }
}

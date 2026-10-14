use openbindings::{Document, JsonValue, Source};
use openbindings_source_inspection::CancellationToken;
use openbindings_source_inspection::{InspectionErrorKind, Inspector};
use source_inspection_consumer::{
    AuthoringFailure, AuthoringOutcome, CATALOG_KIND, CatalogInspector, author_selected,
    author_target,
};

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
    let AuthoringOutcome::Ready(authored) = author_selected(&source).await.unwrap() else {
        panic!("expected a ready document")
    };
    let draft = Document::from_json(&authored).unwrap();
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

#[tokio::test]
async fn authoring_preserves_partial_inventory_and_document_repair_evidence() {
    let source = source(
        r#"[{"id":"known","status":"admitted","binding":9007199254740993},
        {"id":"unknown","status":"unavailable"}]"#,
    );
    let AuthoringOutcome::Partial(inspection) = author_selected(&source).await.unwrap() else {
        panic!("expected selection decision")
    };
    assert!(!inspection.exhaustive);
    let limitation = inspection.limitation.as_ref().unwrap();
    assert_eq!(limitation.code, "example.catalog.unavailable");
    assert_eq!(limitation.details.as_ref().unwrap()["omitted"].text(), "1");
    assert_eq!(inspection.targets.len(), 1);
    let selected = inspection.targets.into_iter().next().unwrap();
    assert!(matches!(
        author_target(&source, selected.clone()).unwrap(),
        AuthoringOutcome::Ready(_)
    ));
    // A caller can edit framing after inspection. Retain the resulting validation
    // and editable draft if that edit makes the final OBI invalid.
    let mut edited = selected;
    edited.operation = Some(openbindings::Operation {
        input: Some(JsonValue::parse("42").unwrap()),
        ..Default::default()
    });
    let AuthoringOutcome::NeedsRepair { draft, validation } =
        author_target(&source, edited).unwrap()
    else {
        panic!("expected repair evidence")
    };
    assert_eq!(
        draft.operations["chosen.operation"]
            .input
            .as_ref()
            .unwrap()
            .text(),
        "42"
    );
    assert!(!validation.report().findings.is_empty());
    assert!(validation.validated().is_none());
    let unsupported = Source {
        kind: "urn:other".into(),
        ..Default::default()
    };
    let Err(AuthoringFailure::Inspection(error)) = author_selected(&unsupported).await else {
        panic!("expected typed inspection refusal")
    };
    assert_eq!(error.kind(), InspectionErrorKind::UnsupportedKind);
}

#[tokio::test]
async fn unestablished_framing_is_omitted_without_losing_admitted_targets() {
    for hint in ["42", "null", "[]", r#"{"type":42}"#] {
        let source = source(&format!(
            r#"[{{"id":"one","status":"admitted","input":{hint}}}]"#
        ));
        let inspection = Inspector::new(CatalogInspector)
            .inspect(&source)
            .await
            .unwrap();
        assert!(inspection.exhaustive);
        assert_eq!(inspection.targets.len(), 1);
        assert!(inspection.targets[0].operation.is_none());
        assert!(matches!(
            author_selected(&source).await.unwrap(),
            AuthoringOutcome::Ready(_)
        ));
    }
}

#[tokio::test]
async fn borrowed_and_shared_non_clone_provider_keep_the_same_state_and_send_future() {
    use openbindings_source_inspection::{
        InspectionError, SourceInspection, SourceInspector, SupportedKind,
    };
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct StatefulCatalog {
        calls: AtomicUsize,
    }
    impl StatefulCatalog {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }
    impl SourceInspector for StatefulCatalog {
        type Inspection<'a> =
            Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>;
        fn supports_kind(&self, kind: &str) -> bool {
            kind == CATALOG_KIND
        }
        fn supported_kinds(&self) -> Vec<SupportedKind> {
            vec![SupportedKind {
                kind: CATALOG_KIND.into(),
                description: None,
            }]
        }
        fn inspect<'a>(
            &'a self,
            source: &'a Source,
            control: CancellationToken,
        ) -> Self::Inspection<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::Relaxed);
                CatalogInspector.inspect(source, control).await
            })
        }
    }
    let provider = Arc::new(StatefulCatalog {
        calls: AtomicUsize::new(0),
    });
    let borrowed = Inspector::new(provider.as_ref());
    let shared = Inspector::new(provider.clone());
    let source = source("[]");
    fn assert_send<T: Send>(_: &T) {}
    let future = borrowed.inspect(&source);
    assert_send(&future);
    future.await.unwrap();
    let future = shared.inspect(&source);
    assert_send(&future);
    future.await.unwrap();
    for listing in [
        borrowed.list_supported_kinds(),
        shared.list_supported_kinds(),
    ] {
        assert_eq!(listing.unwrap()[0].kind, CATALOG_KIND);
    }
    assert!(shared.check_kind_support([CATALOG_KIND]).unwrap()[0].supported);
    assert_eq!(provider.calls(), 2);
}

#[tokio::test]
async fn stack_borrowed_provider_has_an_unboxed_send_future() {
    use openbindings_source_inspection::{InspectionError, SourceInspection, SourceInspector};
    use std::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll},
    };
    struct Adapter<'env>(&'env AtomicUsize);
    struct Work<'a>(&'a AtomicUsize);
    impl Future for Work<'_> {
        type Output = Result<SourceInspection, InspectionError>;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Ok(SourceInspection::complete(vec![])))
        }
    }
    impl SourceInspector for Adapter<'_> {
        type Inspection<'a>
            = Work<'a>
        where
            Self: 'a;
        fn supports_kind(&self, kind: &str) -> bool {
            kind == CATALOG_KIND
        }
        fn inspect<'a>(&'a self, _: &'a Source, _: CancellationToken) -> Self::Inspection<'a> {
            Work(self.0)
        }
    }
    fn send<T: Send>(_: &T) {}
    let calls = AtomicUsize::new(0);
    let adapter = Adapter(&calls);
    let inspector = Inspector::new(&adapter);
    let source = source("[]");
    let future = inspector.inspect(&source);
    send(&future);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    future.await.unwrap();
    let control = CancellationToken::new();
    let future = inspector.inspect_with_control(&source, &control);
    send(&future);
    // The future retains shared cancellation state without borrowing this handle.
    drop(control);
    future.await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

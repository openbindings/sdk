//! A complete local host, assembled from the four object-safe twins.
#[path = "host/context.rs"]
mod context;
#[path = "host/erased.rs"]
mod erased;
#[path = "host/notice.rs"]
mod notice;
#[path = "host/registry.rs"]
mod registry;

use kind_adapter::{AdapterError, Memory};
use openbindings::{ConformantDocument, Document, Edit, JsonValue};
use openbindings_invocation::{
    BindingInvokerExt, BindingRequest, CancellationToken, Context, InvocationSupport, OutputEvent,
};
use openbindings_kind_description::{CheckSourceRequest, DescribeKindRequest, DescriptionLimits};
use openbindings_source_pull::{PullLimits, PullRequest};
use registry::{Registry, Slot};
use std::{error::Error, rc::Rc};

fn memory_slot() -> Slot {
    let adapter = Rc::new(Memory);
    Slot {
        descriptor: adapter.clone(),
        checker: None,
        puller: Some(adapter.clone()),
        invoker: Some(adapter),
    }
}
fn notice_slot() -> Slot {
    let adapter = Rc::new(notice::Notice);
    Slot {
        descriptor: adapter.clone(),
        checker: Some(adapter.clone()),
        puller: None,
        invoker: Some(adapter),
    }
}
fn document() -> ConformantDocument {
    Document::parse(
        r#"{
        "openbindings":"0.2.0","operations":{"notice":{}},
        "sources":{
            "memory":{"kind":"example.memory@1","data":{"channel":"local"}},
            "notice":{"kind":"example.notice@1"}},
        "bindings":{"notice":{"source":"notice","operation":"notice"}}
    }"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap()
}

#[tokio::test]
async fn two_kinds_describe_check_pull_preflight_and_invoke() -> Result<(), Box<dyn Error>> {
    let mut host = Registry::default();
    host.register(kind_adapter::KIND, memory_slot()).await?;
    host.register(notice::KIND, notice_slot()).await?;
    assert!(host.can_invoke(kind_adapter::KIND));
    assert!(host.can_invoke(notice::KIND));
    assert!(!host.can_invoke("example.unknown@1"));
    let checked = host
        .get(notice::KIND)
        .unwrap()
        .checker
        .as_ref()
        .unwrap()
        .check(
            CheckSourceRequest::from_json(JsonValue::parse(
                r#"{"kind":"example.notice@1","context":{"approval":true}}"#,
            )?)?,
            CancellationToken::new(),
            DescriptionLimits::default(),
        )
        .await?;
    assert!(checked.value().value().get("context").is_none());
    let (_, document) = host
        .get(kind_adapter::KIND)
        .unwrap()
        .puller
        .as_ref()
        .unwrap()
        .pull(
            PullRequest::new(document(), "memory"),
            CancellationToken::new(),
            PullLimits::default(),
        )
        .await?;
    for (binding, expected) in [("echo.memory", "7"), ("notice", "1")] {
        let request = BindingRequest::new(document.clone(), binding);
        assert!(
            host.preflight(request.clone(), CancellationToken::new())?
                .await?
                .is_none()
        );
        let (mut input, mut output) = host.invoke(request, CancellationToken::new())?.split();
        if binding == "echo.memory" {
            input.send(JsonValue::integer(7)).await?;
        }
        input.close();
        let mut values = Vec::new();
        let mut complete = false;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => values.push(value.text().to_owned()),
                OutputEvent::Complete => complete = true,
                OutputEvent::Error(error) => return Err(error.into()),
                _ => (),
            }
        }
        assert_eq!(values, [expected]);
        assert!(complete);
    }
    Ok(())
}

#[tokio::test]
async fn registration_refuses_missing_roles_and_keeps_native_error_sources() {
    let mut host = Registry::default();
    let mut broken = memory_slot();
    broken.invoker = None;
    assert!(host.register(kind_adapter::KIND, broken).await.is_err());
    assert!(host.get(kind_adapter::KIND).is_none());
    let mut broken = notice_slot();
    broken.checker = None;
    assert!(host.register(notice::KIND, broken).await.is_err());
    let mut broken = memory_slot();
    broken.puller = None;
    assert!(host.register(kind_adapter::KIND, broken).await.is_err());
    host.register(kind_adapter::KIND, memory_slot())
        .await
        .unwrap();
    assert!(
        host.register(kind_adapter::KIND, memory_slot())
            .await
            .is_err()
    );

    let error = host
        .get(kind_adapter::KIND)
        .unwrap()
        .descriptor
        .describe(
            DescribeKindRequest::new("example.unknown@1").unwrap(),
            CancellationToken::new(),
            DescriptionLimits::default(),
        )
        .await
        .unwrap_err();
    // KindError -> KindFault -> the adapter's own error. map_error preserves it.
    let native = error.source().unwrap().source().unwrap();
    assert!(native.downcast_ref::<AdapterError>().is_some());
}

#[test]
fn context_is_scoped_to_key_kind_and_exact_data() {
    let original = document();
    let mut store = context::Contexts::default();
    let source = original.source("memory").unwrap();
    store.remember(
        &source,
        Context::try_from(JsonValue::parse(r#"{"approval":true}"#).unwrap()).unwrap(),
    );
    assert!(store.get(&source).is_some());
    for (path, text) in [
        (vec!["sources", "memory", "kind"], r#""example.notice@1""#),
        (vec!["sources", "memory", "data"], r#"{"channel":"moved"}"#),
        (
            vec!["sources", "memory", "data"],
            r#"{ "channel": "local" }"#,
        ),
        (vec!["sources", "memory", "data"], "null"),
    ] {
        let changed = original
            .document()
            .edit(&[Edit::set(&path, JsonValue::parse(text).unwrap())])
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        assert!(store.get(&changed.source("memory").unwrap()).is_none());
    }
    let changed = original
        .document()
        .edit(&[Edit::set(&["sources", "other"], source.value().to_owned())])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    assert!(store.get(&changed.source("other").unwrap()).is_none());
    let absent = original.source("notice").unwrap();
    store.remember(
        &absent,
        Context::try_from(JsonValue::parse("{}").unwrap()).unwrap(),
    );
    let null = original
        .document()
        .edit(&[Edit::set(
            &["sources", "notice", "data"],
            JsonValue::parse("null").unwrap(),
        )])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    assert!(store.get(&null.source("notice").unwrap()).is_none());
}

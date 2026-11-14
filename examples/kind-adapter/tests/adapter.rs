use kind_adapter::{KIND, Memory};
use openbindings::{ConformantDocument, Document, Edit, JsonValue};
use openbindings_invocation::ContextChallenge;
use openbindings_invocation::{
    BindingInvokerExt, BindingRequest, CancellationToken, InvocationLimits, OutputEvent,
};
use openbindings_kind_description::{DescribeKindRequest, KindDescriptorExt};
use openbindings_source_pull::{
    ExistingStatus, OmittedStatus, Proposal, PullLimits, PullRequest, SourcePullerExt,
};

fn document(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
fn seed() -> ConformantDocument {
    document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}}}"#,
    )
}

// guide:application:start
#[tokio::test]
async fn pull_then_invoke() -> Result<(), Box<dyn std::error::Error>> {
    use kind_adapter::Memory;
    use openbindings::Document;
    use openbindings_invocation::{
        BindingInvokerExt, BindingRequest, CancellationToken, InvocationError, OutputEvent,
    };
    use openbindings_source_pull::{PullRequest, SourcePullerExt};

    let original = Document::parse(
        r#"{
        "openbindings":"0.2.0", "operations":{},
        "sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}}
    }"#,
    )?
    .validate()?
    .conformant()
    .ok_or("nonconformant document")?;
    let adapter = Memory;
    let request = PullRequest::new(original, "s");
    let (result, interface) = adapter
        .pull_source(request.clone(), CancellationToken::new())
        .await?;
    assert!(result.report().is_complete());
    // This application chooses every addition. No stale proposal is applied.
    let call = adapter.invoke(
        BindingRequest::new(interface, "echo.s"),
        CancellationToken::new(),
    )?;
    let (mut input, mut output) = call.split();
    let sending = async {
        input
            .send(openbindings::JsonValue::parse(
                "123456789012345678901234567890",
            )?)
            .await?;
        input.close();
        Ok::<(), Box<dyn std::error::Error>>(())
    };
    let receiving = async {
        let mut count = 0;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => {
                    assert_eq!(value.text(), "123456789012345678901234567890");
                    count += 1;
                }
                OutputEvent::Complete => {
                    assert_eq!(count, 1);
                    return Ok(());
                }
                OutputEvent::Error(error) => return Err(error),
                _ => (),
            }
        }
        Err(InvocationError::transport_closed())
    };
    let (sent, received) = tokio::join!(sending, receiving);
    received?;
    sent?;
    Ok(())
}
// guide:application:end

#[tokio::test]
async fn describe_and_preflight_agree_with_implemented_facets() {
    let adapter = Memory;
    let listed = adapter.list_kinds(CancellationToken::new()).await.unwrap();
    assert_eq!(listed.len(), 1);
    let request = DescribeKindRequest::from_json(
        JsonValue::parse(format!(r#"{{"kind":"{KIND}"}}"#)).unwrap(),
    )
    .unwrap();
    let description = adapter
        .describe_kind(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(description.kind().as_str(), Some(KIND));
    assert_eq!(
        listed[0]
            .supports()
            .map(|key| key.as_str().unwrap())
            .collect::<Vec<_>>(),
        description
            .supports()
            .map(|key| key.as_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(!description.invoke().unwrap().executes());
    assert!(!description.pull().unwrap().executes());
    let request = PullRequest::new(seed(), "s");
    let (_, interface) = adapter
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(matches!(
        adapter
            .preflight(
                BindingRequest::new(interface, "echo.s"),
                CancellationToken::new()
            )
            .unwrap()
            .await,
        Ok(None)
    ));
}

#[tokio::test]
async fn repeat_pull_is_current_and_schema_drift_is_a_removal_proposal() {
    let request = PullRequest::new(seed(), "s");
    let (_, interface) = Memory
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    let (repeat, _) = Memory
        .pull_source(
            PullRequest::new(interface.clone(), "s"),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(repeat.report().added().len(), 0);
    assert_eq!(
        repeat.report().existing().unwrap().next().unwrap().status(),
        ExistingStatus::Current
    );
    let edited = interface
        .document()
        .edit(&[Edit::set(
            &["operations", "echo", "output"],
            JsonValue::parse("true").unwrap(),
        )])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let (drift, _) = Memory
        .pull_source(PullRequest::new(edited, "s"), CancellationToken::new())
        .await
        .unwrap();
    let entry = drift.report().existing().unwrap().next().unwrap();
    assert_eq!(entry.status(), ExistingStatus::Stale);
    let Some(Proposal::Binding(proposal)) = entry.proposal() else {
        panic!("binding proposal")
    };
    assert_eq!(
        proposal.remove().unwrap().next().unwrap().as_str(),
        Some("output")
    );
}

#[tokio::test]
async fn reuse_resolves_aliases_and_conflicts_are_not_reuse() {
    for (operation, reused) in [
        (r#"{"canonical":{"aliases":["echo"]}}"#, true),
        (r#"{"echo":{"output":true}}"#, false),
    ] {
        let interface = seed()
            .document()
            .edit(&[Edit::set(
                &["operations"],
                JsonValue::parse(operation).unwrap(),
            )])
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let (result, _) = Memory
            .pull_source(PullRequest::new(interface, "s"), CancellationToken::new())
            .await
            .unwrap();
        if reused {
            let entry = result.report().added().next().unwrap();
            assert!(entry.reused());
            assert_eq!(entry.operation().as_str(), Some("canonical"));
        } else {
            assert_eq!(result.report().added().len(), 0);
            assert_eq!(
                result.report().omitted().unwrap().next().unwrap().status(),
                OmittedStatus::Conflict
            );
        }
    }
}

#[tokio::test]
async fn malformed_kind_data_missing_binding_cancellation_and_limits() {
    let interface = document(
        r#"{"openbindings":"0.2.0","operations":{"echo":{}},"sources":{"s":{"kind":"other"}},"bindings":{"b":{"operation":"echo","source":"s"}}}"#,
    );
    for (key, code) in [
        ("b", "example.memory.invalid-data"),
        ("missing", "ERR_BINDING_NOT_FOUND"),
    ] {
        let call = Memory
            .invoke(
                BindingRequest::new(interface.clone(), key),
                CancellationToken::new(),
            )
            .unwrap();
        let (_, mut output) = call.split();
        assert!(
            matches!(output.next().await, Some(OutputEvent::Error(error)) if error.code() == code)
        );
    }
    assert!(
        Memory
            .pull_source(
                PullRequest::new(interface.clone(), "s"),
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let call = Memory
        .invoke(BindingRequest::new(interface.clone(), "b"), cancelled)
        .unwrap();
    let (_, mut output) = call.split();
    assert!(
        matches!(output.next().await, Some(OutputEvent::Error(error)) if error.code() == "ERR_CANCELLED")
    );
    let limits = InvocationLimits {
        input_capacity: 0,
        ..Default::default()
    };
    assert!(
        Memory
            .invoke_with_limits(
                BindingRequest::new(interface, "b"),
                CancellationToken::new(),
                limits
            )
            .is_err()
    );
    let limits = PullLimits {
        max_response_bytes: 1,
        ..Default::default()
    };
    assert!(
        Memory
            .pull_source_with_limits(
                PullRequest::new(seed(), "s"),
                CancellationToken::new(),
                limits
            )
            .await
            .is_err()
    );
}

#[test]
fn nested_exact_json_survives_checked_composition() {
    let value = JsonValue::parse("123456789012345678901234567890").unwrap();
    let fields = std::collections::BTreeMap::from([("data", value)]);
    let composed = JsonValue::from_serializable(&fields).unwrap();
    assert_eq!(
        composed.get("data").unwrap().text(),
        "123456789012345678901234567890"
    );
}

#[tokio::test]
async fn streams_past_queue_capacity_in_order() {
    let request = PullRequest::new(seed(), "s");
    let (_, interface) = Memory
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    let call = Memory
        .invoke(
            BindingRequest::new(interface, "echo.s"),
            CancellationToken::new(),
        )
        .unwrap();
    let control = call.control();
    let (mut input, mut output) = call.split();
    let sending = async {
        for n in 0..1000 {
            input
                .send(JsonValue::from_serializable(&n).unwrap())
                .await
                .unwrap();
        }
        input.close();
    };
    let receiving = async {
        let mut count = 0;
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => {
                    assert_eq!(value.text(), count.to_string());
                    count += 1;
                }
                OutputEvent::Complete => {
                    assert_eq!(count, 1000);
                    return;
                }
                OutputEvent::Error(error) => panic!("{error}"),
                _ => (),
            }
        }
        panic!("missing terminal");
    };
    tokio::join!(sending, receiving);
    assert_eq!(control.undelivered_inputs(), Some(0));
}

#[test]
fn local_future_is_dropped_without_async_cleanup_and_send_composition_remains_available() {
    use openbindings_invocation::{BindingInvoker, InvocationError, ProviderIo};
    use std::{
        cell::Cell,
        future::{Future, Ready, ready},
        pin::Pin,
        rc::Rc,
        task::{Context, Poll, Waker},
    };

    struct Local {
        dropped: Rc<Cell<bool>>,
        resumed: Rc<Cell<bool>>,
    }
    struct Guard(Rc<Cell<bool>>, CancellationToken);
    impl Drop for Guard {
        fn drop(&mut self) {
            assert!(self.1.is_cancelled());
            self.0.set(true);
        }
    }
    impl BindingInvoker for Local {
        type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
        type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
        fn handle_invoke<'a>(
            &'a self,
            _: BindingRequest,
            io: ProviderIo,
            control: CancellationToken,
        ) -> Self::Invoke<'a> {
            Box::pin(async move {
                let _io = io;
                let _guard = Guard(self.dropped.clone(), control.clone());
                control.cancelled().await;
                self.resumed.set(true);
                Ok(())
            })
        }
        fn handle_preflight<'a>(
            &'a self,
            _: BindingRequest,
            _: CancellationToken,
        ) -> Self::Preflight<'a> {
            ready(Ok(None))
        }
    }
    let interface = document(
        r#"{"openbindings":"0.2.0","operations":{"echo":{}},"sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}},"bindings":{"b":{"operation":"echo","source":"s","data":{"mode":"echo"}}}}"#,
    );
    let local = Local {
        dropped: Rc::new(Cell::new(false)),
        resumed: Rc::new(Cell::new(false)),
    };
    let external = CancellationToken::new();
    let call = local
        .invoke(
            BindingRequest::new(interface.clone(), "b"),
            external.clone(),
        )
        .unwrap();
    let (_, mut output) = call.split();
    let mut cx = Context::from_waker(Waker::noop());
    let mut next = Box::pin(output.next());
    assert!(next.as_mut().poll(&mut cx).is_pending());
    drop(next);
    external.cancel();
    let mut next = Box::pin(output.next());
    assert!(
        matches!(next.as_mut().poll(&mut cx), Poll::Ready(Some(OutputEvent::Error(error))) if error.code() == "ERR_CANCELLED")
    );
    drop(next);
    assert!(local.dropped.get());
    assert!(!local.resumed.get());

    fn require_send<T: Send>(_: T) {}
    require_send(
        Memory
            .invoke(
                BindingRequest::new(interface, "b"),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    require_send(Memory.pull_source(PullRequest::new(seed(), "s"), CancellationToken::new()));
}

#[tokio::test]
async fn object_schema_comparison_gaps_are_explicit() {
    for bound in [false, true] {
        let mut draft = seed()
            .document()
            .edit(&[Edit::set(
                &["operations"],
                JsonValue::parse(r##"{"echo":{"output":{"$ref":"#/operations/echo/output"}}}"##)
                    .unwrap(),
            )])
            .unwrap();
        if bound {
            draft = draft
                .edit(&[Edit::set(
                    &["bindings"],
                    JsonValue::parse(
                        r#"{"b":{"operation":"echo","source":"s","data":{"mode":"echo"}}}"#,
                    )
                    .unwrap(),
                )])
                .unwrap();
        }
        let interface = draft.validate().unwrap().conformant().unwrap();
        let (result, _) = Memory
            .pull_source(PullRequest::new(interface, "s"), CancellationToken::new())
            .await
            .unwrap();
        assert!(!result.report().is_complete());
        assert_eq!(result.report().added().len(), 0);
        if bound {
            let entry = result.report().existing().unwrap().next().unwrap();
            assert_eq!(entry.status(), ExistingStatus::Unassessed);
            assert!(entry.proposal().is_none());
        } else {
            assert_eq!(
                result.report().omitted().unwrap().next().unwrap().status(),
                OmittedStatus::ImplementationUnsupported
            );
        }
    }
}

// guide:agreement:start
#[tokio::test]
async fn routing_agrees_with_published_invocation_support() {
    use kind_adapter::{KIND, Memory};
    use openbindings_invocation::{CancellationToken, InvocationSupport};
    use openbindings_kind_description::{DescribeKindRequest, KindDescriptorExt};

    const INVOKE: &str = "openbindings.binding-invoker.invokeBinding";
    let adapter = Memory; // One configuration for all three queries.
    let kinds = adapter.list_kinds(CancellationToken::new()).await.unwrap();
    // Include the adapter's known routing kind even if a broken listing omits it,
    // and a negative case outside its inventory.
    for kind in kinds
        .iter()
        .map(|summary| summary.kind().as_str().unwrap())
        .chain([KIND, "example.unknown@1"])
    {
        let listed = kinds.iter().any(|summary| {
            summary.kind().as_str() == Some(kind)
                && summary.supports().any(|operation| operation == INVOKE)
        });
        let description = adapter
            .describe_kind(
                DescribeKindRequest::new(kind).unwrap(),
                CancellationToken::new(),
            )
            .await;
        let described = match description {
            Ok(description) => {
                assert_eq!(description.kind().as_str(), Some(kind));
                description.supports().any(|operation| operation == INVOKE)
            }
            Err(_) => {
                assert_eq!(kind, "example.unknown@1");
                false
            }
        };
        assert_eq!(adapter.can_invoke(kind), listed);
        assert_eq!(adapter.can_invoke(kind), described);
    }
}
// guide:agreement:end

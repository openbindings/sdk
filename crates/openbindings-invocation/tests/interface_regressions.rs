//! Current-interface regressions preserve behavior while replacing obsolete APIs.
use openbindings::{ConformantDocument, Document, ExternalSchemas, JsonValue};
use openbindings_invocation::{
    BindingClient, BindingInvoker, BindingRequest, CancellationToken, ContextChallenge,
    InvocationError, InvocationSupport, OperationClient, OperationRequest, OutputEvent,
    PreflightOutcome, ProviderIo, ProviderOutput,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    future::{Future, Ready, ready},
    rc::Rc,
    sync::{Arc, Mutex},
};
fn doc(operation: &str) -> ConformantDocument {
    Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"o":{operation}}},"sources":{{"s":{{"kind":"fixture"}}}},"bindings":{{"b":{{"operation":"o","source":"s"}}}}}}"#))
        .unwrap().validate().unwrap().conformant().unwrap()
}
fn request() -> BindingRequest {
    BindingRequest::new(doc("{}"), "b")
}
fn cancel() -> CancellationToken {
    CancellationToken::new()
}
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn terminal(event: Option<OutputEvent>) -> InvocationError {
    match event.unwrap() {
        OutputEvent::Error(error) => error,
        other => panic!("{other:?}"),
    }
}
struct Provider<'a> {
    code: Option<&'a str>,
    output: Arc<Mutex<Option<ProviderOutput>>>,
    hooks: Arc<std::sync::atomic::AtomicUsize>,
}
impl<'a> Provider<'a> {
    fn new(code: Option<&'a str>) -> Self {
        Self {
            code,
            output: Arc::default(),
            hooks: Arc::default(),
        }
    }
}
impl BindingInvoker for Provider<'_> {
    type Invoke<'a>
        = Ready<Result<(), InvocationError>>
    where
        Self: 'a;
    type Preflight<'a>
        = Ready<Result<PreflightOutcome, InvocationError>>
    where
        Self: 'a;
    fn invoke<'a>(
        &'a self,
        _: BindingRequest,
        io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.hooks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (_, output) = io.split();
        *self.output.lock().unwrap() = Some(output);
        ready(self.code.map_or(Ok(()), |code| {
            Err(InvocationError::new(code, None).unwrap())
        }))
    }
    fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        self.hooks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ready(
            self.code.map_or(
                Ok(PreflightOutcome::Challenge(
                    ContextChallenge::try_from(json(r#"{"message":"Context needed","schema":{}}"#))
                        .unwrap(),
                )),
                |code| Err(InvocationError::new(code, None).unwrap()),
            ),
        )
    }
}
impl InvocationSupport for Provider<'_> {
    fn can_invoke(&self, _: &str) -> bool {
        false
    }
}
fn send_preflight<'a, P: BindingInvoker + Sync>(
    client: &'a BindingClient<P>,
    request: BindingRequest,
) -> impl Future<Output = Result<PreflightOutcome, InvocationError>> + Send + 'a
where
    P::Preflight<'a>: Send,
{
    client.preflight(request, cancel()).unwrap()
}
fn send_invocation<'a, P: BindingInvoker + Sync>(
    client: &'a BindingClient<P>,
    request: BindingRequest,
) -> openbindings_invocation::Invocation<
    impl Future<Output = Result<(), InvocationError>> + Send + 'a,
>
where
    P::Invoke<'a>: Send,
{
    client.invoke(request, cancel()).unwrap()
}
#[tokio::test]
async fn borrowed_generic_provider_futures_are_send_without_static_configuration() {
    let configured_code = String::from("example.borrowed");
    let client = BindingClient::new(Provider::new(Some(&configured_code)));
    assert_eq!(
        send_preflight(&client, request()).await.unwrap_err().code(),
        configured_code
    );
    let (_, mut output) = send_invocation(&client, request()).split();
    assert_eq!(terminal(output.next().await).code(), configured_code);
}
async fn check_forwarding<P: BindingInvoker + InvocationSupport>(
    provider: P,
    hooks: Arc<std::sync::atomic::AtomicUsize>,
) {
    let expected = r#"{"message":"Context needed","schema":{}}"#;
    let binding = BindingClient::new(&provider);
    let PreflightOutcome::Challenge(challenge) = binding
        .preflight(request(), cancel())
        .unwrap()
        .await
        .unwrap()
    else {
        panic!("prediction")
    };
    assert_eq!(challenge.value().value().text(), expected);
    let (_, mut output) = binding.invoke(request(), cancel()).unwrap().split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    drop(output);
    let operation = OperationClient::new(
        provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let PreflightOutcome::Challenge(challenge) = operation
        .preflight(OperationRequest::binding(doc("{}"), "b"), cancel())
        .unwrap()
        .await
        .unwrap()
    else {
        panic!("prediction")
    };
    assert_eq!(challenge.value().value().text(), expected);
    let (_, mut output) = operation
        .invoke(OperationRequest::binding(doc("{}"), "b"), cancel())
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert_eq!(hooks.load(std::sync::atomic::Ordering::SeqCst), 4);
}
#[tokio::test]
async fn pointer_forwarding_preserves_separate_preflight_hooks() {
    let provider = Provider::new(None);
    let hooks = provider.hooks.clone();
    check_forwarding(Rc::new(provider), hooks).await;
    let provider = Provider::new(None);
    let hooks = provider.hooks.clone();
    check_forwarding(Arc::new(provider), hooks).await;
    let provider = Provider::new(None);
    let hooks = provider.hooks.clone();
    check_forwarding(Box::new(provider), hooks).await;
    let provider = Provider::new(None);
    let hooks = provider.hooks.clone();
    check_forwarding(&provider, hooks).await;
}
#[tokio::test]
async fn unsupported_ordered_selection_is_not_found_but_explicit_binding_is_attempted() {
    let provider = Provider::new(None);
    let client = OperationClient::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let (_, mut output) = client
        .invoke(
            OperationRequest::ordered(doc("{}"), "o", vec!["b".into()]),
            cancel(),
        )
        .unwrap()
        .split();
    assert_eq!(
        terminal(output.next().await).code(),
        "ERR_BINDING_NOT_FOUND"
    );
    let (_, mut output) = client
        .invoke(OperationRequest::binding(doc("{}"), "b"), cancel())
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
}
#[tokio::test]
async fn independent_provider_output_after_settlement_reports_the_terminal() {
    let provider = Provider::new(Some("ERR_REFUSED"));
    let client = BindingClient::new(&provider);
    let (_, mut output) = client.invoke(request(), cancel()).unwrap().split();
    assert_eq!(terminal(output.next().await).code(), "ERR_REFUSED");
    let mut sender = provider.output.lock().unwrap().take().unwrap();
    assert_eq!(
        sender.send(json("1")).await.unwrap_err().code(),
        "ERR_REFUSED"
    );
}
#[tokio::test]
async fn only_current_operation_owned_codes_are_reserved_at_provider_boundary() {
    for code in [
        "ERR_OPERATION_NOT_FOUND",
        "ERR_BINDING_SELECTION_REQUIRED",
        "ERR_OPERATION_VALIDATION_FAILED",
        "ERR_SCHEMA_UNRESOLVED",
    ] {
        let provider = Provider::new(Some(code));
        let binding = BindingClient::new(&provider);
        let operation = OperationClient::new(
            &provider,
            DefaultEvaluator::default(),
            ExternalSchemas::default(),
        );
        let call = binding.invoke(request(), cancel()).unwrap();
        let evidence = call.control();
        let (_, mut output) = call.split();
        assert_eq!(terminal(output.next().await).code(), "ERR_EXECUTION_FAILED");
        assert_eq!(
            evidence.boundary_failure().unwrap().kind(),
            openbindings_invocation::BoundaryFailureKind::ReservedProviderCode
        );
        let (_, mut output) = operation
            .invoke(OperationRequest::binding(doc("{}"), "b"), cancel())
            .unwrap()
            .split();
        assert_eq!(terminal(output.next().await).code(), "ERR_EXECUTION_FAILED");
        assert_eq!(
            binding
                .preflight(request(), cancel())
                .unwrap()
                .await
                .unwrap_err()
                .code(),
            "ERR_EXECUTION_FAILED"
        );
        assert_eq!(
            operation
                .preflight(OperationRequest::binding(doc("{}"), "b"), cancel())
                .unwrap()
                .await
                .unwrap_err()
                .code(),
            "ERR_EXECUTION_FAILED"
        );
    }
    for code in [
        "ERR_BINDING_NOT_FOUND",
        "ERR_UNKNOWN_SOURCE",
        "example.application",
    ] {
        let client = BindingClient::new(Provider::new(Some(code)));
        let (_, mut output) = client.invoke(request(), cancel()).unwrap().split();
        assert_eq!(terminal(output.next().await).code(), code);
        assert_eq!(
            client
                .preflight(request(), cancel())
                .unwrap()
                .await
                .unwrap_err()
                .code(),
            code
        );
    }
}
#[test]
fn display_shows_every_code_and_debug_shows_only_governed_codes() {
    let governed = InvocationError::new(
        "ERR_OPERATION_VALIDATION_FAILED",
        Some(JsonValue::parse(r#""secret-data""#).unwrap()),
    )
    .unwrap();
    for text in [governed.to_string(), format!("{governed:?}")] {
        assert!(text.contains("ERR_OPERATION_VALIDATION_FAILED"));
        assert!(!text.contains("secret-data"));
    }
    let application = InvocationError::new("ACME_QUOTA", None).unwrap();
    assert!(application.to_string().contains("ACME_QUOTA"));
    assert_eq!(
        format!("{application:?}"),
        r#"InvocationError { code: "<redacted>", data_present: false }"#
    );
}
#[test]
fn invocation_writer_preserves_exact_values_and_byte_limits() {
    let error = InvocationError::new(
        "example.application",
        Some(JsonValue::parse("9007199254740993").unwrap()),
    )
    .unwrap();
    let wire = error.to_json().unwrap();
    assert_eq!(
        wire.get("data").unwrap().number_text(),
        Some("9007199254740993")
    );
    assert!(
        error
            .to_json_with_limits(openbindings::JsonLimits {
                max_bytes: wire.bytes().len(),
                ..Default::default()
            })
            .is_ok()
    );
    assert!(
        error
            .to_json_with_limits(openbindings::JsonLimits {
                max_bytes: wire.bytes().len() - 1,
                ..Default::default()
            })
            .is_err()
    );
}

#[tokio::test]
async fn explain_detail_covers_both_directions_and_names_value_members() {
    use openbindings::{Detail, ValueOutcome};
    use openbindings_invocation::operation::{Direction, ValidationEvidence};
    struct EmitMember;
    impl InvocationSupport for EmitMember {
        fn can_invoke(&self, _: &str) -> bool {
            true
        }
    }
    impl BindingInvoker for EmitMember {
        type Invoke<'a> = std::pin::Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
        type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
        fn invoke<'a>(
            &'a self,
            _: BindingRequest,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invoke<'a> {
            Box::pin(async move {
                while io.next().await.is_some() {}
                io.send(json(r#"{"provider-member":7}"#)).await
            })
        }
        fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
            ready(Ok(PreflightOutcome::NoPrediction))
        }
    }
    let strings = r#"{"type":"object","additionalProperties":{"type":"string"}}"#;
    for (detail, explained) in [(Detail::Outcome, false), (Detail::Explain, true)] {
        for (direction, operation, member) in [
            (
                Direction::Input,
                format!(r#"{{"input":{strings}}}"#),
                "/caller-member",
            ),
            (
                Direction::Output,
                format!(r#"{{"output":{strings}}}"#),
                "/provider-member",
            ),
        ] {
            let client = OperationClient::new(
                EmitMember,
                DefaultEvaluator::default(),
                ExternalSchemas::default(),
            )
            .with_validation_detail(detail);
            let call = client
                .invoke(OperationRequest::binding(doc(&operation), "b"), cancel())
                .unwrap();
            let control = call.control();
            let (mut input, mut output) = call.split();
            input.send(json(r#"{"caller-member":7}"#)).await.unwrap();
            input.close();
            assert_eq!(
                terminal(output.next().await).code(),
                "ERR_OPERATION_VALIDATION_FAILED"
            );
            let failure = control.validation_failure().unwrap();
            assert_eq!(failure.direction(), direction);
            let ValidationEvidence::Evaluation(ValueOutcome::Fails { problems, .. }) =
                failure.evidence()
            else {
                panic!("established failure")
            };
            let instances: Vec<_> = problems
                .iter()
                .map(|problem| problem.instance.to_string())
                .collect();
            if explained {
                assert_eq!(instances, [member]);
            } else {
                assert!(instances.is_empty());
            }
        }
    }
}

struct HoldOutput(Arc<Mutex<Option<ProviderOutput>>>);
impl InvocationSupport for HoldOutput {
    fn can_invoke(&self, _: &str) -> bool {
        true
    }
}
impl BindingInvoker for HoldOutput {
    type Invoke<'a> = std::pin::Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn invoke<'a>(
        &'a self,
        _: BindingRequest,
        io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        let (input, output) = io.split();
        *self.0.lock().unwrap() = Some(output);
        Box::pin(async move {
            let _held = input;
            std::future::pending().await
        })
    }
    fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction))
    }
}
fn post_terminal_precedence<F: Future<Output = Result<(), InvocationError>>>(
    call: openbindings_invocation::Invocation<F>,
    published: &Mutex<Option<ProviderOutput>>,
    first: JsonValue,
    want: &str,
) {
    use std::{
        pin::pin,
        task::{Context, Poll, Waker},
    };
    let mut cx = Context::from_waker(Waker::noop());
    let control = call.control();
    let (_, mut output) = call.split();
    assert!(pin!(output.next()).as_mut().poll(&mut cx).is_pending());
    let mut sender = published.lock().unwrap().take().unwrap();
    {
        let mut send = pin!(sender.send(first));
        assert!(send.as_mut().poll(&mut cx).is_pending());
        let Poll::Ready(event) = pin!(output.next()).as_mut().poll(&mut cx) else {
            panic!("terminal")
        };
        assert_eq!(terminal(event).code(), want);
        assert!(
            matches!(send.as_mut().poll(&mut cx),Poll::Ready(Err(error)) if error.code()==want)
        );
    }
    for late in [json("1"), json(r#""oversized after terminal""#)] {
        assert!(
            matches!(pin!(sender.send(late)).as_mut().poll(&mut cx),Poll::Ready(Err(error)) if error.code()==want)
        );
    }
    control.cancel();
    assert_eq!(control.outcome().unwrap().unwrap_err().code(), want);
}
#[test]
fn established_validation_or_size_failure_wins_over_later_oversized_output() {
    let published = Arc::new(Mutex::new(None));
    let client = OperationClient::new(
        HoldOutput(published.clone()),
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    )
    .with_limits(openbindings_invocation::InvocationLimits {
        max_value_bytes: Some(1),
        ..Default::default()
    })
    .unwrap();
    post_terminal_precedence(
        client
            .invoke(
                OperationRequest::binding(doc(r#"{"output":false}"#), "b"),
                cancel(),
            )
            .unwrap(),
        &published,
        json("1"),
        "ERR_OPERATION_VALIDATION_FAILED",
    );
    let published = Arc::new(Mutex::new(None));
    let client = BindingClient::new(HoldOutput(published.clone()))
        .with_limits(openbindings_invocation::InvocationLimits {
            max_value_bytes: Some(1),
            ..Default::default()
        })
        .unwrap();
    post_terminal_precedence(
        client.invoke(request(), cancel()).unwrap(),
        &published,
        json("null"),
        "ERR_EXECUTION_FAILED",
    );
}

//! Current pinned invocation semantics, independent of removed legacy facades.
use openbindings::{
    ConformantDocument, Detail, Document, EvaluationRequest, ExternalSchemas, JsonValue,
    PreparationFailure, PreparationFailureKind, PreparationLimits, SchemaEvaluator,
    SchemaValidator, ValueOutcome, WorkControl,
};
use openbindings_invocation::{
    BindingInvoker, BindingInvokerExt, BindingRequest, BoundaryFailureKind, CancellationToken,
    Context, ContextChallenge, DefaultOperationInvoker, InvocationError, InvocationSupport,
    OperationRequest, OutputEvent, ProviderIo,
    operation::{Direction, ValidationEvidence},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    cell::Cell,
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context as TaskContext, Poll, Waker},
};
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document(operation: &str) -> ConformantDocument {
    Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"echo":{operation},"other":{{}}}},"sources":{{"s":{{"kind":"fixture","data":null}}}},"bindings":{{"a":{{"operation":"echo","source":"s","data":1e9999}},"b":{{"operation":"echo","source":"s"}},"other":{{"operation":"other","source":"s"}}}}}}"#))
        .unwrap().validate().unwrap().conformant().unwrap()
}
#[derive(Clone, Copy)]
enum Mode {
    Echo,
    Empty,
    Outputs,
    Challenge,
    Error,
    Park,
}
struct Provider {
    mode: Mode,
    supported: bool,
    calls: AtomicUsize,
    queries: AtomicUsize,
    reads: AtomicUsize,
    eof: AtomicUsize,
    preflights: AtomicUsize,
    seen: Mutex<Option<BindingRequest>>,
}
impl Provider {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            supported: true,
            calls: 0.into(),
            queries: 0.into(),
            reads: 0.into(),
            eof: 0.into(),
            preflights: 0.into(),
            seen: Mutex::new(None),
        }
    }
}
fn challenge() -> ContextChallenge {
    ContextChallenge::try_from(json(
        r#"{"message":"Supply a token","schema":{"type":"object","required":["token"]}}"#,
    ))
    .unwrap()
}
impl InvocationSupport for Provider {
    fn can_invoke(&self, kind: &str) -> bool {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.supported && kind == "fixture"
    }
}
impl BindingInvoker for Provider {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request);
        Box::pin(async move {
            let (mut input, mut output) = io.split();
            match self.mode {
                Mode::Empty => (),
                Mode::Park => std::future::pending::<()>().await,
                Mode::Challenge => return Err(InvocationError::challenge(challenge())),
                Mode::Error => {
                    return Err(InvocationError::new("fixture.error", Some(json("null"))).unwrap());
                }
                Mode::Outputs => {
                    output
                        .send(json("1"))
                        .await
                        .map_err(|_| InvocationError::execution_failed())?;
                    output
                        .send(json(r#""bad""#))
                        .await
                        .map_err(|_| InvocationError::execution_failed())?;
                }
                Mode::Echo => {
                    while let Some(value) = super::receive(&mut input).await {
                        self.reads.fetch_add(1, Ordering::Relaxed);
                        output
                            .send(value)
                            .await
                            .map_err(|_| InvocationError::execution_failed())?;
                    }
                    self.eof.fetch_add(1, Ordering::Relaxed);
                }
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        self.preflights.fetch_add(1, Ordering::Relaxed);
        *self.seen.lock().unwrap() = Some(request);
        ready(Ok(if matches!(self.mode, Mode::Challenge) {
            Some(challenge())
        } else {
            None
        }))
    }
}
fn control() -> CancellationToken {
    CancellationToken::new()
}
fn code(event: Option<OutputEvent>) -> String {
    match event.unwrap() {
        OutputEvent::Error(error) => error.code().to_owned(),
        other => panic!("expected terminal, got {other:?}"),
    }
}
#[tokio::test]
async fn binding_is_lazy_owned_exact_and_descriptorless() {
    let provider = Provider::new(Mode::Echo);
    let client = &provider;
    let context = Context::try_from(json(
        r#"{"opaque":{"x":1,"x":2},"number":1e9999,"text":"\ud800"}"#,
    ))
    .unwrap();
    let request = BindingRequest::new(document("{}"), "a").with_context(context.clone());
    let expected = request.to_json().unwrap();
    let (mut input, mut output) = client.invoke(request, control()).unwrap().split();
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    input.send(json("9007199254740993")).await.unwrap();
    input.close();
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    match output.next().await.unwrap() {
        OutputEvent::Output(value) => assert_eq!(value.text(), "9007199254740993"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert!(output.next().await.is_none());
    assert_eq!(
        provider
            .seen
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .to_json()
            .unwrap()
            .text(),
        expected.text()
    );
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn missing_binding_is_terminal_but_empty_key_is_local() {
    let provider = Provider::new(Mode::Empty);
    let client = &provider;
    assert_eq!(
        client
            .invoke(BindingRequest::new(document("{}"), ""), control())
            .unwrap_err()
            .kind(),
        BoundaryFailureKind::EmptyString
    );
    let (_, mut output) = client
        .invoke(BindingRequest::new(document("{}"), "missing"), control())
        .unwrap()
        .split();
    assert_eq!(code(output.next().await), "ERR_BINDING_NOT_FOUND");
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn explicit_operation_binding_bypasses_selection_support() {
    let mut provider = Provider::new(Mode::Empty);
    provider.supported = false;
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let request = OperationRequest::binding(document("{}"), "a");
    let selected = client.select(&request.interface, &request.address).unwrap();
    let (_, mut output) = client.invoke(request, control()).unwrap().split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert_eq!(
        provider.seen.lock().unwrap().as_ref().unwrap().binding,
        selected.key()
    );
    let prepared = client.prepare(document("{}"), "echo", &control()).unwrap();
    let choice = openbindings_invocation::operation::BindingChoice::Binding("b".into());
    let selected = prepared.select(&provider, &choice).unwrap();
    let (_, mut output) = prepared
        .invoke(&provider, choice, None, control())
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert_eq!(
        provider.seen.lock().unwrap().as_ref().unwrap().binding,
        selected.key()
    );
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn all_selection_keys_are_checked_before_any_support_query() {
    let provider = Provider::new(Mode::Empty);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    for bad in ["missing", "other"] {
        let request =
            OperationRequest::ordered(document("{}"), "echo", vec!["a".into(), bad.into()]);
        assert_eq!(
            client
                .select(&request.interface, &request.address)
                .unwrap_err()
                .code(),
            "ERR_BINDING_NOT_FOUND"
        );
        let (_, mut output) = client.invoke(request, control()).unwrap().split();
        assert_eq!(code(output.next().await), "ERR_BINDING_NOT_FOUND");
    }
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn aliases_ordered_selection_and_ambiguity_follow_caller_policy() {
    let provider = Provider::new(Mode::Empty);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let doc = document(r#"{"aliases":["alias"]}"#);
    let request = OperationRequest::operation(doc.clone(), "alias");
    assert_eq!(
        client
            .select(&request.interface, &request.address)
            .unwrap_err()
            .code(),
        "ERR_BINDING_SELECTION_REQUIRED"
    );
    let (_, mut output) = client.invoke(request, control()).unwrap().split();
    assert_eq!(code(output.next().await), "ERR_BINDING_SELECTION_REQUIRED");
    let request = OperationRequest::ordered(doc, "alias", vec!["b".into(), "a".into()]);
    let selected = client.select(&request.interface, &request.address).unwrap();
    assert_eq!(selected.operation().key(), "echo");
    let (_, mut output) = client.invoke(request, control()).unwrap().split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert_eq!(provider.seen.lock().unwrap().as_ref().unwrap().binding, "b");
    assert_eq!(selected.key(), "b");
    let before_calls = provider.calls.load(Ordering::Relaxed);
    let before_queries = provider.queries.load(Ordering::Relaxed);
    let request = OperationRequest::ordered(document("{}"), "absent", vec!["missing".into()]);
    assert_eq!(
        client
            .select(&request.interface, &request.address)
            .unwrap_err()
            .code(),
        "ERR_OPERATION_NOT_FOUND"
    );
    let (_, mut missing) = client.invoke(request, control()).unwrap().split();
    assert_eq!(code(missing.next().await), "ERR_OPERATION_NOT_FOUND");
    assert_eq!(provider.calls.load(Ordering::Relaxed), before_calls);
    assert_eq!(provider.queries.load(Ordering::Relaxed), before_queries);
    let (_, mut duplicate) = client
        .invoke(
            OperationRequest::ordered(document("{}"), "echo", vec!["a".into(), "a".into()]),
            control(),
        )
        .unwrap()
        .split();
    assert!(matches!(
        duplicate.next().await,
        Some(OutputEvent::Complete)
    ));
    assert_eq!(provider.seen.lock().unwrap().as_ref().unwrap().binding, "a");
}
#[tokio::test]
async fn input_is_checked_before_delivery_and_rejection_is_not_eof() {
    let provider = Provider::new(Mode::Echo);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let call = client
        .invoke(
            OperationRequest::binding(document(r#"{"input":{"type":"integer"}}"#), "a"),
            control(),
        )
        .unwrap();
    let evidence = call.control();
    let (mut input, mut output) = call.split();
    input.send(json(r#""bad""#)).await.unwrap();
    input.close();
    assert_eq!(code(output.next().await), "ERR_OPERATION_VALIDATION_FAILED");
    assert_eq!(provider.reads.load(Ordering::Relaxed), 0);
    assert_eq!(provider.eof.load(Ordering::Relaxed), 0);
    let failure = evidence.validation_failure().unwrap();
    assert_eq!(failure.direction(), Direction::Input);
    assert!(matches!(
        failure.evidence(),
        ValidationEvidence::Evaluation(ValueOutcome::Fails { .. })
    ));
}
#[tokio::test]
async fn valid_output_precedes_rejected_output_terminal() {
    let provider = Provider::new(Mode::Outputs);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let (_, mut output) = client
        .invoke(
            OperationRequest::binding(document(r#"{"output":{"type":"integer"}}"#), "a"),
            control(),
        )
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Output(value)) if value.text() == "1"));
    assert_eq!(code(output.next().await), "ERR_OPERATION_VALIDATION_FAILED");
    assert!(output.next().await.is_none());
}
struct LocalEvaluator {
    preparations: Rc<Cell<usize>>,
    evaluations: Rc<Cell<usize>>,
    refusal: bool,
}
#[derive(Clone)]
struct LocalValidator(Rc<Cell<usize>>);
impl SchemaEvaluator for LocalEvaluator {
    type Validator = LocalValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        self.preparations.set(self.preparations.get() + 1);
        if self.refusal {
            Err(PreparationFailure::new(
                PreparationFailureKind::UnsupportedCapability {
                    capability: "test".into(),
                },
            ))
        } else {
            Ok(LocalValidator(self.evaluations.clone()))
        }
    }
}
impl SchemaValidator for LocalValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        self.0.set(self.0.get() + 1);
        ValueOutcome::Satisfies
    }
}
fn evaluator(refusal: bool) -> (LocalEvaluator, Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let prep = Rc::new(Cell::new(0));
    let eval = Rc::new(Cell::new(0));
    (
        LocalEvaluator {
            preparations: prep.clone(),
            evaluations: eval.clone(),
            refusal,
        },
        prep,
        eval,
    )
}
#[test]
fn selection_requires_only_support_and_does_no_schema_or_dispatch_work() {
    use openbindings_invocation::{OperationAddress, operation::BindingChoice};

    // This signature protects select's bounds independently of construction.
    fn select<P: InvocationSupport, E>(
        client: &DefaultOperationInvoker<P, E>,
        interface: &ConformantDocument,
        address: &OperationAddress,
    ) -> Result<openbindings::Binding, InvocationError> {
        client.select(interface, address)
    }
    struct SupportOnly;
    impl InvocationSupport for SupportOnly {
        fn can_invoke(&self, kind: &str) -> bool {
            kind == "fixture"
        }
    }

    let provider = Provider::new(Mode::Empty);
    let (eval, preparations, evaluations) = evaluator(true);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    let interface = document(r#"{"input":{"$ref":"urn:missing"},"output":false}"#);
    let binding = select(
        &client,
        &interface,
        &OperationAddress::Operation {
            name: "echo".into(),
            selection: Some(vec!["b".into(), "a".into()]),
        },
    )
    .unwrap();
    assert_eq!(binding.key(), "b");
    assert_eq!(binding.source().key(), "s");
    assert_eq!(binding.operation().key(), "echo");

    let prepared = client.prepare(document("{}"), "other", &control()).unwrap();
    assert_eq!(
        prepared
            .select(&SupportOnly, &BindingChoice::Sole)
            .unwrap()
            .key(),
        "other"
    );
    assert_eq!(preparations.get(), 0);
    assert_eq!(evaluations.get(), 0);
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    assert_eq!(provider.preflights.load(Ordering::Relaxed), 0);
}
#[test]
fn selection_refusals_are_code_only_and_do_not_disclose_addresses() {
    use openbindings_invocation::{OperationAddress, operation::BindingChoice};
    let provider = Provider::new(Mode::Empty);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let interface = document(r#"{"aliases":["SELECTION_PRIVATE_ALIAS"]}"#);
    let prepared = client
        .prepare(interface.clone(), "echo", &control())
        .unwrap();
    let mut errors = Vec::new();
    for address in [
        OperationAddress::Operation {
            name: "SELECTION_PRIVATE_MISSING".into(),
            selection: None,
        },
        OperationAddress::Binding("SELECTION_PRIVATE_BINDING".into()),
        OperationAddress::Operation {
            name: "echo".into(),
            selection: Some(vec!["a".into(), "SELECTION_PRIVATE_KEY".into()]),
        },
        OperationAddress::Operation {
            name: "SELECTION_PRIVATE_ALIAS".into(),
            selection: None,
        },
    ] {
        errors.push(client.select(&interface, &address).unwrap_err());
    }
    errors.push(
        prepared
            .select(
                &provider,
                &BindingChoice::Binding("SELECTION_PRIVATE_BINDING".into()),
            )
            .unwrap_err(),
    );
    for error in errors {
        assert!(error.data().is_none());
        for rendered in [
            format!("{error}"),
            format!("{error:#}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            assert!(!rendered.contains("SELECTION_PRIVATE"));
        }
    }
}
#[tokio::test]
async fn local_typed_validators_prepare_once_per_declared_direction() {
    let provider = Provider::new(Mode::Echo);
    let (eval, preparations, evaluations) = evaluator(false);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    for attempt in 1..=2 {
        let (mut input, mut output) = client
            .invoke(
                OperationRequest::binding(document(r#"{"input":true,"output":true}"#), "a"),
                control(),
            )
            .unwrap()
            .split();
        let writer = async {
            for _ in 0..4 {
                input.send(json("1")).await.unwrap();
            }
            input.close();
        };
        let reader = async {
            let mut count = 0;
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::Output(_) => count += 1,
                    OutputEvent::Complete => (),
                    other => panic!("{other:?}"),
                }
            }
            assert_eq!(count, 4);
        };
        tokio::join!(writer, reader);
        assert_eq!(preparations.get(), attempt * 2);
        assert_eq!(evaluations.get(), attempt * 8);
    }
}
#[tokio::test]
async fn preparation_refusal_keeps_preparation_evidence_without_value_verdict() {
    let provider = Provider::new(Mode::Echo);
    let (eval, preparations, evaluations) = evaluator(true);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    let call = client
        .invoke(
            OperationRequest::binding(document(r#"{"input":true}"#), "a"),
            control(),
        )
        .unwrap();
    let evidence = call.control();
    let (mut input, mut output) = call.split();
    input.send(json("1")).await.unwrap();
    assert_eq!(code(output.next().await), "ERR_SCHEMA_UNRESOLVED");
    assert!(matches!(
        evidence.validation_failure().unwrap().evidence(),
        ValidationEvidence::Preparation(_)
    ));
    assert_eq!(preparations.get(), 1);
    assert_eq!(evaluations.get(), 0);
}
fn require_send<T: Send>(_: &T) {}
#[tokio::test]
async fn preflight_is_send_with_a_non_send_evaluator_and_never_prepares() {
    let provider = Provider::new(Mode::Challenge);
    let (eval, preparations, evaluations) = evaluator(true);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    let future = client
        .preflight(
            OperationRequest::binding(document(r#"{"input":false,"output":false}"#), "a"),
            control(),
        )
        .unwrap();
    require_send(&future);
    assert!(future.await.unwrap().is_some());
    assert_eq!(preparations.get(), 0);
    assert_eq!(evaluations.get(), 0);
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn absent_contracts_and_error_data_are_not_validated() {
    for mode in [Mode::Echo, Mode::Error, Mode::Challenge] {
        let provider = Provider::new(mode);
        let (eval, preparations, evaluations) = evaluator(true);
        let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
        let doc = if matches!(mode, Mode::Echo) {
            document("{}")
        } else {
            document(r#"{"output":false}"#)
        };
        let (mut input, mut output) = client
            .invoke(OperationRequest::binding(doc, "a"), control())
            .unwrap()
            .split();
        if matches!(mode, Mode::Echo) {
            input.send(json("null")).await.unwrap();
            input.close();
        }
        let event = output.next().await;
        match mode {
            Mode::Echo => assert!(matches!(event, Some(OutputEvent::Output(_)))),
            Mode::Error => match event.unwrap() {
                OutputEvent::Error(error) => assert_eq!(
                    error.to_json().unwrap().text(),
                    r#"{"code":"fixture.error","data":null}"#
                ),
                other => panic!("{other:?}"),
            },
            Mode::Challenge => {
                let Some(OutputEvent::Error(error)) = event else {
                    panic!("challenge")
                };
                assert_eq!(
                    error.to_json().unwrap().text(),
                    InvocationError::challenge(challenge())
                        .to_json()
                        .unwrap()
                        .text()
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(preparations.get(), 0);
        assert_eq!(evaluations.get(), 0);
    }
}
#[test]
fn thread_safe_composition_yields_send_driver_and_provider_handles() {
    let provider = Provider::new(Mode::Park);
    let client = DefaultOperationInvoker::new(
        provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let call = client
        .invoke(
            OperationRequest::binding(document(r#"{"input":true}"#), "a"),
            control(),
        )
        .unwrap();
    require_send(&call);
    let (input, output) = call.split();
    require_send(&input);
    require_send(&output);
}
#[tokio::test]
async fn cancelled_before_first_poll_never_constructs_provider_work() {
    let provider = Provider::new(Mode::Park);
    let client = &provider;
    let cancel = control();
    let (_, mut output) = client
        .invoke(BindingRequest::new(document("{}"), "a"), cancel.clone())
        .unwrap()
        .split();
    cancel.cancel();
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
}
#[test]
fn dropping_output_cancels_parked_provider_and_input_cannot_drive() {
    let provider = Provider::new(Mode::Park);
    let client = &provider;
    let call = client
        .invoke(BindingRequest::new(document("{}"), "a"), control())
        .unwrap();
    let evidence = call.control();
    let (mut input, mut output) = call.split();
    let mut next = Box::pin(output.next());
    assert!(
        next.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(next);
    drop(output);
    assert_eq!(
        evidence.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    let mut send = Box::pin(input.send(json("1")));
    assert!(matches!(
        send.as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop())),
        Poll::Ready(Err(_))
    ));
}
#[test]
fn current_wire_addressing_is_closed_and_preserves_exact_values() {
    let doc = document("{}").value().to_owned();
    for address in [
        r#""operation":"echo""#,
        r#""operation":"echo","selection":["b","a"]"#,
        r#""binding":"a""#,
    ] {
        let text = format!(
            r#"{{"interface":{}, {},"context":{{"x":1e9999,"x":"\ud800"}}}}"#,
            doc.text(),
            address
        );
        let request = OperationRequest::from_json(&json(&text)).unwrap();
        let encoded = request.to_json().unwrap();
        assert_eq!(
            encoded.get("context").unwrap().text(),
            r#"{"x":1e9999,"x":"\ud800"}"#
        );
    }
    for address in [
        r#""operation":"echo","binding":"a""#,
        r#""binding":"a","selection":["a"]"#,
        r#""operation":"echo","selection":[]"#,
        r#""operation":"echo","operation":"echo""#,
        r#""operation":"echo","target":"old""#,
    ] {
        let text = format!(r#"{{"interface":{}, {}}}"#, doc.text(), address);
        assert!(
            OperationRequest::from_json(&json(&text)).is_err(),
            "{address}"
        );
    }
}

#[tokio::test]
async fn prepared_local_validators_are_reused_without_requiring_provider_clone() {
    use openbindings_invocation::operation::BindingChoice;
    let (eval, preparations, evaluations) = evaluator(false);
    // Provider itself is deliberately not Clone; only the owned validator is.
    let client =
        DefaultOperationInvoker::new(Provider::new(Mode::Echo), eval, ExternalSchemas::default());
    let prepared = client
        .prepare(
            document(r#"{"input":true,"output":true,"aliases":["alias"]}"#),
            "alias",
            &control(),
        )
        .unwrap();
    drop(client);
    let provider = Provider::new(Mode::Echo);
    assert_eq!(prepared.operation(), "echo");
    assert_eq!(preparations.get(), 2);
    for _ in 0..3 {
        let (mut input, mut output) = prepared
            .clone()
            .invoke(
                &provider,
                BindingChoice::Binding("a".into()),
                None,
                control(),
            )
            .unwrap()
            .split();
        for _ in 0..4 {
            input.send(json("1")).await.unwrap();
            assert!(matches!(output.next().await, Some(OutputEvent::Output(_))));
        }
        input.close();
        assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    }
    assert_eq!(preparations.get(), 2);
    assert_eq!(evaluations.get(), 24);
}
struct SendNotSyncValidator(Cell<usize>);
impl SchemaValidator for SendNotSyncValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        self.0.set(self.0.get() + 1);
        ValueOutcome::Satisfies
    }
}
struct LocalCompiler(Rc<Cell<usize>>);
impl SchemaEvaluator for LocalCompiler {
    type Validator = SendNotSyncValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        self.0.set(self.0.get() + 1);
        Ok(SendNotSyncValidator(Cell::new(0)))
    }
}
#[tokio::test]
async fn prepared_driver_is_send_without_sync_validator_or_send_evaluator() {
    use openbindings_invocation::operation::BindingChoice;
    let client = DefaultOperationInvoker::new(
        Provider::new(Mode::Echo),
        LocalCompiler(Rc::new(Cell::new(0))),
        ExternalSchemas::default(),
    );
    let prepared = client
        .prepare(
            document(r#"{"input":true,"output":true}"#),
            "echo",
            &control(),
        )
        .unwrap();
    drop(client);
    let provider = Provider::new(Mode::Echo);
    require_send(&prepared);
    let call = prepared
        .invoke(
            &provider,
            BindingChoice::Binding("a".into()),
            None,
            control(),
        )
        .unwrap();
    require_send(&call);
    let (mut input, mut output) = call.split();
    input.send(json("1")).await.unwrap();
    input.close();
    assert!(matches!(output.next().await, Some(OutputEvent::Output(_))));
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
}
#[tokio::test]
async fn prepared_snapshot_and_operation_membership_are_stable() {
    use openbindings_invocation::operation::BindingChoice;
    let provider = Provider::new(Mode::Echo);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let prepared = client
        .prepare(
            document(r#"{"input":{"type":"integer"}}"#),
            "echo",
            &control(),
        )
        .unwrap();
    let replacement = document(r#"{"input":{"type":"string"}}"#);
    assert_ne!(
        prepared.interface().value().text(),
        replacement.value().text()
    );
    assert_eq!(
        prepared
            .select(&provider, &BindingChoice::Binding("other".into()))
            .unwrap_err()
            .code(),
        "ERR_BINDING_NOT_FOUND"
    );
    let (_, mut output) = prepared
        .clone()
        .invoke(
            &provider,
            BindingChoice::Binding("other".into()),
            None,
            control(),
        )
        .unwrap()
        .split();
    assert_eq!(code(output.next().await), "ERR_BINDING_NOT_FOUND");
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
    let (mut input, mut output) = prepared
        .invoke(
            &provider,
            BindingChoice::Binding("a".into()),
            None,
            control(),
        )
        .unwrap()
        .split();
    input.send(json(r#""replacement-only""#)).await.unwrap();
    assert_eq!(code(output.next().await), "ERR_OPERATION_VALIDATION_FAILED");
    assert_eq!(provider.reads.load(Ordering::Relaxed), 0);
}
struct CancelDuringPreparation(CancellationToken);
impl SchemaEvaluator for CancelDuringPreparation {
    type Validator = SendNotSyncValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        self.0.cancel();
        Ok(SendNotSyncValidator(Cell::new(0)))
    }
}
#[test]
fn preparation_cancellation_has_a_direction_only_when_a_schema_was_being_prepared() {
    use openbindings_invocation::operation::OperationPreparationFailure;
    use std::error::Error;
    let cancel = control();
    cancel.cancel();
    let client = DefaultOperationInvoker::new(
        Provider::new(Mode::Empty),
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let failure = client.prepare(document("{}"), "echo", &cancel).unwrap_err();
    assert!(matches!(failure, OperationPreparationFailure::Cancelled));
    assert!(failure.source().is_none());
    let cancel = control();
    let client = DefaultOperationInvoker::new(
        Provider::new(Mode::Empty),
        CancelDuringPreparation(cancel.clone()),
        ExternalSchemas::default(),
    );
    let error = client
        .prepare(document(r#"{"output":true}"#), "echo", &cancel)
        .unwrap_err();
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<PreparationFailure>()
            .is_some()
    );
    assert_eq!(error.to_string(), "operation schema preparation failed");
    let OperationPreparationFailure::Schema(failure) = error else {
        panic!("typed preparation cancellation")
    };
    assert_eq!(failure.direction(), Direction::Output);
    assert!(matches!(
        failure.evidence(),
        ValidationEvidence::Preparation(PreparationFailure {
            kind: PreparationFailureKind::Cancelled,
            ..
        })
    ));
}

#[tokio::test]
async fn late_input_mismatch_preserves_prior_output_without_a_clean_eof() {
    let provider = Provider::new(Mode::Echo);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let (mut input, mut output) = client
        .invoke(
            OperationRequest::binding(document(r#"{"input":{"type":"integer"}}"#), "a"),
            control(),
        )
        .unwrap()
        .split();
    input.send(json("1")).await.unwrap();
    assert!(matches!(output.next().await,Some(OutputEvent::Output(value)) if value.text()=="1"));
    input.send(json(r#""bad""#)).await.unwrap();
    input.close();
    assert_eq!(code(output.next().await), "ERR_OPERATION_VALIDATION_FAILED");
    assert_eq!(provider.reads.load(Ordering::Relaxed), 1);
    assert_eq!(provider.eof.load(Ordering::Relaxed), 0);
}
struct SemanticEvaluator(bool);
struct SemanticValidator(bool, openbindings::SchemaOrigin);
impl SchemaEvaluator for SemanticEvaluator {
    type Validator = SemanticValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let root = request.environment(limits, control)?.root();
        let pointer = format!("{}/$ref", root.pointer()).parse().unwrap();
        Ok(SemanticValidator(
            self.0,
            openbindings::SchemaOrigin::new(root.origin().source_uri.clone(), pointer),
        ))
    }
}
impl SchemaValidator for SemanticValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        if self.0 {
            ValueOutcome::Undefined {
                detail: openbindings::UndefinedResult::new(
                    "static-reference-defect",
                    "established undefined",
                )
                .with_location(self.1.clone()),
            }
        } else {
            ValueOutcome::NoVerdict {
                detail: openbindings::NoVerdict::new(
                    openbindings::NoVerdictReason::UnsupportedCapability,
                    "review.unsupported",
                    "cannot establish",
                )
                .with_location(self.1.clone()),
            }
        }
    }
}
#[tokio::test]
async fn undefined_and_no_verdict_withhold_both_directions_and_keep_exact_evidence() {
    for undefined in [false, true] {
        for direction in [Direction::Input, Direction::Output] {
            let mode = if direction == Direction::Input {
                Mode::Echo
            } else {
                Mode::Outputs
            };
            let provider = Provider::new(mode);
            let client = DefaultOperationInvoker::new(
                &provider,
                SemanticEvaluator(undefined),
                ExternalSchemas::default(),
            );
            let operation = format!(
                r#"{{"{}":{{"$ref":"https://external.test/schema"}}}}"#,
                direction.as_str()
            );
            let call = client
                .invoke(
                    OperationRequest::binding(document(&operation), "a"),
                    control(),
                )
                .unwrap();
            let evidence = call.control();
            let (mut input, mut output) = call.split();
            if direction == Direction::Input {
                input.send(json("1")).await.unwrap();
            }
            input.close();
            let Some(OutputEvent::Error(error)) = output.next().await else {
                panic!("withheld value")
            };
            assert_eq!(error.code(), "ERR_SCHEMA_UNRESOLVED");
            assert!(error.data().is_none());
            assert!(output.next().await.is_none());
            let failure = evidence.validation_failure().unwrap();
            assert_eq!(failure.direction(), direction);
            let location = match failure.evidence() {
                ValidationEvidence::Evaluation(ValueOutcome::Undefined { detail }) => {
                    assert!(undefined);
                    assert_eq!(detail.code, "static-reference-defect");
                    detail.location.as_ref().unwrap()
                }
                ValidationEvidence::Evaluation(ValueOutcome::NoVerdict { detail }) => {
                    assert!(!undefined);
                    assert_eq!(
                        detail.reason,
                        openbindings::NoVerdictReason::UnsupportedCapability
                    );
                    assert_eq!(detail.code, "review.unsupported");
                    detail.location.as_ref().unwrap()
                }
                other => panic!("{other:?}"),
            };
            assert_eq!(
                location.pointer.to_string(),
                format!("/operations/echo/{}/$ref", direction.as_str())
            );
            assert_eq!(provider.reads.load(Ordering::Relaxed), 0);
            assert_eq!(provider.eof.load(Ordering::Relaxed), 0);
        }
    }
}

#[tokio::test]
async fn declared_schemas_without_values_do_not_prepare_even_when_unresolvable() {
    let provider = Provider::new(Mode::Empty);
    let (eval, preps, evaluations) = evaluator(true);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    let (_, mut output) = client
        .invoke(
            OperationRequest::binding(
                document(r#"{"input":false,"output":{"$ref":"https://absent.test/schema"}}"#),
                "a",
            ),
            control(),
        )
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert_eq!(preps.get(), 0);
    assert_eq!(evaluations.get(), 0);
}

#[tokio::test]
async fn absent_schema_operation_forwards_all_exact_values_context_and_original_document() {
    let provider = Provider::new(Mode::Echo);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let context =
        Context::try_from(json(r#"{"secret":1e9999,"duplicate":1,"duplicate":2}"#)).unwrap();
    let interface = document("{}");
    let original = interface.value().to_owned();
    let call = client
        .invoke(
            OperationRequest::binding(interface, "a").with_context(context.clone()),
            control(),
        )
        .unwrap();
    let evidence = call.control();
    let (mut input, mut output) = call.split();
    for value in ["null", "9007199254740993", "1e9999", r#"{"x":1,"x":2}"#] {
        input.send(json(value)).await.unwrap();
        assert!(
            matches!(output.next().await,Some(OutputEvent::Output(actual)) if actual.text()==value)
        );
    }
    input.close();
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    assert!(output.next().await.is_none());
    let seen = provider.seen.lock().unwrap();
    let seen = seen.as_ref().unwrap();
    assert_eq!(seen.binding, "a");
    assert_eq!(seen.interface.value().text(), original.text());
    assert_eq!(
        seen.context.as_ref().unwrap().value().value().text(),
        context.value().value().text()
    );
    assert!(evidence.validation_failure().is_none());
}
struct PreparationProbe {
    calls: AtomicUsize,
    live: std::sync::Arc<AtomicUsize>,
    stop_at: AtomicUsize,
    cancel: Option<CancellationToken>,
}
struct HeldValidator(std::sync::Arc<AtomicUsize>);
impl Drop for HeldValidator {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl SchemaValidator for HeldValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
impl SchemaEvaluator for PreparationProbe {
    type Validator = HeldValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.stop_at.load(Ordering::SeqCst) {
            if let Some(control) = &self.cancel {
                control.cancel();
            } else {
                return Err(PreparationFailure::new(
                    PreparationFailureKind::Implementation {
                        message: "temporary fixture failure".into(),
                    },
                ));
            }
        }
        self.live.fetch_add(1, Ordering::SeqCst);
        Ok(HeldValidator(self.live.clone()))
    }
}
#[test]
fn second_direction_failure_or_late_cancel_releases_partial_preparation_and_allows_retry() {
    use openbindings_invocation::operation::OperationPreparationFailure;
    for cancel_late in [false, true] {
        let cancellation = control();
        let probe = std::sync::Arc::new(PreparationProbe {
            calls: 0.into(),
            live: std::sync::Arc::default(),
            stop_at: 2.into(),
            cancel: cancel_late.then(|| cancellation.clone()),
        });
        let provider = Provider::new(Mode::Empty);
        let client =
            DefaultOperationInvoker::new(&provider, probe.clone(), ExternalSchemas::default());
        let interface = document(r#"{"input":true,"output":true}"#);
        let stopped = control();
        stopped.cancel();
        assert!(matches!(
            client.prepare(interface.clone(), "echo", &stopped),
            Err(OperationPreparationFailure::Cancelled)
        ));
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        let Err(OperationPreparationFailure::Schema(failure)) =
            client.prepare(interface.clone(), "echo", &cancellation)
        else {
            panic!("second schema failure")
        };
        assert_eq!(failure.direction(), Direction::Output);
        let ValidationEvidence::Preparation(failure) = failure.evidence() else {
            panic!("preparation evidence")
        };
        assert_eq!(
            matches!(failure.kind, PreparationFailureKind::Cancelled),
            cancel_late
        );
        assert_eq!(probe.calls.load(Ordering::SeqCst), 2);
        assert_eq!(probe.live.load(Ordering::SeqCst), 0);
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        probe.stop_at.store(0, Ordering::SeqCst);
        let prepared = client.prepare(interface, "echo", &control()).unwrap();
        assert_eq!(probe.calls.load(Ordering::SeqCst), 4);
        assert_eq!(probe.live.load(Ordering::SeqCst), 2);
        drop(prepared);
        assert_eq!(probe.live.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn explicit_preparation_distinguishes_absence_missing_name_and_declared_refusal() {
    use openbindings_invocation::operation::OperationPreparationFailure;
    let provider = Provider::new(Mode::Empty);
    let (eval, preps, evaluations) = evaluator(true);
    let client = DefaultOperationInvoker::new(&provider, eval, ExternalSchemas::default());
    let interface = document(r#"{"aliases":["say"]}"#);
    assert!(client.prepare(interface.clone(), "say", &control()).is_ok());
    let error = client.prepare(interface, "absent", &control()).unwrap_err();
    let cause = std::error::Error::source(&error).unwrap();
    assert_eq!(
        cause.downcast_ref::<InvocationError>().unwrap().code(),
        "ERR_OPERATION_NOT_FOUND"
    );
    assert_eq!(
        error.to_string(),
        "operation preparation name resolution failed"
    );
    assert_eq!(preps.get(), 0);
    assert_eq!(evaluations.get(), 0);
    for schema in ["true", "false"] {
        assert!(matches!(
            client.prepare(
                document(&format!(r#"{{"input":{schema}}}"#)),
                "echo",
                &control()
            ),
            Err(OperationPreparationFailure::Schema(_))
        ));
    }
    assert_eq!(preps.get(), 2);
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn prepared_choice_admission_and_complete_membership_checks_precede_support() {
    use openbindings_invocation::operation::BindingChoice;
    let provider = Provider::new(Mode::Empty);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let prepared = client.prepare(document("{}"), "echo", &control()).unwrap();
    for (choice, kind) in [
        (vec![], BoundaryFailureKind::EmptyArray),
        (vec![String::new()], BoundaryFailureKind::EmptyString),
    ] {
        assert_eq!(
            prepared
                .select(&provider, &BindingChoice::Ordered(choice.clone()))
                .unwrap_err()
                .code(),
            "ERR_BINDING_NOT_FOUND"
        );
        assert_eq!(
            prepared
                .clone()
                .invoke(&provider, BindingChoice::Ordered(choice), None, control())
                .unwrap_err()
                .kind(),
            kind
        );
    }
    for bad in ["other", "missing"] {
        let choice = BindingChoice::Ordered(vec!["a".into(), bad.into()]);
        assert_eq!(
            prepared.select(&provider, &choice).unwrap_err().code(),
            "ERR_BINDING_NOT_FOUND"
        );
        let (_, mut output) = prepared
            .clone()
            .invoke(&provider, choice, None, control())
            .unwrap()
            .split();
        assert_eq!(code(output.next().await), "ERR_BINDING_NOT_FOUND");
    }
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    assert_eq!(provider.queries.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn concurrent_prepared_clones_keep_selection_context_cancellation_and_evidence_separate() {
    use openbindings_invocation::operation::BindingChoice;
    let provider = Provider::new(Mode::Echo);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let prepared = client
        .prepare(
            document(r#"{"input":{"type":"integer"},"output":{"type":"integer"}}"#),
            "echo",
            &control(),
        )
        .unwrap();
    let context_a =
        Context::try_from(json(r#"{"token":"A","n":1e9999,"same":1,"same":2}"#)).unwrap();
    let context_b = Context::try_from(json(r#"{"token":"B","n":9007199254740993}"#)).unwrap();
    let parent = control();
    let a = prepared
        .clone()
        .invoke(
            &provider,
            BindingChoice::Ordered(vec!["b".into(), "a".into()]),
            Some(context_a.clone()),
            parent.clone(),
        )
        .unwrap();
    let a_control = a.control();
    let (mut ai, mut ao) = a.split();
    let b = prepared
        .clone()
        .invoke(
            &provider,
            BindingChoice::Binding("a".into()),
            Some(context_b.clone()),
            parent.clone(),
        )
        .unwrap();
    let b_control = b.control();
    let (mut bi, mut bo) = b.split();
    assert!(
        Box::pin(ao.next())
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop()))
            .is_pending()
    );
    {
        let seen = provider.seen.lock().unwrap();
        let seen = seen.as_ref().unwrap();
        assert_eq!(seen.binding, "b");
        assert_eq!(
            seen.context.as_ref().unwrap().value().value().text(),
            context_a.value().value().text()
        );
    }
    assert!(
        Box::pin(bo.next())
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop()))
            .is_pending()
    );
    {
        let seen = provider.seen.lock().unwrap();
        let seen = seen.as_ref().unwrap();
        assert_eq!(seen.binding, "a");
        assert_eq!(
            seen.context.as_ref().unwrap().value().value().text(),
            context_b.value().value().text()
        );
    }
    a_control.cancel();
    assert_eq!(code(ao.next().await), "ERR_CANCELLED");
    assert!(ai.send(json("1")).await.is_err());
    assert!(!parent.is_cancelled());
    assert!(b_control.outcome().is_none());
    bi.send(json("2")).await.unwrap();
    bi.close();
    assert!(matches!(bo.next().await,Some(OutputEvent::Output(value)) if value.text()=="2"));
    assert!(matches!(bo.next().await, Some(OutputEvent::Complete)));
    let c = prepared
        .clone()
        .invoke(
            &provider,
            BindingChoice::Binding("a".into()),
            Some(context_a),
            control(),
        )
        .unwrap();
    let c_control = c.control();
    let (mut ci, mut co) = c.split();
    ci.send(json(r#""bad""#)).await.unwrap();
    ci.close();
    assert_eq!(code(co.next().await), "ERR_OPERATION_VALIDATION_FAILED");
    assert!(a_control.validation_failure().is_none());
    assert!(b_control.validation_failure().is_none());
    assert!(c_control.validation_failure().is_some());
    let d = prepared
        .invoke(
            &provider,
            BindingChoice::Binding("a".into()),
            Some(context_b),
            control(),
        )
        .unwrap();
    let d_control = d.control();
    let (mut di, mut dout) = d.split();
    di.send(json("3")).await.unwrap();
    di.close();
    assert!(matches!(dout.next().await,Some(OutputEvent::Output(value)) if value.text()=="3"));
    assert!(matches!(dout.next().await, Some(OutputEvent::Complete)));
    assert!(d_control.validation_failure().is_none());
}

#[tokio::test]
async fn empty_operation_addresses_resolve_as_absent_names() {
    use openbindings_invocation::operation::OperationPreparationFailure;
    let provider = Provider::new(Mode::Empty);
    let client = DefaultOperationInvoker::new(
        &provider,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    for (field, code) in [
        ("operation", "ERR_OPERATION_NOT_FOUND"),
        ("binding", "ERR_BINDING_NOT_FOUND"),
    ] {
        let wire = json(&format!(
            r#"{{"interface":{},"{field}":""}}"#,
            document("{}").value().text()
        ));
        let request = OperationRequest::from_json(&wire).unwrap();
        assert_eq!(
            client
                .select(&request.interface, &request.address)
                .unwrap_err()
                .code(),
            code
        );
        assert_eq!(
            request.to_json().unwrap().get(field).unwrap().as_str(),
            Some("")
        );
        let (_, mut output) = client.invoke(request.clone(), control()).unwrap().split();
        let Some(OutputEvent::Error(error)) = output.next().await else {
            panic!("terminal")
        };
        assert_eq!(error.code(), code);
        assert_eq!(
            client
                .preflight(request, control())
                .unwrap()
                .await
                .unwrap_err()
                .code(),
            code
        );
    }
    assert!(matches!(
        client.prepare(document("{}"), "", &control()),
        Err(OperationPreparationFailure::Resolution(error)) if error.code() == "ERR_OPERATION_NOT_FOUND"
    ));
    assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
}

//! Checked entry points, application routing and third-party invocation construction.
use openbindings::{
    ConformantDocument, Detail, Document, EvaluationRequest, ExternalSchemas, JsonValue,
    PreparationFailure, PreparationLimits, SchemaEvaluator, SchemaValidator, ValueOutcome,
    WorkControl,
};
use openbindings_invocation::ContextChallenge;
use openbindings_invocation::{
    BindingInvoker, BindingInvokerExt, BindingRequest, BoundaryFailure, BoundaryFailureKind,
    CancellationToken, DefaultOperationInvoker, Invocation, InvocationError, InvocationLimits,
    InvocationSupport, OperationAddress, OperationInvoker, OperationRequest, OutputEvent,
    ProviderIo,
};
use std::{
    cell::Cell,
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
fn cancel() -> CancellationToken {
    CancellationToken::new()
}
fn document() -> ConformantDocument {
    Document::parse(r#"{"openbindings":"0.2.0","operations":{"a":{"input":true,"output":true},"b":{"input":true,"output":true}},"sources":{"a":{"kind":"example.a"},"b":{"kind":"example.b"}},"bindings":{"a":{"operation":"a","source":"a"},"b":{"operation":"b","source":"b"}}}"#).unwrap().validate().unwrap().conformant().unwrap()
}
struct Echo<'a>(&'a AtomicUsize);
impl BindingInvoker for Echo<'_> {
    type Invoke<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    type Preflight<'a>
        = Ready<Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        self.0.fetch_add(1, Ordering::Relaxed);
        ready(Ok(None))
    }
}
struct Router<'a> {
    a: Echo<'a>,
    b: Echo<'a>,
}
impl Router<'_> {
    fn route(&self, request: &BindingRequest) -> &Echo<'_> {
        match request
            .interface
            .binding(&request.binding)
            .unwrap()
            .source()
            .kind()
        {
            "example.a" => &self.a,
            "example.b" => &self.b,
            _ => unreachable!(),
        }
    }
}
impl BindingInvoker for Router<'_> {
    type Invoke<'a>
        = <Echo<'a> as BindingInvoker>::Invoke<'a>
    where
        Self: 'a;
    type Preflight<'a>
        = Ready<Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.route(&request).handle_invoke(request, io, control)
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a> {
        self.route(&request).handle_preflight(request, control)
    }
}
impl InvocationSupport for Router<'_> {
    fn can_invoke(&self, kind: &str) -> bool {
        matches!(kind, "example.a" | "example.b")
    }
}
struct CountingEvaluator(Rc<Cell<usize>>, Rc<Cell<usize>>);
struct CountingValidator(Rc<Cell<usize>>);
impl SchemaEvaluator for CountingEvaluator {
    type Validator = CountingValidator;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        self.0.set(self.0.get() + 1);
        Ok(CountingValidator(self.1.clone()))
    }
}
impl SchemaValidator for CountingValidator {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        self.0.set(self.0.get() + 1);
        ValueOutcome::Satisfies
    }
}
fn generic_send<'a, P: BindingInvoker + Sync>(
    p: &'a P,
    request: BindingRequest,
) -> Invocation<impl Future<Output = Result<(), InvocationError>> + Send + 'a>
where
    P::Invoke<'a>: Send,
{
    p.invoke(request, cancel()).unwrap()
}
fn generic_preflight_send<'a, P: BindingInvoker + Sync>(
    p: &'a P,
    request: BindingRequest,
) -> impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + Send + 'a
where
    P::Preflight<'a>: Send,
{
    p.preflight(request, cancel()).unwrap()
}
async fn echo<F: Future<Output = Result<(), InvocationError>>>(call: Invocation<F>) {
    let (mut input, mut output) = call.split();
    let value = JsonValue::parse("9007199254740993").unwrap();
    input.send(value).await.unwrap();
    input.close();
    let Some(OutputEvent::Output(value)) = output.next().await else {
        panic!("output")
    };
    assert_eq!(value.text(), "9007199254740993");
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
}
#[tokio::test]
async fn aggregate_routes_both_interfaces_and_checks_each_value_once() {
    let a = AtomicUsize::new(0);
    let b = AtomicUsize::new(0);
    let router = Router {
        a: Echo(&a),
        b: Echo(&b),
    };
    for key in ["a", "b"] {
        let request = BindingRequest::new(document(), key);
        echo(generic_send(&router, request.clone())).await;
        assert!((generic_preflight_send(&router, request).await.unwrap()).is_none());
    }
    assert_eq!(
        (a.load(Ordering::Relaxed), b.load(Ordering::Relaxed)),
        (2, 2)
    );
    let preparations = Rc::new(Cell::new(0));
    let evaluations = Rc::new(Cell::new(0));
    let invoker = DefaultOperationInvoker::new(
        router,
        CountingEvaluator(preparations.clone(), evaluations.clone()),
        ExternalSchemas::default(),
    );
    for key in ["a", "b"] {
        let request = OperationRequest::operation(document(), key);
        echo(OperationInvoker::invoke(&invoker, request.clone(), cancel()).unwrap()).await;
        assert!(
            OperationInvoker::preflight(&invoker, request, cancel())
                .unwrap()
                .await
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        (a.load(Ordering::Relaxed), b.load(Ordering::Relaxed)),
        (4, 4)
    );
    assert_eq!(
        preparations.get(),
        4,
        "one preparation for each declared direction"
    );
    assert_eq!(
        evaluations.get(),
        4,
        "one check for each input/output, no second channel"
    );
}
// A third-party implementation for an interface with no available operations.
struct EmptyInvoker;
impl OperationInvoker for EmptyInvoker {
    fn invoke(
        &self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>
    {
        request.admit()?;
        let code = match request.address {
            OperationAddress::Operation { .. } => "ERR_OPERATION_NOT_FOUND",
            OperationAddress::Binding(_) => "ERR_BINDING_NOT_FOUND",
        };
        Ok(Invocation::new(
            InvocationLimits::default(),
            control,
            move |_, _| ready(Err(InvocationError::new(code, None).unwrap())),
        )?)
    }
    fn preflight(
        &self,
        request: OperationRequest,
        _: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    > {
        request.admit()?;
        Ok(ready(Err(InvocationError::new(
            "ERR_OPERATION_NOT_FOUND",
            None,
        )
        .unwrap())))
    }
}
#[tokio::test]
async fn third_party_invocation_preserves_operation_codes_and_local_admission() {
    let empty = Document::parse(r#"{"openbindings":"0.2.0","operations":{}}"#)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    for name in ["", "missing"] {
        let call = EmptyInvoker
            .invoke(OperationRequest::operation(empty.clone(), name), cancel())
            .unwrap();
        let (_, mut output) = call.split();
        let Some(OutputEvent::Error(error)) = output.next().await else {
            panic!("terminal")
        };
        assert_eq!(error.code(), "ERR_OPERATION_NOT_FOUND");
        assert!(output.next().await.is_none());
    }
}
#[tokio::test]
async fn public_constructor_is_lazy_cancellable_and_keeps_limits_optional() {
    let starts = Cell::new(0);
    let control = cancel();
    let call = Invocation::new(InvocationLimits::default(), control.clone(), |_, _| {
        starts.set(starts.get() + 1);
        ready(Ok(()))
    })
    .unwrap();
    assert_eq!(starts.get(), 0);
    control.cancel();
    let (_, mut output) = call.split();
    assert!(matches!(output.next().await,Some(OutputEvent::Error(e)) if e.code()=="ERR_CANCELLED"));
    assert_eq!(starts.get(), 0);
    let invalid = InvocationLimits {
        input_capacity: 0,
        ..Default::default()
    };
    let calls = AtomicUsize::new(0);
    let provider = Echo(&calls);
    assert_eq!(
        provider
            .invoke_with_limits(BindingRequest::new(document(), "a"), cancel(), invalid)
            .unwrap_err()
            .kind(),
        BoundaryFailureKind::InvalidLimits
    );
    assert!(Invocation::new(invalid, cancel(), |_, _| ready(Ok(()))).is_err());
    assert!(InvocationLimits::default().max_value_bytes.is_none());
}

#[tokio::test]
async fn public_constructor_cancels_before_dropping_started_work() {
    use std::task::{Context, Poll, Waker};
    struct PendingWork {
        _io: ProviderIo,
        local: CancellationToken,
        dropped: Rc<Cell<bool>>,
    }
    impl Future for PendingWork {
        type Output = Result<(), InvocationError>;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for PendingWork {
        fn drop(&mut self) {
            assert!(self.local.is_cancelled());
            self.dropped.set(true);
        }
    }
    let parent = cancel();
    let dropped = Rc::new(Cell::new(false));
    let witness = dropped.clone();
    let call = Invocation::new(
        InvocationLimits::default(),
        parent.clone(),
        move |io, local| PendingWork {
            _io: io,
            local,
            dropped: witness,
        },
    )
    .unwrap();
    let (_, mut output) = call.split();
    {
        let mut next = Box::pin(output.next());
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert!(!dropped.get());
    drop(output);
    assert!(dropped.get());
    assert!(!parent.is_cancelled());
}

#[test]
fn preparing_operation_schemas_needs_no_routing_support() {
    let calls = AtomicUsize::new(0);
    let preparations = Rc::new(Cell::new(0));
    let invoker = DefaultOperationInvoker::new(
        Echo(&calls),
        CountingEvaluator(preparations.clone(), Rc::new(Cell::new(0))),
        ExternalSchemas::default(),
    )
    .with_limits(InvocationLimits::default())
    .unwrap()
    .with_preparation_limits(PreparationLimits::default())
    .with_validation_detail(Detail::Explain);
    invoker.prepare(document(), "a", &cancel()).unwrap();
    assert_eq!(preparations.get(), 2);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

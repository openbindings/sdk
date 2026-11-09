//! Cancellation reaches synchronous work and never changes an earlier terminal.
use openbindings::{
    ConformantDocument, Detail, Document, EvaluationRequest, ExternalSchemas, JsonValue,
    PreparationFailure, PreparationLimits, SchemaEvaluator, SchemaValidator, ValueOutcome,
    WorkControl,
};
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, DefaultOperationInvoker, InvocationError,
    InvocationSupport, OperationRequest, OutputEvent, PreflightOutcome, ProviderIo,
};
use std::{
    future::{Future, poll_fn},
    pin::{Pin, pin},
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
fn doc() -> ConformantDocument {
    Document::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}},"bindings":{"b":{"operation":"run","source":"s"}},"sources":{"s":{"kind":"test"}}}"#).unwrap().validate().unwrap().conformant().unwrap()
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn terminal(result: Poll<Option<OutputEvent>>) -> InvocationError {
    match result {
        Poll::Ready(Some(OutputEvent::Error(error))) => error,
        other => panic!("terminal required: {other:?}"),
    }
}
#[derive(Clone, Copy)]
enum Stage {
    Preparation,
    Evaluation,
}
struct Evaluator {
    barrier: Option<Arc<Barrier>>,
    saw_cancel: Arc<AtomicBool>,
    stage: Stage,
}
struct Prepared {
    barrier: Option<Arc<Barrier>>,
    saw_cancel: Arc<AtomicBool>,
}
fn observe(barrier: &Barrier, control: &dyn WorkControl, saw: &AtomicBool) {
    barrier.wait();
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until && !control.is_cancelled() {
        std::thread::yield_now();
    }
    saw.store(control.is_cancelled(), Ordering::SeqCst);
    assert!(
        saw.load(Ordering::SeqCst),
        "synchronous cooperative work must observe external cancellation"
    );
}
impl SchemaEvaluator for Evaluator {
    type Validator = Prepared;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        if matches!(self.stage, Stage::Preparation)
            && let Some(barrier) = &self.barrier
        {
            observe(barrier, control, &self.saw_cancel);
            control.check()?;
        }
        Ok(Prepared {
            barrier: self
                .barrier
                .clone()
                .filter(|_| matches!(self.stage, Stage::Evaluation)),
            saw_cancel: self.saw_cancel.clone(),
        })
    }
}
impl SchemaValidator for Prepared {
    fn evaluate(&self, _: &JsonValue, _: Detail, control: &dyn WorkControl) -> ValueOutcome {
        if let Some(barrier) = &self.barrier {
            observe(barrier, control, &self.saw_cancel);
        }
        ValueOutcome::Fails {
            message: "fixture mismatch".into(),
            problems: vec![],
            problems_complete: false,
        }
    }
}
struct Provider {
    repeat_poll: bool,
    eof: Arc<AtomicBool>,
}
impl InvocationSupport for Provider {
    fn can_invoke(&self, _: &str) -> bool {
        true
    }
}
impl BindingInvoker for Provider {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = std::future::Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            let value = if self.repeat_poll {
                let mut read = pin!(io.next());
                poll_fn(|cx| match read.as_mut().poll(cx) {
                    Poll::Ready(value) => Poll::Ready(value),
                    Poll::Pending => read.as_mut().poll(cx),
                })
                .await
            } else {
                io.next().await
            };
            if value.is_none() {
                self.eof.store(true, Ordering::SeqCst);
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        std::future::ready(Ok(PreflightOutcome::NoPrediction))
    }
}
fn concurrent_cancel(stage: Stage) {
    let barrier = Arc::new(Barrier::new(2));
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let eof = Arc::new(AtomicBool::new(false));
    let client = DefaultOperationInvoker::new(
        Provider {
            repeat_poll: false,
            eof: eof.clone(),
        },
        Evaluator {
            barrier: Some(barrier.clone()),
            saw_cancel: saw_cancel.clone(),
            stage,
        },
        ExternalSchemas::default(),
    );
    let external = CancellationToken::new();
    let call = client
        .invoke(OperationRequest::binding(doc(), "b"), external.clone())
        .unwrap();
    let control = call.control();
    let (mut input, mut output) = call.split();
    assert!(poll(pin!(input.send(JsonValue::null()))).is_ready());
    let thread = std::thread::spawn(move || {
        barrier.wait();
        external.cancel();
    });
    let result = poll(pin!(output.next()));
    thread.join().unwrap();
    assert_eq!(terminal(result).code(), "ERR_CANCELLED");
    assert!(saw_cancel.load(Ordering::SeqCst));
    assert!(control.validation_failure().is_none());
    assert!(!eof.load(Ordering::SeqCst));
}
#[test]
fn external_cancel_reaches_cooperative_evaluator() {
    concurrent_cancel(Stage::Evaluation);
}
#[test]
fn external_cancel_reaches_preparation_in_current_poll() {
    concurrent_cancel(Stage::Preparation);
}
#[test]
fn rejected_input_cannot_turn_into_eof_on_second_poll() {
    let eof = Arc::new(AtomicBool::new(false));
    let client = DefaultOperationInvoker::new(
        Provider {
            repeat_poll: true,
            eof: eof.clone(),
        },
        Evaluator {
            barrier: None,
            saw_cancel: Arc::default(),
            stage: Stage::Evaluation,
        },
        ExternalSchemas::default(),
    );
    let (mut input, mut output) = client
        .invoke(
            OperationRequest::binding(doc(), "b"),
            CancellationToken::new(),
        )
        .unwrap()
        .split();
    assert!(poll(pin!(input.send(JsonValue::null()))).is_ready());
    assert_eq!(
        terminal(poll(pin!(output.next()))).code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
    assert!(!eof.load(Ordering::SeqCst));
}
struct CancelThenComplete {
    external: CancellationToken,
    synchronous: bool,
}
impl InvocationSupport for CancelThenComplete {
    fn can_invoke(&self, _: &str) -> bool {
        true
    }
}
impl BindingInvoker for CancelThenComplete {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> =
        Pin<Box<dyn Future<Output = Result<PreflightOutcome, InvocationError>> + Send + 'a>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        _: ProviderIo,
        local: CancellationToken,
    ) -> Self::Invoke<'a> {
        if self.synchronous {
            self.external.cancel();
            assert!(local.is_cancelled());
        }
        Box::pin(async move {
            self.external.cancel();
            assert!(local.is_cancelled());
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        local: CancellationToken,
    ) -> Self::Preflight<'a> {
        if self.synchronous {
            self.external.cancel();
            assert!(local.is_cancelled());
        }
        Box::pin(async move {
            self.external.cancel();
            assert!(local.is_cancelled());
            Ok(PreflightOutcome::NoPrediction)
        })
    }
}
fn cancellation_overrides_success(synchronous: bool, preflight: bool) {
    let external = CancellationToken::new();
    let client = DefaultOperationInvoker::new(
        CancelThenComplete {
            external: external.clone(),
            synchronous,
        },
        Evaluator {
            barrier: None,
            saw_cancel: Arc::default(),
            stage: Stage::Evaluation,
        },
        ExternalSchemas::default(),
    );
    if preflight {
        let future = client
            .preflight(OperationRequest::binding(doc(), "b"), external.clone())
            .unwrap();
        assert!(
            matches!(poll(pin!(future)),Poll::Ready(Err(error)) if error.code()=="ERR_CANCELLED")
        );
    } else {
        let call = client
            .invoke(OperationRequest::binding(doc(), "b"), external.clone())
            .unwrap();
        let control = call.control();
        let (_, mut output) = call.split();
        assert_eq!(terminal(poll(pin!(output.next()))).code(), "ERR_CANCELLED");
        assert!(control.validation_failure().is_none());
    }
    assert!(external.is_cancelled());
}
#[test]
fn cancellation_during_poll_overrides_ready_provider_success() {
    cancellation_overrides_success(false, false);
}
#[test]
fn cancellation_during_poll_overrides_ready_preflight_success() {
    cancellation_overrides_success(false, true);
}
#[test]
fn synchronous_provider_and_preflight_start_observe_relay() {
    cancellation_overrides_success(true, false);
    cancellation_overrides_success(true, true);
}
#[test]
fn already_settled_validation_keeps_its_evidence_when_external_cancel_follows() {
    let external = CancellationToken::new();
    let client = DefaultOperationInvoker::new(
        Provider {
            repeat_poll: false,
            eof: Arc::default(),
        },
        Evaluator {
            barrier: None,
            saw_cancel: Arc::default(),
            stage: Stage::Evaluation,
        },
        ExternalSchemas::default(),
    );
    let call = client
        .invoke(OperationRequest::binding(doc(), "b"), external.clone())
        .unwrap();
    let control = call.control();
    let (mut input, mut output) = call.split();
    assert!(poll(pin!(input.send(JsonValue::null()))).is_ready());
    assert_eq!(
        terminal(poll(pin!(output.next()))).code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
    let before = control.validation_failure().unwrap();
    let openbindings_invocation::operation::ValidationEvidence::Evaluation(before_outcome) =
        before.evidence()
    else {
        panic!("evaluation evidence required")
    };
    external.cancel();
    control.cancel();
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
    let after = control.validation_failure().unwrap();
    assert_eq!(after.direction(), before.direction());
    let openbindings_invocation::operation::ValidationEvidence::Evaluation(after_outcome) =
        after.evidence()
    else {
        panic!("evaluation evidence required")
    };
    assert_eq!(
        serde_json::to_value(after_outcome).unwrap(),
        serde_json::to_value(before_outcome).unwrap()
    );
}

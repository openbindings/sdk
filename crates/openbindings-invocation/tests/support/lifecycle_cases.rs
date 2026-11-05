//! Shared lifecycle witnesses through inherent receive and Stream receive adapters.
use openbindings::{Document, JsonValue};
use openbindings_invocation::{
    BindingClient, BindingInvoker, BindingRequest, BoundaryFailureKind, CancellationToken,
    Context as InvocationContext, ContextChallenge, InvocationError, InvocationLimits,
    InvocationOutput, OutputEvent, PreflightOutcome, ProviderIo, WriteError,
};
use std::{
    future::{Future, pending},
    pin::{Pin, pin},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn request() -> BindingRequest {
    let interface = Document::parse(r#"{"openbindings":"0.2.0","operations":{"o":{}},"sources":{"s":{"kind":"fixture"}},"bindings":{"b":{"operation":"o","source":"s"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
    BindingRequest::new(interface, "b")
}
fn challenge() -> ContextChallenge {
    ContextChallenge::try_from(json(
        r#"{"message":"Supply a token","schema":{"type":"object","required":["token"]}}"#,
    ))
    .unwrap()
}
fn cancel() -> CancellationToken {
    CancellationToken::new()
}
struct Guard(Arc<AtomicUsize>);
impl Guard {
    fn new(counter: &Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter.clone())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Empty,
    Null,
    Echo,
    Server(usize),
    Count,
    EarlyClose,
    OutputError,
    Challenge,
    OutputChallenge,
    AuthFailure,
    Park,
    GateEcho,
    Duplex,
}
struct Fixture<'a> {
    mode: Mode,
    name: &'a str,
    calls: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    effects: Arc<AtomicUsize>,
    gate: CancellationToken,
}
impl Fixture<'_> {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            name: "borrowed",
            calls: Arc::default(),
            live: Arc::default(),
            effects: Arc::default(),
            gate: CancellationToken::new(),
        }
    }
}
impl BindingInvoker for Fixture<'_> {
    type Invoke<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    type Preflight<'a>
        = std::future::Ready<Result<PreflightOutcome, InvocationError>>
    where
        Self: 'a;
    fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        std::future::ready(Ok(PreflightOutcome::NoPrediction))
    }
    fn invoke<'a>(
        &'a self,
        _r: BindingRequest,
        io: ProviderIo,
        _c: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let (mut input, mut output) = io.split();
            assert_eq!(self.name, "borrowed");
            let _guard = Guard::new(&self.live);
            match self.mode {
                Mode::Empty => {}
                Mode::Null => output.send(json("null")).await?,
                Mode::Echo | Mode::GateEcho => {
                    if matches!(self.mode, Mode::GateEcho) {
                        self.gate.cancelled().await;
                    }
                    while let Some(v) = super::receive(&mut input).await {
                        self.effects.fetch_add(1, Ordering::SeqCst);
                        output.send(v).await?;
                    }
                }
                Mode::Server(n) => {
                    input.close();
                    for i in 0..n {
                        output.send(json(&i.to_string())).await?;
                    }
                }
                Mode::Count => {
                    let mut n = 0;
                    while super::receive(&mut input).await.is_some() {
                        n += 1;
                        self.effects.fetch_add(1, Ordering::SeqCst);
                    }
                    output.send(json(&n.to_string())).await?;
                }
                Mode::EarlyClose => {
                    input.close();
                    self.gate.cancelled().await;
                    output.send(json("7")).await?;
                }
                Mode::OutputError => {
                    output.send(json("1")).await?;
                    return Err(InvocationError::new("custom.failed", Some(json("null"))).unwrap());
                }
                Mode::Challenge => return Err(InvocationError::challenge(challenge())),
                Mode::OutputChallenge => {
                    output.send(json("1")).await?;
                    return Err(InvocationError::challenge(challenge()));
                }
                Mode::AuthFailure => {
                    self.effects.fetch_add(1, Ordering::SeqCst);
                    return Err(InvocationError::new("custom.auth", None).unwrap());
                }
                Mode::Park => pending::<()>().await,
                Mode::Duplex => {
                    let read = async {
                        let mut n = 0;
                        while super::receive(&mut input).await.is_some() {
                            n += 1;
                        }
                        n
                    };
                    let write = async {
                        for i in 0..37 {
                            output.send(json(&i.to_string())).await?;
                        }
                        Ok::<_, InvocationError>(())
                    };
                    let (n, result) = tokio::join!(read, write);
                    result?;
                    assert_eq!(n, 37);
                }
            }
            Ok(())
        })
    }
}

#[derive(Default)]
struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn code(event: Option<OutputEvent>) -> String {
    match event.unwrap() {
        OutputEvent::Error(error) => error.code().into(),
        other => panic!("{other:?}"),
    }
}
async fn collect<F: Future<Output = Result<(), InvocationError>>>(
    output: &mut InvocationOutput<F>,
) -> Result<Vec<String>, InvocationError> {
    let mut values = Vec::new();
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(value) => values.push(value.text().to_owned()),
            OutputEvent::InputClosed => (),
            OutputEvent::Complete => return Ok(values),
            OutputEvent::Error(error) => return Err(error),
            _ => panic!("unknown current event"),
        }
    }
    panic!("terminal event was required")
}
#[tokio::test]
async fn lazy_admission_and_advisory_no_prediction_do_no_provider_work() {
    let provider = Fixture::new(Mode::Empty);
    let client = BindingClient::new(&provider);
    assert!(matches!(
        client
            .preflight(request(), cancel())
            .unwrap()
            .await
            .unwrap(),
        PreflightOutcome::NoPrediction
    ));
    drop(client.invoke(request(), cancel()).unwrap());
    let mut bad = request();
    bad.binding.clear();
    assert!(client.invoke(bad, cancel()).is_err());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn zero_null_and_output_then_failure_require_terminal_observation() {
    for mode in [Mode::Empty, Mode::Null, Mode::OutputError] {
        let provider = Fixture::new(mode);
        let client = BindingClient::new(&provider);
        let call = client.invoke(request(), cancel()).unwrap();
        let control = call.control();
        let (_, mut output) = call.split();
        match mode {
            Mode::Empty => assert!(matches!(output.next().await, Some(OutputEvent::Complete))),
            Mode::Null => {
                assert!(
                    matches!(output.next().await, Some(OutputEvent::Output(v)) if v.text()=="null")
                );
                assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
            }
            _ => {
                assert!(
                    matches!(output.next().await, Some(OutputEvent::Output(v)) if v.text()=="1")
                );
                let Some(OutputEvent::Error(error)) = output.next().await else {
                    panic!("failure")
                };
                assert_eq!(error.code(), "custom.failed");
                assert_eq!(error.data().unwrap().text(), "null");
            }
        }
        assert!(output.next().await.is_none());
        assert_eq!(provider.live.load(Ordering::SeqCst), 0);
        let before = control
            .outcome()
            .unwrap()
            .err()
            .map(|e| e.code().to_owned());
        control.cancel();
        assert_eq!(
            control
                .outcome()
                .unwrap()
                .err()
                .map(|e| e.code().to_owned()),
            before
        );
    }
}
#[tokio::test]
async fn all_cardinalities_share_output_driving_and_bounded_bidi_handoffs() {
    for mode in [Mode::Echo, Mode::Count, Mode::Duplex] {
        let provider = Fixture::new(mode);
        let client = BindingClient::new(&provider);
        let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
        let writing = async {
            for i in 0..37 {
                input.send(json(&i.to_string())).await.unwrap();
            }
            input.close();
        };
        let (_, values) = tokio::join!(writing, collect(&mut output));
        let values = values.unwrap();
        if matches!(mode, Mode::Count) {
            assert_eq!(values, ["37"]);
        } else {
            assert_eq!(values, (0..37).map(|i| i.to_string()).collect::<Vec<_>>());
        }
    }
    let provider = Fixture::new(Mode::Server(23));
    let client = BindingClient::new(&provider);
    let (input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    drop(input);
    assert_eq!(
        collect(&mut output).await.unwrap(),
        (0..23).map(|i| i.to_string()).collect::<Vec<_>>()
    );
}
#[tokio::test]
async fn accepted_input_drains_on_half_close_and_early_close_keeps_output_open() {
    let provider = Fixture::new(Mode::GateEcho);
    let client = BindingClient::new(&provider);
    let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    input.send(json("9007199254740993")).await.unwrap();
    input.close();
    assert_eq!(input.send(json("2")).await, Err(WriteError::Closed));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    provider.gate.cancel();
    assert_eq!(collect(&mut output).await.unwrap(), ["9007199254740993"]);
    let provider = Fixture::new(Mode::EarlyClose);
    let client = BindingClient::new(&provider);
    let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    assert!(matches!(
        output.next().await,
        Some(OutputEvent::InputClosed)
    ));
    input.closed().await;
    assert!(input.is_closed());
    assert!(output.control().outcome().is_none());
    assert_eq!(input.send(json("1")).await, Err(WriteError::Closed));
    provider.gate.cancel();
    assert_eq!(collect(&mut output).await.unwrap(), ["7"]);
}
#[tokio::test]
async fn pending_write_drop_and_pending_read_drop_do_not_consume_or_duplicate() {
    let provider = Fixture::new(Mode::GateEcho);
    let client = BindingClient::new(&provider);
    let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    input.send(json("1")).await.unwrap();
    {
        let mut write = pin!(input.send(json("2")));
        assert!(poll(write.as_mut()).is_pending());
    }
    {
        let mut read = pin!(output.next());
        assert!(poll(read.as_mut()).is_pending());
    }
    input.close();
    provider.gate.cancel();
    assert_eq!(collect(&mut output).await.unwrap(), ["1"]);
}
#[tokio::test]
async fn cancellation_full_queues_preserves_committed_output_order_and_releases_job() {
    let provider = Fixture::new(Mode::Server(23));
    let client = BindingClient::new(&provider);
    let call = client.invoke(request(), cancel()).unwrap();
    let control = call.control();
    let (_, mut output) = call.split();
    assert!(matches!(output.next().await, Some(OutputEvent::Output(v)) if v.text()=="0"));
    control.cancel();
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    for i in 1..4 {
        assert!(
            matches!(output.next().await, Some(OutputEvent::Output(v)) if v.text()==i.to_string())
        );
    }
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert!(output.next().await.is_none());
    assert_eq!(provider.live.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn pre_cancel_external_control_and_output_abandonment_do_not_cancel_siblings() {
    let provider = Fixture::new(Mode::Park);
    let client = BindingClient::new(&provider);
    let cancelled = cancel();
    cancelled.cancel();
    let (_, mut output) = client.invoke(request(), cancelled).unwrap().split();
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    let parent = cancel();
    let first = client.invoke(request(), parent.clone()).unwrap();
    let sibling = client.invoke(request(), parent.clone()).unwrap();
    let sibling_control = sibling.control();
    let (_, mut first_output) = first.split();
    assert!(poll(pin!(first_output.next())).is_pending());
    drop(first_output);
    assert!(!parent.is_cancelled());
    assert!(sibling_control.outcome().is_none());
    let (_, mut sibling_output) = sibling.split();
    assert!(poll(pin!(sibling_output.next())).is_pending());
    parent.cancel();
    assert_eq!(code(sibling_output.next().await), "ERR_CANCELLED");
}
#[tokio::test]
async fn challenge_boundary_is_checked_without_inventing_retry_or_effect_guarantees() {
    for mode in [Mode::Challenge, Mode::OutputChallenge, Mode::AuthFailure] {
        let provider = Fixture::new(mode);
        let client = BindingClient::new(&provider);
        let call = client.invoke(request(), cancel()).unwrap();
        let control = call.control();
        let (_, mut output) = call.split();
        if matches!(mode, Mode::OutputChallenge) {
            assert!(matches!(output.next().await, Some(OutputEvent::Output(_))));
        }
        let Some(OutputEvent::Error(error)) = output.next().await else {
            panic!("error")
        };
        match mode {
            Mode::Challenge => {
                assert_eq!(
                    error.context_required().unwrap().message().as_str(),
                    Some("Supply a token")
                );
                assert_eq!(provider.effects.load(Ordering::SeqCst), 0);
            }
            Mode::OutputChallenge => {
                assert_eq!(error.code(), "ERR_EXECUTION_FAILED");
                assert!(error.context_required().is_none());
                assert_eq!(
                    control.boundary_failure().unwrap().kind(),
                    BoundaryFailureKind::ChallengeAfterOutput
                );
            }
            Mode::AuthFailure => {
                assert_eq!(error.code(), "custom.auth");
                assert!(error.context_required().is_none());
                assert_eq!(provider.effects.load(Ordering::SeqCst), 1);
            }
            _ => unreachable!(),
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn native_borrowed_nonclone_provider_is_send_and_retained_output_survives() {
    fn send<T: Send>(_: &T) {}
    let retained;
    {
        let name = String::from("borrowed");
        let mut provider = Fixture::new(Mode::Null);
        provider.name = &name;
        let client = BindingClient::new(Arc::new(provider));
        let call = client.invoke(request(), cancel()).unwrap();
        send(&call);
        let (_, mut output) = call.split();
        let Some(OutputEvent::Output(value)) = output.next().await else {
            panic!("value")
        };
        retained = value;
        assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    }
    assert_eq!(retained.text(), "null");
}
#[tokio::test]
async fn default_delivery_accepts_values_above_the_retired_one_mib_cap() {
    let provider = Fixture::new(Mode::Echo);
    let client = BindingClient::new(&provider);
    let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    let value = JsonValue::string(&"x".repeat(1024 * 1024)).unwrap();
    input.send(value.clone()).await.unwrap();
    input.close();
    let Some(OutputEvent::Output(delivered)) = output.next().await else {
        panic!("the default policy must deliver the parsed value")
    };
    assert_eq!(delivered.bytes(), value.bytes());
    assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
}

#[tokio::test]
async fn an_explicit_zero_value_allowance_refuses_all_values() {
    let provider = Fixture::new(Mode::Echo);
    let client = BindingClient::new(&provider)
        .with_limits(InvocationLimits {
            max_value_bytes: Some(0),
            ..Default::default()
        })
        .unwrap();
    let (mut input, _output) = client.invoke(request(), cancel()).unwrap().split();
    assert_eq!(input.send(json("0")).await, Err(WriteError::ValueTooLarge));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn delivery_limits_are_local_on_input_and_terminal_on_bad_provider_output() {
    let provider = Fixture::new(Mode::Echo);
    let client = BindingClient::new(&provider)
        .with_limits(InvocationLimits {
            max_value_bytes: Some(1),
            ..Default::default()
        })
        .unwrap();
    let (mut input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    assert_eq!(
        input.send(json("123")).await,
        Err(WriteError::ValueTooLarge)
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    input.send(json("7")).await.unwrap();
    input.close();
    assert_eq!(collect(&mut output).await.unwrap(), ["7"]);
    let provider = Fixture::new(Mode::Null);
    let client = BindingClient::new(&provider)
        .with_limits(InvocationLimits {
            max_value_bytes: Some(1),
            ..Default::default()
        })
        .unwrap();
    let call = client.invoke(request(), cancel()).unwrap();
    let control = call.control();
    let (_, mut output) = call.split();
    assert_eq!(code(output.next().await), "ERR_EXECUTION_FAILED");
    assert_eq!(
        control.boundary_failure().unwrap().kind(),
        BoundaryFailureKind::OutputResourceLimit
    );
    assert!(
        BindingClient::new(&provider)
            .with_limits(InvocationLimits {
                input_capacity: 0,
                ..Default::default()
            })
            .is_err()
    );
}
#[test]
fn no_busy_self_wake_when_provider_is_pending() {
    let provider = Fixture::new(Mode::Park);
    let client = BindingClient::new(&provider);
    let (_, mut output) = client.invoke(request(), cancel()).unwrap().split();
    let counter = Arc::new(Counter::default());
    let waker = Waker::from(counter.clone());
    let mut read = pin!(output.next());
    for _ in 0..128 {
        assert!(
            read.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
}
#[test]
fn dropped_pending_read_does_not_keep_its_waker_alive() {
    let provider = Fixture::new(Mode::Echo);
    let client = BindingClient::new(&provider);
    let (_input, mut output) = client.invoke(request(), cancel()).unwrap().split();
    let owner = Arc::new(Counter::default());
    let weak = Arc::downgrade(&owner);
    let waker = Waker::from(owner);
    {
        let mut read = pin!(output.next());
        assert!(
            read.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    drop(waker);
    assert!(weak.upgrade().is_none());
}
#[tokio::test]
async fn supplied_unusable_context_can_receive_a_safe_renewed_challenge() {
    let provider = Fixture::new(Mode::Challenge);
    let client = BindingClient::new(&provider);
    let request = request()
        .with_context(InvocationContext::try_from(json(r#"{"token":"expired"}"#)).unwrap());
    let (_, mut output) = client.invoke(request, cancel()).unwrap().split();
    assert_eq!(code(output.next().await), "CONTEXT_REQUIRED");
    assert_eq!(provider.effects.load(Ordering::SeqCst), 0);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(output.next().await.is_none());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn per_handle_cancel_before_first_poll_does_no_provider_work() {
    let provider = Fixture::new(Mode::Empty);
    let client = BindingClient::new(&provider);
    let call = client.invoke(request(), cancel()).unwrap();
    call.control().cancel();
    let (_, mut output) = call.split();
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
struct Prediction {
    answer: Result<PreflightOutcome, InvocationError>,
    terminal: Result<(), InvocationError>,
    calls: AtomicUsize,
}
impl BindingInvoker for Prediction {
    type Invoke<'a> = std::future::Ready<Result<(), InvocationError>>;
    type Preflight<'a> = std::future::Ready<Result<PreflightOutcome, InvocationError>>;
    fn invoke<'a>(
        &'a self,
        _: BindingRequest,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(self.terminal.clone())
    }
    fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        std::future::ready(self.answer.clone())
    }
}
#[tokio::test]
async fn advisory_preflight_can_challenge_or_fail_without_becoming_an_invocation_gate() {
    for answer in [
        Ok(PreflightOutcome::NoPrediction),
        Ok(PreflightOutcome::Challenge(challenge())),
        Err(InvocationError::refused()),
        Err(InvocationError::execution_failed()),
    ] {
        let provider = Prediction {
            answer: answer.clone(),
            terminal: Ok(()),
            calls: AtomicUsize::new(0),
        };
        let client = BindingClient::new(&provider);
        let result = client.preflight(request(), cancel()).unwrap().await;
        match (&answer, result) {
            (Ok(PreflightOutcome::NoPrediction), Ok(PreflightOutcome::NoPrediction)) => (),
            (Ok(PreflightOutcome::Challenge(want)), Ok(PreflightOutcome::Challenge(got))) => {
                assert_eq!(got.value().value().text(), want.value().value().text());
            }
            (Err(want), Err(got)) => assert_eq!(want.code(), got.code()),
            other => panic!("{other:?}"),
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        let (_, mut output) = client.invoke(request(), cancel()).unwrap().split();
        assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    }
}
#[tokio::test]
async fn no_preflight_prediction_does_not_suppress_a_live_context_challenge() {
    let challenge = challenge();
    let provider = Prediction {
        answer: Ok(PreflightOutcome::NoPrediction),
        terminal: Err(InvocationError::challenge(challenge.clone())),
        calls: AtomicUsize::new(0),
    };
    let client = BindingClient::new(&provider);
    assert!(matches!(
        client.preflight(request(), cancel()).unwrap().await,
        Ok(PreflightOutcome::NoPrediction)
    ));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    let (_, mut output) = client.invoke(request(), cancel()).unwrap().split();
    let Some(OutputEvent::Error(error)) = output.next().await else {
        panic!("live context challenge must remain an unsuccessful terminal");
    };
    assert_eq!(
        error.context_required().unwrap().value().value().text(),
        challenge.value().value().text()
    );
    assert!(output.next().await.is_none());
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "no automatic retry"
    );
}
#[tokio::test]
async fn oversized_terminal_and_preflight_answers_fall_back_without_data_or_retry_claim() {
    let huge = JsonValue::string(&"x".repeat(600)).unwrap();
    let error = InvocationError::new("example.application", Some(huge.clone())).unwrap();
    let big_challenge = ContextChallenge::try_from(json(&format!(
        r#"{{"message":{},"schema":{{}}}}"#,
        huge.text()
    )))
    .unwrap();
    for answer in [
        Err(error.clone()),
        Ok(PreflightOutcome::Challenge(big_challenge)),
    ] {
        let provider = Prediction {
            answer,
            terminal: Err(error.clone()),
            calls: AtomicUsize::new(0),
        };
        let client = BindingClient::new(&provider)
            .with_limits(InvocationLimits {
                max_error_bytes: 256,
                ..Default::default()
            })
            .unwrap();
        let failure = client
            .preflight(request(), cancel())
            .unwrap()
            .await
            .unwrap_err();
        assert_eq!(failure.code(), "ERR_EXECUTION_FAILED");
        assert!(failure.data().is_none());
        let call = client.invoke(request(), cancel()).unwrap();
        let control = call.control();
        let (_, mut output) = call.split();
        let Some(OutputEvent::Error(failure)) = output.next().await else {
            panic!("error")
        };
        assert_eq!(failure.code(), "ERR_EXECUTION_FAILED");
        assert!(failure.data().is_none());
        assert_eq!(
            control.boundary_failure().unwrap().kind(),
            BoundaryFailureKind::TerminalResourceLimit
        );
    }
}

#[tokio::test]
async fn cancelled_full_input_wakes_pending_submission_before_output_is_repolled() {
    let provider = Fixture::new(Mode::GateEcho);
    let client = BindingClient::new(&provider);
    let call = client.invoke(request(), cancel()).unwrap();
    let control = call.control();
    let (mut input, mut output) = call.split();
    input.send(json("1")).await.unwrap();
    assert!(poll(pin!(output.next())).is_pending());
    let counter = Arc::new(Counter::default());
    let waker = Waker::from(counter.clone());
    {
        let mut send = pin!(input.send(json("2")));
        assert!(
            send.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        control.cancel();
        assert!(counter.0.load(Ordering::SeqCst) > 0);
        assert!(matches!(
            poll(send.as_mut()),
            Poll::Ready(Err(WriteError::Closed))
        ));
    }
    assert_eq!(provider.effects.load(Ordering::SeqCst), 0);
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert_eq!(provider.live.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn dropped_and_cancelled_attempts_preserve_live_siblings_and_other_parents() {
    let external = cancel();
    let unrelated = cancel();
    let provider = Fixture::new(Mode::GateEcho);
    let client = BindingClient::new(&provider);
    let (mut ai, mut a) = client.invoke(request(), external.clone()).unwrap().split();
    let (mut bi, mut b) = client.invoke(request(), external.clone()).unwrap().split();
    let (mut ci, mut c) = client.invoke(request(), unrelated.clone()).unwrap().split();
    ai.close();
    bi.close();
    ci.close();
    for output in [&mut a, &mut b, &mut c] {
        assert!(poll(pin!(output.next())).is_pending());
    }
    drop(a);
    assert!(!external.is_cancelled() && !unrelated.is_cancelled());
    provider.gate.cancel();
    assert!(matches!(b.next().await, Some(OutputEvent::Complete)));
    assert!(matches!(c.next().await, Some(OutputEvent::Complete)));
    assert_eq!(provider.live.load(Ordering::SeqCst), 0);
    let provider = Fixture::new(Mode::Park);
    let client = BindingClient::new(&provider);
    let (_, mut a) = client.invoke(request(), external.clone()).unwrap().split();
    let (_, mut b) = client.invoke(request(), external.clone()).unwrap().split();
    let (_, mut c) = client.invoke(request(), unrelated.clone()).unwrap().split();
    for output in [&mut a, &mut b, &mut c] {
        assert!(poll(pin!(output.next())).is_pending());
    }
    a.control().cancel();
    assert_eq!(code(a.next().await), "ERR_CANCELLED");
    assert!(!external.is_cancelled() && !unrelated.is_cancelled());
    assert!(poll(pin!(b.next())).is_pending());
    external.cancel();
    assert_eq!(code(b.next().await), "ERR_CANCELLED");
    assert!(poll(pin!(c.next())).is_pending());
    assert!(!unrelated.is_cancelled());
    drop(c);
    assert_eq!(provider.live.load(Ordering::SeqCst), 0);
}

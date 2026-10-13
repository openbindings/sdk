use openbindings::{JsonValue, Source, WorkControl};
use openbindings_invocation::{
    BindingRequest, BindingValue, ContextChallenge, InvocationError, InvocationLimits, ProviderIo,
    WriteError,
    binding::{BindingInvoker, Invoker, SupportedKind},
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

fn json(s: &str) -> JsonValue {
    JsonValue::parse(s).unwrap()
}
fn source() -> Source {
    Source {
        kind: "fixture".into(),
        ..Default::default()
    }
}
fn request<'a>(s: &'a Source, b: &'a BindingValue) -> BindingRequest<'a> {
    BindingRequest {
        source: s,
        binding: b,
        context: None,
    }
}
fn challenge() -> ContextChallenge {
    ContextChallenge::new(json(r#"{"target":"","alternatives":[{"requirements":[{"type":"custom.token","name":"token","durable":false}]}]}"#)).unwrap()
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
    gate: WorkControl,
}
impl Fixture<'_> {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            name: "borrowed",
            calls: Arc::default(),
            live: Arc::default(),
            effects: Arc::default(),
            gate: WorkControl::new(),
        }
    }
}
impl BindingInvoker for Fixture<'_> {
    type Invocation<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    fn supports_kind(&self, k: &str) -> bool {
        k == "fixture"
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        vec![SupportedKind {
            kind: "fixture".into(),
            description: Some("test only".into()),
        }]
    }
    fn invoke<'a>(
        &'a self,
        _r: BindingRequest<'a>,
        mut io: ProviderIo,
        _c: WorkControl,
    ) -> Self::Invocation<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert_eq!(self.name, "borrowed");
            let _guard = Guard::new(&self.live);
            match self.mode {
                Mode::Empty => {}
                Mode::Null => io.emit(json("null")).await?,
                Mode::Echo | Mode::GateEcho => {
                    if matches!(self.mode, Mode::GateEcho) {
                        self.gate.cancelled().await;
                    }
                    while let Some(v) = io.next().await {
                        self.effects.fetch_add(1, Ordering::SeqCst);
                        io.emit(v).await?;
                    }
                }
                Mode::Server(n) => {
                    io.close_input();
                    for i in 0..n {
                        io.emit(json(&i.to_string())).await?;
                    }
                }
                Mode::Count => {
                    let mut n = 0;
                    while io.next().await.is_some() {
                        n += 1;
                        self.effects.fetch_add(1, Ordering::SeqCst);
                    }
                    io.emit(json(&n.to_string())).await?;
                }
                Mode::EarlyClose => {
                    io.close_input();
                    self.gate.cancelled().await;
                    io.emit(json("7")).await?;
                }
                Mode::OutputError => {
                    io.emit(json("1")).await?;
                    return Err(InvocationError::new("custom.failed", Some(json("null"))).unwrap());
                }
                Mode::Challenge => return Err(InvocationError::challenge(challenge())),
                Mode::OutputChallenge => {
                    io.emit(json("1")).await?;
                    return Err(InvocationError::challenge(challenge()));
                }
                Mode::AuthFailure => {
                    self.effects.fetch_add(1, Ordering::SeqCst);
                    return Err(InvocationError::new("custom.auth", None).unwrap());
                }
                Mode::Park => pending::<()>().await,
                Mode::Duplex => {
                    let (mut input, mut output) = io.split();
                    let read = async {
                        let mut n = 0;
                        while input.next().await.is_some() {
                            n += 1;
                        }
                        n
                    };
                    let write = async {
                        for i in 0..37 {
                            output.emit(json(&i.to_string())).await?;
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
    let waker = Waker::from(Arc::new(Counter::default()));
    future.poll(&mut Context::from_waker(&waker))
}
fn ready<T>(value: Poll<T>) -> T {
    match value {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("expected ready"),
    }
}
fn code(value: Option<Result<JsonValue, InvocationError>>) -> String {
    value.unwrap().unwrap_err().code().into()
}

#[tokio::test]
async fn lazy_admission_exact_support_and_default_preflight() {
    let provider = Fixture::new(Mode::Empty);
    let s = source();
    let b = BindingValue::default();
    let facade = Invoker::new(&provider);
    assert!(facade.preflight(request(&s, &b)).await.unwrap().is_none());
    drop(facade.invoke(request(&s, &b)));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        facade
            .check_kind_support(["fixture", "unknown", "fixture"])
            .unwrap()
            .iter()
            .map(|x| x.supported)
            .collect::<Vec<_>>(),
        [true, false]
    );
    assert!(facade.check_kind_support([""]).is_err());
    assert_eq!(
        facade.list_supported_kinds().unwrap()[0]
            .description
            .as_deref(),
        Some("test only")
    );
    let bad = Source {
        kind: "unknown".into(),
        ..Default::default()
    };
    let mut call = facade.invoke(request(&bad, &b));
    assert_eq!(code(call.next().await), "ERR_REFUSED");
    let mut bad_binding = BindingValue::default();
    bad_binding.extensions.insert("source".into(), json("42"));
    let mut call = facade.invoke(request(&s, &bad_binding));
    assert_eq!(code(call.next().await), "ERR_REFUSED");
    assert_eq!(call.boundary_error().unwrap().code(), "unexpected-member");
    let null = json("null");
    let mut call = facade.invoke(BindingRequest {
        context: Some(&null),
        ..request(&s, &b)
    });
    assert_eq!(code(call.next().await), "ERR_REFUSED");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn zero_null_and_output_then_failure_require_terminal_observation() {
    let s = source();
    let b = BindingValue::default();
    for mode in [Mode::Empty, Mode::Null, Mode::OutputError] {
        let p = Fixture::new(mode);
        let f = Invoker::new(&p);
        let mut call = f.invoke(request(&s, &b));
        match mode {
            Mode::Empty => assert!(call.next().await.is_none()),
            Mode::Null => {
                assert_eq!(call.next().await.unwrap().unwrap().text(), "null");
                assert!(call.next().await.is_none());
            }
            _ => {
                assert_eq!(call.next().await.unwrap().unwrap().text(), "1");
                let e = call.next().await.unwrap().unwrap_err();
                assert_eq!(e.code(), "custom.failed");
                assert_eq!(e.data().unwrap().text(), "null");
                assert!(call.next().await.is_none());
            }
        }
        assert!(call.next().await.is_none());
        assert!(call.outcome().is_some());
        assert_eq!(p.calls.load(Ordering::SeqCst), 1);
        assert_eq!(p.live.load(Ordering::SeqCst), 0);
        let before = call.outcome().unwrap().err().map(|e| e.code().to_owned());
        call.control().cancel();
        assert_eq!(
            call.outcome().unwrap().err().map(|e| e.code().to_owned()),
            before
        );
    }
}
#[tokio::test]
async fn all_cardinalities_use_same_lifecycle_and_bidi_halves_drive_each_other() {
    let s = source();
    let b = BindingValue::default();
    for mode in [Mode::Echo, Mode::Count, Mode::Duplex] {
        let p = Fixture::new(mode);
        let f = Invoker::new(&p);
        let (mut input, mut output) = f.invoke(request(&s, &b)).split();
        let write = async {
            for i in 0..37 {
                input.write(json(&i.to_string())).await.unwrap();
            }
            input.close();
        };
        let read = async {
            let mut values = Vec::new();
            while let Some(v) = output.next().await {
                values.push(v.unwrap().text().to_owned());
            }
            values
        };
        // Both results survive; a fail-fast join could hide the output terminal.
        let (_, values) = tokio::join!(write, read);
        match mode {
            Mode::Count => assert_eq!(values, ["37"]),
            _ => assert_eq!(values, (0..37).map(|i| i.to_string()).collect::<Vec<_>>()),
        }
        assert!(output.outcome().unwrap().is_ok());
    }
    let p = Fixture::new(Mode::Server(23));
    let f = Invoker::new(&p);
    let (input, mut output) = f.invoke(request(&s, &b)).split();
    drop(input);
    for i in 0..23 {
        assert_eq!(output.next().await.unwrap().unwrap().text(), i.to_string());
    }
    assert!(output.next().await.is_none());
}
#[tokio::test]
async fn accepted_input_drains_on_half_close_and_early_close_keeps_output_open() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::GateEcho);
    let f = Invoker::new(&p);
    let mut call = f.invoke(request(&s, &b));
    call.write(json("9007199254740993")).await.unwrap();
    call.close();
    assert_eq!(call.write(json("2")).await, Err(WriteError::Closed));
    p.gate.cancel();
    assert_eq!(
        call.next().await.unwrap().unwrap().text(),
        "9007199254740993"
    );
    assert!(call.next().await.is_none());
    let p = Fixture::new(Mode::EarlyClose);
    let f = Invoker::new(&p);
    let (mut input, mut output) = f.invoke(request(&s, &b)).split();
    input.closed().await;
    assert!(input.is_closed());
    assert_eq!(input.write(json("1")).await, Err(WriteError::Closed));
    assert!(output.outcome().is_none());
    p.gate.cancel();
    assert_eq!(output.next().await.unwrap().unwrap().text(), "7");
    assert!(output.next().await.is_none());
}
#[tokio::test]
async fn pending_write_drop_and_pending_read_drop_do_not_consume_or_duplicate() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::GateEcho);
    let f = Invoker::new(&p);
    let (mut input, mut output) = f.invoke(request(&s, &b)).split();
    input.write(json("1")).await.unwrap();
    {
        let mut write = pin!(input.write(json("2")));
        assert!(poll(write.as_mut()).is_pending());
    }
    {
        let mut read = pin!(output.next());
        assert!(poll(read.as_mut()).is_pending());
    }
    input.close();
    p.gate.cancel();
    assert_eq!(output.next().await.unwrap().unwrap().text(), "1");
    assert!(output.next().await.is_none());
    assert_eq!(p.effects.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn cancellation_full_queues_preserves_accepted_output_order_and_releases_job() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Server(100));
    let f = Invoker::new(&p);
    let mut call = f.invoke(request(&s, &b));
    assert_eq!(call.next().await.unwrap().unwrap().text(), "0");
    assert_eq!(p.live.load(Ordering::SeqCst), 1);
    let control = call.control();
    control.cancel();
    assert_eq!(p.live.load(Ordering::SeqCst), 0);
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    for i in 1..4 {
        assert_eq!(call.next().await.unwrap().unwrap().text(), i.to_string());
    }
    assert_eq!(code(call.next().await), "ERR_CANCELLED");
    assert!(call.next().await.is_none());
    let p = Fixture::new(Mode::GateEcho);
    let f = Invoker::new(&p);
    let (mut input, mut output) = f.invoke(request(&s, &b)).split();
    let control = output.control();
    input.write(json("1")).await.unwrap();
    let mut write = pin!(input.write(json("2")));
    assert!(poll(write.as_mut()).is_pending());
    control.cancel();
    assert_eq!(ready(poll(write.as_mut())), Err(WriteError::Closed));
    assert_eq!(code(output.next().await), "ERR_CANCELLED");
    assert_eq!(p.effects.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn pre_cancel_external_control_and_output_abandonment_do_not_cancel_siblings() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Park);
    let f = Invoker::new(&p);
    let external = WorkControl::new();
    external.cancel();
    let mut call = f.invoke_with_control(request(&s, &b), &external);
    assert_eq!(code(call.next().await), "ERR_CANCELLED");
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
    let external = WorkControl::new();
    let mut a = f.invoke_with_control(request(&s, &b), &external);
    let mut bcall = f.invoke_with_control(request(&s, &b), &external);
    {
        let mut next = pin!(a.next());
        assert!(poll(next.as_mut()).is_pending());
    }
    {
        let mut next = pin!(bcall.next());
        assert!(poll(next.as_mut()).is_pending());
    }
    assert_eq!(p.live.load(Ordering::SeqCst), 2);
    let control = a.control();
    drop(a);
    assert_eq!(p.live.load(Ordering::SeqCst), 1);
    assert!(!external.is_cancelled());
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    external.cancel();
    assert_eq!(code(bcall.next().await), "ERR_CANCELLED");
    assert_eq!(p.live.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn challenge_boundary_is_checked_without_inventing_retry_or_effect_guarantees() {
    let s = source();
    let b = BindingValue::default();
    for mode in [Mode::Challenge, Mode::OutputChallenge, Mode::AuthFailure] {
        let p = Fixture::new(mode);
        let f = Invoker::new(&p);
        let mut call = f.invoke(request(&s, &b));
        if matches!(mode, Mode::OutputChallenge) {
            assert!(call.next().await.unwrap().is_ok());
        }
        let e = call.next().await.unwrap().unwrap_err();
        match mode {
            Mode::Challenge => {
                assert_eq!(e.context_required().unwrap().target(), "");
                assert_eq!(p.effects.load(Ordering::SeqCst), 0);
            }
            Mode::OutputChallenge => {
                assert_eq!(e.code(), "ERR_EXECUTION_FAILED");
                assert!(e.context_required().is_none());
                assert_eq!(
                    call.boundary_error().unwrap().code(),
                    "challenge-after-output"
                );
            }
            Mode::AuthFailure => {
                assert_eq!(e.code(), "custom.auth");
                assert!(e.context_required().is_none());
                assert_eq!(p.effects.load(Ordering::SeqCst), 1);
            }
            _ => unreachable!(),
        }
        assert_eq!(p.calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn native_borrowed_nonclone_provider_is_send_and_retained_output_survives() {
    fn assert_send<T: Send>(_: &T) {}
    let retained;
    {
        let name = String::from("borrowed");
        let mut p = Fixture::new(Mode::Null);
        p.name = &name;
        let f = Invoker::new(Arc::new(p));
        let s = source();
        let b = BindingValue::default();
        let mut call = f.invoke(request(&s, &b));
        assert_send(&call);
        retained = call.next().await.unwrap().unwrap();
        assert!(call.next().await.is_none());
    }
    assert_eq!(retained.text(), "null");
}
#[tokio::test]
async fn delivery_limits_are_local_on_input_and_terminal_on_bad_provider_output() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Echo);
    let f = Invoker::with_limits(
        &p,
        InvocationLimits {
            max_value_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut call = f.invoke(request(&s, &b));
    assert_eq!(
        call.write(json("123")).await,
        Err(WriteError::ValueTooLarge)
    );
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
    call.write(json("7")).await.unwrap();
    call.close();
    assert_eq!(call.next().await.unwrap().unwrap().text(), "7");
    assert!(call.next().await.is_none());
    let p = Fixture::new(Mode::Null);
    let f = Invoker::with_limits(
        &p,
        InvocationLimits {
            max_value_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut call = f.invoke(request(&s, &b));
    assert_eq!(code(call.next().await), "ERR_EXECUTION_FAILED");
    assert_eq!(
        call.boundary_error().unwrap().code(),
        "output-resource-limit"
    );
    assert!(
        Invoker::with_limits(
            &p,
            InvocationLimits {
                input_capacity: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn no_busy_self_wake_when_provider_is_pending() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Park);
    let f = Invoker::new(&p);
    let mut call = f.invoke(request(&s, &b));
    let counter = Arc::new(Counter::default());
    let waker = Waker::from(counter.clone());
    let mut next = pin!(call.next());
    assert!(
        next.as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
}

struct Advisory;
impl BindingInvoker for Advisory {
    type Invocation<'a> = std::future::Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == "fixture"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: WorkControl,
    ) -> Self::Invocation<'a> {
        std::future::ready(Ok(()))
    }
}
impl openbindings_invocation::binding::BindingPreflight for Advisory {
    type Preflight<'a>
        = Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<openbindings_invocation::ContextChallenge>,
                        InvocationError,
                    >,
                > + Send
                + 'a,
        >,
    >
    where
        Self: 'a;
    fn preflight<'a>(&'a self, r: BindingRequest<'a>, _: WorkControl) -> Self::Preflight<'a> {
        Box::pin(async move {
            if r.context.is_some() {
                Err(InvocationError::new("custom.preflight", None).unwrap())
            } else {
                Ok(Some(challenge()))
            }
        })
    }
}

#[tokio::test]
async fn advisory_preflight_can_challenge_or_fail_without_becoming_an_invocation_gate() {
    let s = source();
    let b = BindingValue::default();
    let f = Invoker::new(Advisory);
    assert!(
        f.preflight_with_provider(request(&s, &b))
            .await
            .unwrap()
            .is_some()
    );
    let context = json("{}");
    let r = BindingRequest {
        context: Some(&context),
        ..request(&s, &b)
    };
    assert_eq!(
        f.preflight_with_provider(r).await.unwrap_err().code(),
        "custom.preflight"
    );
    let mut call = f.invoke(r);
    assert!(call.next().await.is_none());
}
#[tokio::test]
async fn input_and_output_wakeups_can_be_joined_inside_provider() {
    // Joined provider directions use the same bounded lifecycle; the driver
    // unit tests separately check each endpoint’s task-waker notifications.
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Duplex);
    let f = Invoker::new(&p);
    let (mut input, mut output) = f.invoke(request(&s, &b)).split();
    let write = async {
        for i in 0..37 {
            input.write(json(&i.to_string())).await.unwrap();
        }
        input.close();
    };
    let read = async {
        let mut n = 0;
        while output.next().await.is_some() {
            n += 1;
        }
        assert_eq!(n, 37);
    };
    tokio::join!(write, read);
}
#[test]
fn full_output_queue_does_not_prevent_immediate_control_cancellation() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Server(100));
    let f = Invoker::new(&p);
    let (mut input, mut output) = f.invoke(request(&s, &b)).split();
    let control = output.control();
    let mut closed = pin!(input.closed());
    ready(poll(closed.as_mut()));
    control.cancel();
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    let mut read = pin!(output.next());
    assert_eq!(ready(poll(read.as_mut())).unwrap().unwrap().text(), "0");
}
#[test]
fn dropped_pending_read_does_not_keep_its_waker_alive() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Park);
    let f = Invoker::new(&p);
    let mut call = f.invoke(request(&s, &b));
    let counter = Arc::new(Counter::default());
    let waker = Waker::from(counter.clone());
    {
        let mut read = pin!(call.next());
        assert!(
            read.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    assert_eq!(Arc::strong_count(&counter), 2);
}

#[tokio::test]
async fn supplied_unusable_context_can_receive_a_safe_renewed_challenge() {
    let s = source();
    let b = BindingValue::default();
    let context = json(r#"{"credential":"unusable-test-value"}"#);
    let p = Fixture::new(Mode::Challenge);
    let f = Invoker::new(&p);
    let mut call = f.invoke(BindingRequest {
        context: Some(&context),
        ..request(&s, &b)
    });
    assert!(
        call.next()
            .await
            .unwrap()
            .unwrap_err()
            .context_required()
            .is_some()
    );
    assert_eq!(p.effects.load(Ordering::SeqCst), 0);
    assert_eq!(p.calls.load(Ordering::SeqCst), 1);
    // The facade never resolves context or starts another attempt.
    assert!(call.next().await.is_none());
    assert_eq!(p.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn per_handle_cancel_before_first_poll_does_no_provider_work() {
    let s = source();
    let b = BindingValue::default();
    let p = Fixture::new(Mode::Empty);
    let f = Invoker::new(&p);
    let mut call = f.invoke(request(&s, &b));
    call.control().cancel();
    assert_eq!(code(call.next().await), "ERR_CANCELLED");
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
}
struct LargeFailure;
impl BindingInvoker for LargeFailure {
    type Invocation<'a> = std::future::Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == "fixture"
    }
    fn invoke<'a>(
        &'a self,
        r: BindingRequest<'a>,
        _: ProviderIo,
        _: WorkControl,
    ) -> Self::Invocation<'a> {
        std::future::ready(Err(InvocationError::new(
            "custom.large",
            r.source.content.clone(),
        )
        .unwrap()))
    }
}
impl openbindings_invocation::binding::BindingPreflight for LargeFailure {
    type Preflight<'a>
        = Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<openbindings_invocation::ContextChallenge>,
                        InvocationError,
                    >,
                > + Send
                + 'a,
        >,
    >
    where
        Self: 'a;
    fn preflight<'a>(&'a self, r: BindingRequest<'a>, _: WorkControl) -> Self::Preflight<'a> {
        Box::pin(async move {
            if r.source.content.is_none() {
                Err(InvocationError::challenge(challenge()))
            } else {
                Err(InvocationError::new("custom.large", r.source.content.clone()).unwrap())
            }
        })
    }
}

#[tokio::test]
async fn oversized_terminal_and_preflight_failure_fall_back_without_data_or_retry_claim() {
    let s = Source {
        content: Some(json(&format!("\"{}\"", "x".repeat(512)))),
        ..source()
    };
    let b = BindingValue::default();
    let f = Invoker::with_limits(
        LargeFailure,
        InvocationLimits {
            max_error_bytes: 256,
            ..Default::default()
        },
    )
    .unwrap();
    let mut call = f.invoke(request(&s, &b));
    let e = call.next().await.unwrap().unwrap_err();
    assert_eq!(e.code(), "ERR_EXECUTION_FAILED");
    assert!(e.data().is_none());
    assert_eq!(
        call.boundary_error().unwrap().code(),
        "terminal-resource-limit"
    );
    let e = f
        .preflight_with_provider(request(&s, &b))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "ERR_EXECUTION_FAILED");
    assert!(e.data().is_none());
}

#[tokio::test]
async fn preflight_requires_context_challenges_in_the_result_not_the_error_channel() {
    let s = source();
    let b = BindingValue::default();
    let f = Invoker::new(LargeFailure);
    let error = f
        .preflight_with_provider(request(&s, &b))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "ERR_EXECUTION_FAILED");
    assert!(error.context_required().is_none());
}

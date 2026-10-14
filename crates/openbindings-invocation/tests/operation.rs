use openbindings::{
    Direction, JsonValue, NoVerdict, ParsedDocument, PreparedSchema, ResourceSet, SchemaEvaluator,
    SchemaRequest, ValueOutcome, WorkControl,
};
use openbindings_invocation::{
    BindingRequest, InvocationError, ProviderIo,
    binding::BindingInvoker,
    operation::{Invoker, Request, ValidationStage},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    future::{Future, pending},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn doc(operations: &str, bindings: &str, sources: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(r#"{{"openbindings":"0.2.0","operations":{operations},"bindings":{bindings},"sources":{sources}}}"#)).unwrap()
}
fn one(operation: &str) -> ParsedDocument {
    doc(
        &format!(r#"{{"echo":{operation}}}"#),
        r#"{"b":{"operation":"echo","source":"s","content":null,"x-opaque":1e9999}}"#,
        r#"{"s":{"kind":"fixture","content":null,"x-opaque":9007199254740993}}"#,
    )
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
struct Provider<'a> {
    mode: Mode,
    name: &'a str,
    calls: AtomicUsize,
    received: AtomicUsize,
    eof: AtomicUsize,
    live: Arc<AtomicUsize>,
    seen: Mutex<Option<(String, String, Option<String>)>>,
}
impl Provider<'_> {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            name: "fixture",
            calls: AtomicUsize::new(0),
            received: AtomicUsize::new(0),
            eof: AtomicUsize::new(0),
            live: Arc::default(),
            seen: Mutex::default(),
        }
    }
}
struct Guard(Arc<AtomicUsize>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl BindingInvoker for Provider<'_> {
    type Invocation<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == self.name
    }
    fn invoke<'a>(
        &'a self,
        r: BindingRequest<'a>,
        mut io: ProviderIo,
        _: WorkControl,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.live.fetch_add(1, Ordering::SeqCst);
            let _guard = Guard(self.live.clone());
            let snapshot = openbindings_invocation::BindingInvocationInput {
                source: r.source.clone(),
                binding: r.binding.clone(),
                context: r.context.cloned(),
            }
            .to_json()
            .unwrap();
            *self.seen.lock().unwrap() = Some((
                snapshot.get("source").unwrap().text().into(),
                r.binding.to_json().unwrap().text().into(),
                r.context.map(|v| v.text().into()),
            ));
            match self.mode {
                Mode::Empty => (),
                Mode::Park => pending::<()>().await,
                Mode::Outputs => {
                    io.emit(json("1")).await?;
                    io.emit(json("\"bad\"")).await?;
                }
                Mode::Challenge => return Err(challenge()),
                Mode::Error => {
                    return Err(
                        InvocationError::new("fixture.application", Some(json("null"))).unwrap(),
                    );
                }
                Mode::Echo => {
                    while let Some(value) = io.next().await {
                        self.received.fetch_add(1, Ordering::SeqCst);
                        io.emit(value).await?;
                    }
                    self.eof.fetch_add(1, Ordering::SeqCst);
                }
            }
            Ok(())
        })
    }
}
impl openbindings_invocation::binding::BindingPreflight for Provider<'_> {
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
    fn preflight<'a>(&'a self, _: BindingRequest<'a>, _: WorkControl) -> Self::Preflight<'a> {
        Box::pin(async move { Ok(challenge().context_required()) })
    }
}

fn challenge() -> InvocationError {
    InvocationError::new("CONTEXT_REQUIRED",Some(json(r#"{"target":"opaque","alternatives":[{"requirements":[{"type":"test.custom","x":1e9999}]}]}"#))).unwrap()
}
async fn outcome<P: BindingInvoker>(
    invoker: &Invoker<P>,
    request: Request<'_>,
) -> Result<(), InvocationError> {
    let mut call = invoker.invoke(request);
    call.close();
    while let Some(item) = call.next().await {
        item?;
    }
    call.outcome().unwrap()
}
fn assert_code(result: Result<(), InvocationError>, code: &str) {
    let e = result.unwrap_err();
    assert_eq!(e.code(), code);
    assert!(e.data().is_none());
}
fn checked<'a, 'b>(
    document: ParsedDocument,
    provider: &'a Provider<'b>,
) -> Invoker<&'a Provider<'b>> {
    Invoker::new(document, provider)
        .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn names_ordered_choices_and_explicit_bindings_do_not_invent_policy() {
    let p = Provider::new(Mode::Empty);
    let d = doc(
        r#"{"echo":{"aliases":["say"]},"other":{}}"#,
        r#"{"a":{"operation":"echo","source":"s","preference":99},"b":{"operation":"echo","source":"s","deprecated":true},"unsupported":{"operation":"echo","source":"u"},"foreign":{"operation":"other","source":"s"},"missing-source":{"operation":"echo","source":"absent"},"alias-ref":{"operation":"say","source":"s"}}"#,
        r#"{"s":{"kind":"fixture"},"u":{"kind":"other"}}"#,
    );
    let invoke = Invoker::new(d, &p);
    assert_code(
        outcome(&invoke, Request::operation("say")).await,
        "ERR_BINDING_SELECTION_REQUIRED",
    );
    assert_code(
        outcome(&invoke, Request::operation("absent")).await,
        "ERR_OPERATION_NOT_FOUND",
    );
    for keys in [
        &["a", "absent"][..],
        &["a", "foreign"],
        &["unsupported"],
        &["missing-source"],
    ] {
        assert_code(
            outcome(&invoke, Request::ordered("say", keys)).await,
            "ERR_BINDING_NOT_FOUND",
        );
    }
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
    outcome(&invoke, Request::ordered("say", &["unsupported", "b", "a"]))
        .await
        .unwrap();
    assert!(
        p.seen
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .1
            .contains("\"deprecated\":true")
    );
    outcome(&invoke, Request::ordered("say", &["a", "a"]))
        .await
        .unwrap();
    assert_code(
        outcome(&invoke, Request::ordered("say", &[])).await,
        "ERR_REFUSED",
    );
    assert_code(
        outcome(&invoke, Request::binding("missing-source")).await,
        "ERR_UNKNOWN_SOURCE",
    );
    assert_code(
        outcome(&invoke, Request::binding("unsupported")).await,
        "ERR_BINDING_NOT_FOUND",
    );
    assert_code(
        outcome(&invoke, Request::binding("alias-ref")).await,
        "ERR_OPERATION_NOT_FOUND",
    );
    assert_code(
        outcome(&invoke, Request::binding("absent")).await,
        "ERR_BINDING_NOT_FOUND",
    );
    assert_eq!(p.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        invoke
            .preflight_with_provider(Request::ordered("say", &["a", "absent"]))
            .await
            .unwrap_err()
            .code(),
        "ERR_BINDING_NOT_FOUND"
    );
    assert!(
        invoke
            .preflight_with_provider(Request::binding("a"))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(p.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test(flavor = "current_thread")]
async fn missing_ambiguous_and_sole_candidates() {
    let p = Provider::new(Mode::Empty);
    for operation in [
        r#"{"echo":{"aliases":["echo"]}}"#,
        r#"{"echo":{},"other":{"aliases":["echo"]}}"#,
    ] {
        let i = Invoker::new(
            doc(
                operation,
                r#"{"b":{"operation":"echo","source":"s"}}"#,
                r#"{"s":{"kind":"fixture"}}"#,
            ),
            &p,
        );
        assert_code(
            outcome(&i, Request::operation("echo")).await,
            "ERR_OPERATION_NOT_FOUND",
        );
        assert_code(
            outcome(&i, Request::binding("b")).await,
            "ERR_OPERATION_NOT_FOUND",
        );
    }
    let i = Invoker::new(doc(r#"{"echo":{}}"#, "{}", "{}"), &p);
    assert_code(
        outcome(&i, Request::operation("echo")).await,
        "ERR_BINDING_NOT_FOUND",
    );
    outcome(
        &Invoker::new(one(r#"{"aliases":["say"]}"#), &p),
        Request::operation("say"),
    )
    .await
    .unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn exact_opaque_values_carriers_and_context_are_preserved() {
    let p = Provider::new(Mode::Echo);
    let i = Invoker::new(one("{}"), &p);
    let context = json(r#"{"secret":1e9999,"duplicate":1,"duplicate":2}"#);
    let mut call = i.invoke(Request::binding("b").with_context(&context));
    for value in ["null", "9007199254740993", "1e9999", r#"{"x":1,"x":2}"#] {
        call.write(json(value)).await.unwrap();
        assert_eq!(call.next().await.unwrap().unwrap().text(), value);
    }
    call.close();
    assert!(call.next().await.is_none());
    assert!(call.outcome().unwrap().is_ok());
    assert!(call.validation_failure().is_none());
    let seen = p.seen.lock().unwrap();
    let (source, binding, c) = seen.as_ref().unwrap();
    assert_eq!(
        source,
        r#"{"kind":"fixture","content":null,"x-opaque":9007199254740993}"#
    );
    assert_eq!(
        binding,
        r#"{"operation":"echo","source":"s","content":null,"x-opaque":1e9999}"#
    );
    assert_eq!(c.as_deref(), Some(context.text()));
}
#[tokio::test(flavor = "current_thread")]
async fn late_input_failure_is_not_eof_and_keeps_prior_outputs() {
    let p = Provider::new(Mode::Echo);
    let i = checked(
        one(r#"{"input":{"type":"integer"},"output":{"type":"integer"}}"#),
        &p,
    );
    let mut call = i.invoke(Request::operation("echo"));
    call.write(json("1")).await.unwrap();
    call.write(json("\"bad\"")).await.unwrap();
    assert_eq!(call.next().await.unwrap().unwrap().text(), "1");
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
    assert!(call.next().await.is_none());
    assert_eq!(p.received.load(Ordering::SeqCst), 1);
    assert_eq!(p.eof.load(Ordering::SeqCst), 0);
    assert_eq!(p.live.load(Ordering::SeqCst), 0);
    let failure = call.validation_failure().unwrap();
    assert_eq!(failure.direction(), Direction::Input);
    assert_eq!(failure.stage(), ValidationStage::Evaluation);
    assert!(matches!(failure.outcome(), ValueOutcome::Fails { .. }));
    assert!(call.outcome().unwrap().unwrap_err().data().is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn output_failure_withholds_only_failing_value() {
    let p = Provider::new(Mode::Outputs);
    let i = checked(one(r#"{"output":{"type":"integer"}}"#), &p);
    let mut call = i.invoke(Request::operation("echo"));
    assert_eq!(call.next().await.unwrap().unwrap().text(), "1");
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
    assert!(call.next().await.is_none());
    assert_eq!(
        call.validation_failure().unwrap().direction(),
        Direction::Output
    );
}
#[tokio::test(flavor = "current_thread")]
async fn preparation_is_lazy_absence_is_not_true_and_no_verdict_is_not_failure() {
    let p = Provider::new(Mode::Empty);
    let i = Invoker::new(
        one(r#"{"input":false,"output":{"$ref":"https://absent.test/schema"}}"#),
        &p,
    );
    outcome(&i, Request::operation("echo")).await.unwrap();
    assert!(
        i.preflight_with_provider(Request::operation("echo"))
            .await
            .unwrap()
            .is_some()
    );
    let p = Provider::new(Mode::Echo);
    for schema in [r#"{"input":true}"#, r#"{"input":false}"#] {
        let i = Invoker::new(one(schema), &p);
        let mut call = i.invoke(Request::operation("echo"));
        call.write(json("null")).await.unwrap();
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            "ERR_SCHEMA_UNRESOLVED"
        );
        assert_eq!(
            call.validation_failure().unwrap().stage(),
            ValidationStage::Preparation
        );
    }
    let i = checked(
        one(r#"{"input":{"$ref":"https://absent.test/schema"}}"#),
        &p,
    );
    let mut call = i.invoke(Request::operation("echo"));
    call.write(json("1")).await.unwrap();
    call.close();
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "ERR_SCHEMA_UNRESOLVED"
    );
    assert!(matches!(
        call.validation_failure().unwrap().outcome(),
        ValueOutcome::NoVerdict { .. }
    ));
    assert_eq!(p.received.load(Ordering::SeqCst), 0);
}
#[tokio::test(flavor = "current_thread")]
async fn binding_errors_and_challenges_bypass_output_contract() {
    for mode in [Mode::Challenge, Mode::Error] {
        let p = Provider::new(mode);
        let i = checked(one(r#"{"output":false}"#), &p);
        let error = outcome(&i, Request::operation("echo")).await.unwrap_err();
        let expected = match mode {
            Mode::Challenge => challenge(),
            _ => InvocationError::new("fixture.application", Some(json("null"))).unwrap(),
        };
        assert_eq!(
            error.to_json().unwrap().text(),
            expected.to_json().unwrap().text()
        );
    }
}
struct Counted {
    prepares: Arc<AtomicUsize>,
    evaluations: Arc<AtomicUsize>,
}
struct Prepared {
    inner: Arc<dyn PreparedSchema>,
    evaluations: Arc<AtomicUsize>,
}
impl SchemaEvaluator for Counted {
    fn prepare(
        &self,
        r: &SchemaRequest,
        c: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Prepared {
            inner: DefaultEvaluator::new().prepare(r, c)?,
            evaluations: self.evaluations.clone(),
        }))
    }
}
impl PreparedSchema for Prepared {
    fn validate(&self, v: &JsonValue, c: &WorkControl) -> ValueOutcome {
        self.evaluations.fetch_add(1, Ordering::SeqCst);
        self.inner.validate(v, c)
    }
}
#[tokio::test(flavor = "current_thread")]
async fn prepared_contracts_reused_across_values_and_attempts() {
    let p = Provider::new(Mode::Echo);
    let prepares = Arc::new(AtomicUsize::new(0));
    let evaluations = Arc::new(AtomicUsize::new(0));
    let i = Invoker::new(one(r#"{"input":true,"output":true}"#), &p)
        .with_evaluator(
            Arc::new(Counted {
                prepares: prepares.clone(),
                evaluations: evaluations.clone(),
            }),
            ResourceSet::default(),
        )
        .unwrap();
    for _ in 0..3 {
        let mut call = i.invoke(Request::operation("echo"));
        for _ in 0..20 {
            call.write(json("1")).await.unwrap();
            assert!(call.next().await.unwrap().is_ok());
        }
        call.close();
        assert!(call.next().await.is_none());
    }
    assert_eq!(prepares.load(Ordering::SeqCst), 2);
    assert_eq!(evaluations.load(Ordering::SeqCst), 120);
}
#[tokio::test(flavor = "current_thread")]
async fn borrowed_send_and_local_rc_and_cancellation_release() {
    fn send<T: Send>(_: &T) {}
    let name = String::from("fixture");
    let mut p = Provider::new(Mode::Park);
    p.name = &name;
    let i = Invoker::new(one("{}"), &p);
    let mut call = i.invoke(Request::operation("echo"));
    send(&call);
    call.write(json("null")).await.unwrap();
    let control = call.control();
    control.cancel();
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    assert_eq!(p.live.load(Ordering::SeqCst), 0);
    assert!(control.validation_failure().is_none());
    let external = WorkControl::new();
    external.cancel();
    let mut call = i.invoke_with_control(Request::operation("echo"), &external);
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    assert_eq!(p.calls.load(Ordering::SeqCst), 1);
    let local = std::rc::Rc::new(Provider::new(Mode::Empty));
    outcome(&Invoker::new(one("{}"), local), Request::operation("echo"))
        .await
        .unwrap();
}

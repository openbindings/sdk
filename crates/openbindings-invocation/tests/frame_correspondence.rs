//! Deterministic SDK test correspondence, not a general remote transport or a
//! portable streaming fixture standard. Envelope and temporal oracles are
//! independent of the native provider's emitted values and effect counters.

use openbindings::{JsonRef, JsonValue};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingInvocationInput, BindingRequest, InvocationError, InvocationLimits, ProviderIo,
    WriteError,
    binding::{BindingInvoker, Invoker},
};
use std::{
    collections::HashSet,
    future::{Future, pending},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const ROLE: &str = include_str!("fixtures/binding-invoker-0.1.json");
const OPEN: &str = r#"{"kind":"open","input":{"source":{"kind":"fixture.frames"},"binding":{}}}"#;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

// These envelopes match the pinned role shapes. The mapper's malformed-frame
// classification, admission order and lifecycle traces are this SDK's policy;
// interface adoption does not prescribe these internal choices.
fn fields(kind: &str, output: bool) -> Option<&'static [&'static str]> {
    match (kind, output) {
        ("open", false) => Some(&["kind", "input"]),
        ("input", false) | ("output", true) => Some(&["kind", "value"]),
        ("close", false) | ("input_closed" | "complete", true) => Some(&["kind"]),
        ("error", true) => Some(&["kind", "error"]),
        _ => None,
    }
}

fn envelope(value: JsonRef<'_>, output: bool) -> Result<&str, InvocationError> {
    let kind = value
        .get("kind")
        .and_then(JsonRef::as_str)
        .ok_or_else(InvocationError::frame_protocol)?;
    let allowed = fields(kind, output).ok_or_else(InvocationError::frame_protocol)?;
    let members = value
        .members()
        .ok_or_else(InvocationError::frame_protocol)?;
    let mut seen = HashSet::new();
    for member in members {
        let name = member
            .name
            .as_str()
            .ok_or_else(InvocationError::frame_protocol)?;
        if !allowed.contains(&name) || !seen.insert(name) {
            return Err(InvocationError::frame_protocol());
        }
    }
    if seen.len() != allowed.len() {
        return Err(InvocationError::frame_protocol());
    }
    Ok(kind)
}

enum Action {
    Open(Box<BindingInvocationInput>),
    Input(JsonValue),
    Close,
    Ignore,
}

#[derive(Default)]
struct Ingress {
    opened: bool,
    input_closed: bool,
    terminal: bool,
}

impl Ingress {
    fn receive(&mut self, frame: &JsonValue) -> Result<Action, InvocationError> {
        if self.terminal {
            return Ok(Action::Ignore);
        }
        let result = self.live_receive(frame);
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
    fn live_receive(&mut self, frame: &JsonValue) -> Result<Action, InvocationError> {
        let kind = envelope(frame.view(), false)?;
        if (!self.opened && kind != "open") || (self.opened && kind == "open") {
            return Err(InvocationError::frame_protocol());
        }
        match kind {
            "open" => {
                let request = BindingInvocationInput::from_json_ref(frame.get("input").unwrap())
                    .map_err(|_| InvocationError::refused())?;
                self.opened = true;
                Ok(Action::Open(Box::new(request)))
            }
            "close" => {
                self.input_closed = true;
                Ok(Action::Close)
            }
            "input" if self.input_closed => Ok(Action::Ignore),
            "input" => Ok(Action::Input(frame.get("value").unwrap().to_owned())),
            _ => unreachable!("validated envelope"),
        }
    }
    fn provider_closed(&mut self) {
        self.input_closed = true;
    }
}

// Test-only ordered wire encoder. A settled terminal is independent of the
// consumer reading it. The native output sequence feeds it in the tests below.
#[derive(Default)]
struct Egress {
    frames: Vec<JsonValue>,
    terminal: bool,
}

impl Egress {
    fn output(&mut self, value: &JsonValue) -> bool {
        if self.terminal {
            return false;
        }
        self.frames.push(json(&format!(
            r#"{{"kind":"output","value":{}}}"#,
            value.text()
        )));
        true
    }
    fn input_closed(&mut self) {
        assert!(!self.terminal);
        self.frames.push(json(r#"{"kind":"input_closed"}"#));
    }
    fn settle(&mut self, outcome: Result<(), InvocationError>) -> bool {
        if self.terminal {
            return false;
        }
        self.terminal = true;
        self.frames.push(match outcome {
            Ok(()) => json(r#"{"kind":"complete"}"#),
            Err(error) => json(&format!(
                r#"{{"kind":"error","error":{}}}"#,
                error.to_json().unwrap().text()
            )),
        });
        true
    }
    fn text(&self) -> Vec<&str> {
        self.frames.iter().map(JsonValue::text).collect()
    }
}

#[derive(Default)]
struct Consumer {
    values: Vec<JsonValue>,
    input_closed: bool,
    terminal: Option<Result<(), InvocationError>>,
}

impl Consumer {
    fn receive(&mut self, frame: &JsonValue) -> Result<(), InvocationError> {
        if self.terminal.is_some() {
            return Err(InvocationError::frame_protocol());
        }
        let result = self.live_receive(frame);
        if let Err(error) = &result {
            self.terminal = Some(Err(error.clone()));
        }
        result
    }
    fn live_receive(&mut self, frame: &JsonValue) -> Result<(), InvocationError> {
        match envelope(frame.view(), true)? {
            "output" => self.values.push(frame.get("value").unwrap().to_owned()),
            "input_closed" => self.input_closed = true,
            "complete" => self.terminal = Some(Ok(())),
            "error" => {
                let error = InvocationError::from_json_ref(frame.get("error").unwrap())
                    .map_err(|_| InvocationError::frame_protocol())?;
                if !self.values.is_empty() && error.code() == "CONTEXT_REQUIRED" {
                    return Err(InvocationError::frame_protocol());
                }
                self.terminal = Some(Err(error));
            }
            _ => unreachable!("validated envelope"),
        }
        Ok(())
    }
    fn outer_transport_closed(&mut self) {
        if self.terminal.is_none() {
            self.terminal = Some(Err(InvocationError::transport_closed()));
        }
    }
    fn code(&self) -> Option<&str> {
        self.terminal
            .as_ref()
            .and_then(|outcome| outcome.as_ref().err())
            .map(InvocationError::code)
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Echo,
    NoInput,
    Empty,
    OutputFailure,
    QueuedBeforeCancel,
}

struct Fixture {
    mode: Mode,
    attempts: Arc<AtomicUsize>,
    accepted_outputs: Arc<AtomicUsize>,
}

impl Fixture {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            attempts: Arc::default(),
            accepted_outputs: Arc::default(),
        }
    }
}

impl BindingInvoker for Fixture {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "fixture.frames"
    }
    fn invoke<'a>(
        &'a self,
        _request: BindingRequest<'a>,
        mut io: ProviderIo,
        _control: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            match self.mode {
                Mode::Echo => {
                    while let Some(value) = io.next().await {
                        io.emit(value).await?;
                        self.accepted_outputs.fetch_add(1, Ordering::SeqCst);
                    }
                }
                Mode::NoInput => {
                    io.close_input();
                    io.emit(json("null")).await?;
                    self.accepted_outputs.fetch_add(1, Ordering::SeqCst);
                }
                Mode::Empty => {}
                Mode::OutputFailure => {
                    io.emit(json("9007199254740993")).await?;
                    self.accepted_outputs.fetch_add(1, Ordering::SeqCst);
                    return Err(
                        InvocationError::new("fixture/application", Some(json("null"))).unwrap(),
                    );
                }
                Mode::QueuedBeforeCancel => {
                    for value in ["1e9999", r#"{"same":1,"same":2}"#, "3"] {
                        io.emit(json(value)).await?;
                        self.accepted_outputs.fetch_add(1, Ordering::SeqCst);
                    }
                    pending::<()>().await;
                    unreachable!("fixture remains pending until cancelled")
                }
            }
            Ok(())
        })
    }
}

fn opened(ingress: &mut Ingress) -> BindingInvocationInput {
    let Action::Open(input) = ingress.receive(&json(OPEN)).unwrap() else {
        panic!("expected open")
    };
    *input
}

fn assert_error(result: Result<Action, InvocationError>, code: &str) {
    match result {
        Err(error) => assert_eq!(error.code(), code),
        Ok(_) => panic!("expected {code}"),
    }
}

async fn start_native(frame: &JsonValue, provider: &Fixture) -> Result<(), InvocationError> {
    let Action::Open(input) = Ingress::default().receive(frame)? else {
        unreachable!("a live initial envelope exposes only open or a terminal error")
    };
    let invoker = Invoker::new(provider);
    let mut invocation = invoker.invoke(input.request());
    invocation.close();
    while let Some(item) = invocation.next().await {
        item?;
    }
    invocation.outcome().expect("observed terminal")
}

#[test]
fn pinned_role_has_the_complete_closed_frame_envelopes_used_by_the_mapper() {
    let role = json(ROLE);
    for (schema, output) in [
        ("BindingInvokerInputFrame", false),
        ("BindingInvokerOutputFrame", true),
    ] {
        let variants = role.at(&format!("/schemas/{schema}/oneOf")).unwrap();
        assert_eq!(variants.len(), Some(if output { 4 } else { 3 }));
        for variant in variants.elements().unwrap() {
            assert_eq!(variant.get("type").unwrap().as_str(), Some("object"));
            assert_eq!(
                variant.get("additionalProperties").unwrap().as_bool(),
                Some(false)
            );
            let kind = variant
                .at("/properties/kind/const")
                .unwrap()
                .as_str()
                .unwrap();
            let expected: HashSet<_> = fields(kind, output).unwrap().iter().copied().collect();
            let required: HashSet<_> = variant
                .get("required")
                .unwrap()
                .elements()
                .unwrap()
                .map(|v| v.as_str().unwrap())
                .collect();
            let properties: HashSet<_> = variant
                .get("properties")
                .unwrap()
                .members()
                .unwrap()
                .map(|m| m.name.as_str().unwrap())
                .collect();
            assert_eq!(required, expected);
            assert_eq!(properties, expected);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_envelopes_and_first_nonopen_frames_are_protocol_errors_before_work() {
    let provider = Fixture::new(Mode::Echo);
    for frame in [
        "null",
        "[]",
        "{}",
        r#"{"kind":null}"#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":"open"}"#,
        r#"{"kind":"open","input":{},"extra":0}"#,
        r#"{"kind":"open","input":{},"kind":"open"}"#,
        r#"{"kind":"open","input":{},"input":{}}"#,
        r#"{"kind":"close"}"#,
        r#"{"kind":"input","value":null}"#,
    ] {
        let mut ingress = Ingress::default();
        assert_error(ingress.receive(&json(frame)), "ERR_FRAME_PROTOCOL");
        assert_eq!(
            start_native(&json(frame), &provider)
                .await
                .unwrap_err()
                .code(),
            "ERR_FRAME_PROTOCOL"
        );
        assert!(matches!(
            ingress.receive(&json(OPEN)).unwrap(),
            Action::Ignore
        ));
    }
    // No Open action is exposed, so no provider construction/invocation occurs.
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 0);
    start_native(&json(OPEN), &provider).await.unwrap();
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn valid_open_envelopes_refuse_invalid_carried_payloads_before_work() {
    let provider = Fixture::new(Mode::Echo);
    for payload in [
        "null",
        "{}",
        r#"{"source":{"kind":"fixture.frames"}}"#,
        r#"{"source":{"kind":"fixture.frames"},"binding":{},"extra":0}"#,
        r#"{"source":{"kind":"fixture.frames","location":"legacy"},"binding":{}}"#,
        r#"{"source":{"kind":"fixture.frames","kind":"other"},"binding":{}}"#,
        r#"{"source":{"kind":"fixture.frames"},"binding":{"selector":"legacy"}}"#,
        r#"{"source":{"kind":"fixture.frames"},"binding":{},"context":null}"#,
    ] {
        let frame = json(&format!(r#"{{"kind":"open","input":{payload}}}"#));
        assert_eq!(
            start_native(&frame, &provider).await.unwrap_err().code(),
            "ERR_REFUSED"
        );
    }
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 0);
    start_native(&json(OPEN), &provider).await.unwrap();
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn second_open_is_a_protocol_error_even_with_bad_payload_or_after_input_closure() {
    for duplicate in [
        OPEN,
        r#"{"kind":"open","input":null}"#,
        r#"{"kind":"open","input":{}}"#,
        r#"{"kind":"open"}"#,
    ] {
        for closed in [false, true] {
            let mut ingress = Ingress::default();
            opened(&mut ingress);
            if closed {
                ingress.provider_closed();
            }
            assert_error(ingress.receive(&json(duplicate)), "ERR_FRAME_PROTOCOL");
        }
    }
}

#[test]
fn late_input_is_ignored_only_after_its_envelope_passes_validation() {
    for provider_closed in [false, true] {
        let mut ingress = Ingress::default();
        opened(&mut ingress);
        if provider_closed {
            ingress.provider_closed();
        } else {
            assert!(matches!(
                ingress.receive(&json(r#"{"kind":"close"}"#)).unwrap(),
                Action::Close
            ));
            assert!(matches!(
                ingress.receive(&json(r#"{"kind":"close"}"#)).unwrap(),
                Action::Close
            ));
        }
        assert!(matches!(
            ingress
                .receive(&json(r#"{"kind":"input","value":{"n":1,"n":2}}"#))
                .unwrap(),
            Action::Ignore
        ));
        for malformed in [
            r#"{"kind":"input"}"#,
            r#"{"kind":"input","value":null,"extra":0}"#,
            r#"{"kind":"input","value":1,"value":2}"#,
            r#"{"kind":"unknown"}"#,
            r#"{"kind":"close","extra":0}"#,
        ] {
            let mut fresh = Ingress::default();
            opened(&mut fresh);
            fresh.provider_closed();
            assert_error(fresh.receive(&json(malformed)), "ERR_FRAME_PROTOCOL");
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn echo_frames_correspond_to_native_half_close_exact_values_and_complete() {
    let provider = Fixture::new(Mode::Echo);
    let mut ingress = Ingress::default();
    let request = opened(&mut ingress);
    let invoker = Invoker::new(&provider);
    let mut invocation = invoker.invoke(request.request());
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 0);
    for frame in [
        r#"{"kind":"input","value":9007199254740993}"#,
        r#"{"kind":"input","value":null}"#,
        r#"{"kind":"close"}"#,
        r#"{"kind":"close"}"#,
        r#"{"kind":"input","value":"late"}"#,
    ] {
        match ingress.receive(&json(frame)).unwrap() {
            Action::Input(value) => invocation.write(value).await.unwrap(),
            Action::Close => invocation.close(),
            Action::Ignore => assert_eq!(
                invocation.write(json("\"late\"")).await,
                Err(WriteError::Closed)
            ),
            Action::Open(_) => panic!("second open"),
        }
    }
    let mut egress = Egress::default();
    while let Some(item) = invocation.next().await {
        egress.output(&item.unwrap());
    }
    assert!(invocation.outcome().unwrap().is_ok());
    egress.settle(Ok(()));
    assert_eq!(
        egress.text(),
        [
            r#"{"kind":"output","value":9007199254740993}"#,
            r#"{"kind":"output","value":null}"#,
            r#"{"kind":"complete"}"#
        ]
    );
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.accepted_outputs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn provider_input_closure_needs_no_caller_close_and_keeps_output_open() {
    let provider = Fixture::new(Mode::NoInput);
    let mut ingress = Ingress::default();
    let request = opened(&mut ingress);
    let invoker = Invoker::new(&provider);
    let (mut input, mut outputs) = invoker.invoke(request.request()).split();
    input.closed().await;
    ingress.provider_closed();
    let mut egress = Egress::default();
    egress.input_closed();
    assert!(matches!(
        ingress
            .receive(&json(r#"{"kind":"input","value":0}"#))
            .unwrap(),
        Action::Ignore
    ));
    assert_eq!(input.write(json("0")).await, Err(WriteError::Closed));
    egress.output(&outputs.next().await.unwrap().unwrap());
    assert!(outputs.next().await.is_none());
    egress.settle(outputs.outcome().unwrap());
    assert_eq!(
        egress.text(),
        [
            r#"{"kind":"input_closed"}"#,
            r#"{"kind":"output","value":null}"#,
            r#"{"kind":"complete"}"#
        ]
    );
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn native_empty_completion_and_output_then_failure_have_distinct_terminal_frames() {
    for (mode, expected) in [
        (Mode::Empty, vec![r#"{"kind":"complete"}"#]),
        (
            Mode::OutputFailure,
            vec![
                r#"{"kind":"output","value":9007199254740993}"#,
                r#"{"kind":"error","error":{"code":"fixture/application","data":null}}"#,
            ],
        ),
    ] {
        let provider = Fixture::new(mode);
        let request = opened(&mut Ingress::default());
        let invoker = Invoker::new(&provider);
        let mut invocation = invoker.invoke(request.request());
        let mut egress = Egress::default();
        while let Some(item) = invocation.next().await {
            match item {
                Ok(value) => {
                    egress.output(&value);
                }
                Err(error) => {
                    egress.settle(Err(error));
                }
            }
        }
        egress.settle(invocation.outcome().unwrap());
        assert!(invocation.next().await.is_none());
        assert_eq!(egress.text(), expected);
        let mut consumer = Consumer::default();
        for frame in &egress.frames {
            consumer.receive(frame).unwrap();
        }
        consumer.outer_transport_closed();
        assert_ne!(consumer.code(), Some("ERR_TRANSPORT_CLOSED"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_drains_committed_outputs_before_one_terminal_frame() {
    let provider = Fixture::new(Mode::QueuedBeforeCancel);
    let request = opened(&mut Ingress::default());
    let invoker = Invoker::with_limits(
        &provider,
        InvocationLimits {
            output_capacity: 2,
            ..InvocationLimits::default()
        },
    )
    .unwrap();
    let mut invocation = invoker.invoke(request.request());
    let control = invocation.control();
    let mut egress = Egress::default();
    // Polling this write starts the provider: two outputs fill its queue and
    // the third emission parks. No output consumer has read a value yet.
    invocation.write(json("null")).await.unwrap();
    assert_eq!(provider.accepted_outputs.load(Ordering::SeqCst), 2);
    control.cancel();
    assert_eq!(
        control.outcome().unwrap().unwrap_err().code(),
        "ERR_CANCELLED"
    );
    egress.output(&invocation.next().await.unwrap().unwrap());
    egress.output(&invocation.next().await.unwrap().unwrap());
    egress.settle(Err(invocation.next().await.unwrap().unwrap_err()));
    control.cancel();
    assert!(invocation.next().await.is_none());
    assert!(!egress.settle(Ok(())));
    assert!(!egress.output(&json("3")));
    assert_eq!(
        egress.text(),
        [
            r#"{"kind":"output","value":1e9999}"#,
            r#"{"kind":"output","value":{"same":1,"same":2}}"#,
            r#"{"kind":"error","error":{"code":"ERR_CANCELLED"}}"#
        ]
    );
    assert_eq!(provider.attempts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.accepted_outputs.load(Ordering::SeqCst), 2);
}

#[test]
fn outer_closure_synthesizes_failure_only_when_no_inner_terminal_was_seen() {
    for frames in [
        vec![],
        vec![r#"{"kind":"input_closed"}"#],
        vec![r#"{"kind":"output","value":null}"#],
    ] {
        let mut consumer = Consumer::default();
        for frame in frames {
            consumer.receive(&json(frame)).unwrap();
        }
        assert!(consumer.terminal.is_none());
        consumer.outer_transport_closed();
        consumer.outer_transport_closed();
        assert_eq!(consumer.code(), Some("ERR_TRANSPORT_CLOSED"));
    }
    for terminal in [
        r#"{"kind":"complete"}"#,
        r#"{"kind":"error","error":{"code":"fixture/failure"}}"#,
    ] {
        let mut consumer = Consumer::default();
        consumer.receive(&json(terminal)).unwrap();
        consumer.outer_transport_closed();
        assert_ne!(consumer.code(), Some("ERR_TRANSPORT_CLOSED"));
        assert_eq!(
            consumer
                .receive(&json(r#"{"kind":"output","value":1}"#))
                .unwrap_err()
                .code(),
            "ERR_FRAME_PROTOCOL"
        );
        assert_ne!(consumer.code(), Some("ERR_FRAME_PROTOCOL")); // First terminal remains authoritative.
    }
}

#[test]
fn malformed_output_and_invalid_safe_challenges_are_not_portable_successes() {
    for frame in [
        r#"{"kind":"complete","extra":0}"#,
        r#"{"kind":"output"}"#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":"error","error":{"code":""}}"#,
        r#"{"kind":"error","error":{"code":"CONTEXT_REQUIRED","data":null}}"#,
    ] {
        let mut consumer = Consumer::default();
        assert_eq!(
            consumer.receive(&json(frame)).unwrap_err().code(),
            "ERR_FRAME_PROTOCOL"
        );
        consumer.outer_transport_closed();
        assert_eq!(consumer.code(), Some("ERR_FRAME_PROTOCOL"));
    }
    let mut consumer = Consumer::default();
    consumer
        .receive(&json(r#"{"kind":"output","value":null}"#))
        .unwrap();
    assert_eq!(consumer.receive(&json(r#"{"kind":"error","error":{"code":"CONTEXT_REQUIRED","data":{"target":"","alternatives":[{"requirements":[{"type":"extension"}]}]}}}"#)).unwrap_err().code(), "ERR_FRAME_PROTOCOL");
}

#[test]
fn operation_role_reuses_the_same_closed_frame_protocol() {
    use serde_json_compat::Value;
    // Ignore explanatory annotations, retaining every executable schema keyword.
    fn executable(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("description");
                object.remove("title");
                for value in object.values_mut() {
                    executable(value);
                }
            }
            Value::Array(items) => {
                for value in items {
                    executable(value);
                }
            }
            _ => (),
        }
    }
    let mut binding: Value = serde_json_compat::from_str(ROLE).unwrap();
    let mut operation: Value =
        serde_json_compat::from_str(include_str!("fixtures/operation-invoker-0.1.json")).unwrap();
    executable(&mut binding);
    executable(&mut operation);
    assert_eq!(
        binding["schemas"]["BindingInvokerOutputFrame"],
        operation["schemas"]["OperationInvokerOutputFrame"]
    );
    binding["schemas"]["BindingInvokerInputFrame"]["oneOf"][0]["properties"]["input"]["$ref"] =
        Value::String("#/schemas/OperationInvocationInput".into());
    assert_eq!(
        binding["schemas"]["BindingInvokerInputFrame"],
        operation["schemas"]["OperationInvokerInputFrame"]
    );
    for schema in [
        "InvocationError",
        "BindingContext",
        "ContextRequiredDetails",
        "ContextRequirement",
    ] {
        assert_eq!(binding["schemas"][schema], operation["schemas"][schema]);
    }
    assert_eq!(
        operation["schemas"]["OperationInvocationInput"]["additionalProperties"],
        Value::Bool(false)
    );
    assert_eq!(
        operation["schemas"]["OperationInvocationInput"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

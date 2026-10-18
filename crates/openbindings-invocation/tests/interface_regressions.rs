use openbindings::{Document, JsonValue, ResourceSet, Source};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingRequest, BindingValue, ContextChallenge, InvocationError, ProviderIo,
    binding::{BindingInvoker, Invoker},
    operation::{self, Request},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    cell::Cell,
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

fn source() -> Source {
    Source {
        kind: "example.interfaces".into(),
        ..Default::default()
    }
}
fn request<'a>(source: &'a Source, binding: &'a BindingValue) -> BindingRequest<'a> {
    BindingRequest {
        source,
        binding,
        context: None,
    }
}
fn document(operation: &str) -> Document {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"run":{operation}}},"sources":{{"s":{{"kind":"example.interfaces"}},"unsupported":{{"kind":"other"}}}},"bindings":{{"b":{{"operation":"run","source":"s"}},"u":{{"operation":"run","source":"unsupported"}}}}}}"#
    )).unwrap()
}

struct InvokeOnly;
impl BindingInvoker for InvokeOnly {
    type Invocation<'a> = Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "example.interfaces"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        ready(Ok(()))
    }
}

// all selection paths and the advisory path use the specific code.
#[tokio::test]
async fn unsupported_explicit_and_ordered_bindings_are_not_found() {
    let facade = operation::Invoker::new(
        document("{}")
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant invocation document"),
        SendProvider,
    );
    for selection in [Request::binding("u"), Request::ordered("run", &["u"])] {
        let mut call = facade.invoke(selection);
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            "ERR_BINDING_NOT_FOUND"
        );
        assert!(call.boundary_error().is_none());
        assert_eq!(
            facade.preflight(selection).await.unwrap_err().code(),
            "ERR_BINDING_NOT_FOUND"
        );
    }
}

// The hook's job is the provider's invocation future, so the same bound makes
// a generic default preflight Send.
fn send_default<'a, P: BindingInvoker + Sync>(
    facade: &'a Invoker<P>,
    request: BindingRequest<'a>,
) -> impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + Send + 'a
where
    for<'b> P::Invocation<'b>: Send,
{
    facade.preflight(request)
}
#[expect(
    clippy::manual_async_fn,
    reason = "The preflight trait requires a generic Send return type"
)]
fn send_provider<'a, P>(
    binding: &'a Invoker<P>,
    operation: &'a operation::Invoker<P>,
    request: BindingRequest<'a>,
) -> impl Future<Output = ()> + Send + 'a
where
    // Plain operation calls borrow the invoker, so a provider that can be
    // shared suffices, as for binding calls.
    P: BindingInvoker + Sync,
    for<'b> P::Invocation<'b>: Send,
{
    async move {
        assert!(binding.preflight(request).await.unwrap().is_none());
        assert!(
            operation
                .preflight(Request::binding("b"))
                .await
                .unwrap()
                .is_none()
        );
        let mut call = binding.invoke(request);
        call.close();
        assert!(call.next().await.is_none());
        let mut call = operation.invoke(Request::binding("b"));
        call.close();
        assert!(call.next().await.is_none());
    }
}

struct SendProvider;
impl BindingInvoker for SendProvider {
    type Invocation<'a> = Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        InvokeOnly.supports_kind(kind)
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        ready(Ok(()))
    }
    fn preflight<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: CancellationToken,
    ) -> Option<Self::Invocation<'a>> {
        Some(ready(Ok(())))
    }
}

// no required preflight items for invoke-only providers; the default is
// still no prediction. Optional named futures support generic Send consumers.
#[tokio::test]
async fn generic_send_preflight_and_invoke_only_default() {
    let source = source();
    let value = BindingValue::default();
    let request = request(&source, &value);
    assert!(
        send_default(&Invoker::new(InvokeOnly), request)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        operation::Invoker::new(
            document("{}")
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            InvokeOnly
        )
        .preflight(Request::binding("b"))
        .await
        .unwrap()
        .is_none()
    );
    send_provider(
        &Invoker::new(SendProvider),
        &operation::Invoker::new(
            document("{}")
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            SendProvider,
        ),
        request,
    )
    .await;
}

struct Challenging;
impl BindingInvoker for Challenging {
    type Invocation<'a> = Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        InvokeOnly.supports_kind(kind)
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        ready(Ok(()))
    }
    fn preflight<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: CancellationToken,
    ) -> Option<Self::Invocation<'a>> {
        let challenge = ContextChallenge::new(
            JsonValue::parse(
                r#"{"target":"example.target","alternatives":[{"requirements":[{"type":"auth.bearer"}]}]}"#,
            )
            .unwrap(),
        )
        .unwrap();
        Some(ready(Err(InvocationError::challenge(challenge))))
    }
}

// Every preflight entry point runs the SDK's checks and then asks the
// provider's hook; none answers for a provider that has one.
#[tokio::test]
async fn preflight_returns_the_provider_hook_challenge_on_both_facades() {
    let source = source();
    let value = BindingValue::default();
    let binding = Invoker::new(Challenging);
    let operation = operation::Invoker::new(
        document("{}")
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant invocation document"),
        Challenging,
    );
    let control = CancellationToken::new();
    let challenges = [
        binding.preflight(request(&source, &value)).await,
        binding
            .preflight_with_control(request(&source, &value), &control)
            .await,
        operation.preflight(Request::binding("b")).await,
        operation
            .preflight_with_control(Request::operation("run"), &control)
            .await,
    ];
    for challenge in challenges {
        assert_eq!(challenge.unwrap().unwrap().target(), "example.target");
    }
}

struct LocalProvider(Rc<Cell<usize>>);
impl BindingInvoker for LocalProvider {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        InvokeOnly.supports_kind(kind)
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            self.0.set(self.0.get() + 1);
            Ok(())
        })
    }
    fn preflight<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: CancellationToken,
    ) -> Option<Self::Invocation<'a>> {
        Some(Box::pin(async move {
            self.0.set(self.0.get() + 1);
            Ok(())
        }))
    }
}

// Rc configuration is held across both local future types. the
// interface's forwarding implementations must preserve both hooks without boxing.
#[tokio::test]
async fn local_rc_preflight_and_forwarding_preserve_hooks() {
    let calls = Rc::new(Cell::new(0));
    let provider = Rc::new(LocalProvider(calls.clone()));
    let binding = Invoker::new(provider.clone());
    let operation = operation::Invoker::new(
        document("{}")
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant invocation document"),
        &provider,
    );
    let source = source();
    let value = BindingValue::default();
    assert!(
        binding
            .preflight(request(&source, &value))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        operation
            .preflight(Request::binding("b"))
            .await
            .unwrap()
            .is_none()
    );
    let mut call = binding.invoke(request(&source, &value));
    call.close();
    assert!(call.next().await.is_none());
    let mut call = operation.invoke(Request::binding("b"));
    call.close();
    assert!(call.next().await.is_none());
    assert_eq!(calls.get(), 4);
    let shared = Arc::new(SendProvider);
    send_provider(
        &Invoker::new(shared.clone()),
        &operation::Invoker::new(
            document("{}")
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            shared,
        ),
        request(&source, &value),
    )
    .await;
}

// codes are diagnostic facts; portable application data stays private.
// Display shows every code. Debug is structural: it shows the codes the
// invoker interfaces govern and redacts application-defined codes.
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

// Once an invocation settles, a provider emission reports that terminal
// error, not a cancellation it did not suffer.
#[tokio::test]
async fn emit_after_settlement_reports_the_terminal_error() {
    struct EmitTwice {
        values: [String; 2],
        refusals: std::sync::Mutex<Vec<String>>,
    }
    impl BindingInvoker for EmitTwice {
        type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
        fn supports_kind(&self, kind: &str) -> bool {
            InvokeOnly.supports_kind(kind)
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                for value in &self.values {
                    let error = io
                        .emit(JsonValue::parse(value.as_str()).unwrap())
                        .await
                        .unwrap_err();
                    self.refusals.lock().unwrap().push(error.code().to_owned());
                }
                Ok(())
            })
        }
    }
    let invalid = || "\"bad\"".to_owned();
    let oversized = || format!("\"{}\"", "x".repeat(300));
    let integer = r#"{"output":{"type":"integer"}}"#;
    // The first emission settles the terminal; the second must report it.
    for (operation, values, code) in [
        (
            integer,
            [invalid(), invalid()],
            "ERR_OPERATION_VALIDATION_FAILED",
        ),
        (
            integer,
            [invalid(), oversized()],
            "ERR_OPERATION_VALIDATION_FAILED",
        ),
        ("{}", [oversized(), "1".to_owned()], "ERR_EXECUTION_FAILED"),
    ] {
        let provider = EmitTwice {
            values,
            refusals: std::sync::Mutex::default(),
        };
        let invoker = operation::Invoker::new(
            document(operation)
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            &provider,
        )
        .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .with_limits(openbindings_invocation::InvocationLimits {
            max_value_bytes: 256,
            ..Default::default()
        })
        .unwrap();
        let mut call = invoker.invoke(Request::binding("b"));
        call.close();
        assert_eq!(call.next().await.unwrap().unwrap_err().code(), code);
        assert_eq!(*provider.refusals.lock().unwrap(), [code, code]);
    }
}

// One detail setting explains caller input and provider output alike, and
// an explanation locates the failure by the value's own member names.
#[tokio::test]
async fn explain_detail_covers_both_directions_and_names_value_members() {
    struct EmitMember;
    impl BindingInvoker for EmitMember {
        type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
        fn supports_kind(&self, kind: &str) -> bool {
            InvokeOnly.supports_kind(kind)
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                while io.next().await.is_some() {}
                io.emit(JsonValue::parse(r#"{"provider-member":7}"#).unwrap())
                    .await
            })
        }
    }
    let strings = r#"{"type":"object","additionalProperties":{"type":"string"}}"#;
    for (detail, explained) in [
        (openbindings::Detail::Outcome, false),
        (openbindings::Detail::Explain, true),
    ] {
        for (direction, operation, member) in [
            (
                operation::Direction::Input,
                format!(r#"{{"input":{strings}}}"#),
                "/caller-member",
            ),
            (
                operation::Direction::Output,
                format!(r#"{{"output":{strings}}}"#),
                "/provider-member",
            ),
        ] {
            let invoker = operation::Invoker::new(
                document(&operation)
                    .validate()
                    .unwrap()
                    .conformant()
                    .expect("conformant invocation document"),
                EmitMember,
            )
            .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
            .with_validation_detail(detail);
            let mut call = invoker.invoke(Request::binding("b"));
            call.write(JsonValue::parse(r#"{"caller-member":7}"#).unwrap())
                .await
                .unwrap();
            call.close();
            assert_eq!(
                call.next().await.unwrap().unwrap_err().code(),
                "ERR_OPERATION_VALIDATION_FAILED"
            );
            let failure = call.validation_failure().unwrap();
            assert_eq!(failure.direction(), direction);
            let openbindings::ValueOutcome::Fails { problems, .. } = failure.outcome() else {
                panic!("established failure expected")
            };
            let instances: Vec<_> = problems.iter().map(|p| p.instance.to_string()).collect();
            if explained {
                assert_eq!(instances, [member]);
            } else {
                assert!(instances.is_empty());
            }
        }
    }
}

// the invocation crate's private writer has the same exact byte-bound
// contract as inspection and synthesis; forwarding is exercised above.
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

struct Failure(&'static str);
impl BindingInvoker for Failure {
    type Invocation<'a> = Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        InvokeOnly.supports_kind(kind)
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        ready(Err(
            InvocationError::new(self.0, Some(JsonValue::null())).unwrap()
        ))
    }
    fn preflight<'a>(
        &'a self,
        _: BindingRequest<'a>,
        _: CancellationToken,
    ) -> Option<Self::Invocation<'a>> {
        Some(ready(Err(InvocationError::new(
            self.0,
            Some(JsonValue::null()),
        )
        .unwrap())))
    }
}

// filter only provider-origin errors, in each facade and provider hook.
#[tokio::test]
async fn reserved_provider_invocation_and_preflight_codes_are_rejected() {
    let source = source();
    let value = BindingValue::default();
    for code in [
        "ERR_OPERATION_NOT_FOUND",
        "ERR_BINDING_NOT_FOUND",
        "ERR_BINDING_SELECTION_REQUIRED",
        "ERR_UNKNOWN_SOURCE",
        "ERR_OPERATION_VALIDATION_FAILED",
        "ERR_SCHEMA_UNRESOLVED",
    ] {
        let binding = Invoker::new(Failure(code));
        let operation = operation::Invoker::new(
            document("{}")
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            Failure(code),
        );
        let mut calls = [binding.invoke(request(&source, &value))];
        let mut operation_call = operation.invoke(Request::binding("b"));
        calls[0].close();
        operation_call.close();
        let error = calls[0].next().await.unwrap().unwrap_err();
        assert_eq!(error.code(), "ERR_EXECUTION_FAILED");
        assert!(error.data().is_none());
        assert_eq!(
            calls[0].boundary_error().unwrap().code(),
            "reserved-provider-code"
        );
        let error = operation_call.next().await.unwrap().unwrap_err();
        assert_eq!(error.code(), "ERR_EXECUTION_FAILED");
        assert!(error.data().is_none());
        assert_eq!(
            operation_call.boundary_error().unwrap().code(),
            "reserved-provider-code"
        );
        assert!(operation_call.validation_failure().is_none());
        for error in [
            binding
                .preflight(request(&source, &value))
                .await
                .unwrap_err(),
            operation
                .preflight(Request::binding("b"))
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code(), "ERR_EXECUTION_FAILED");
            assert!(error.data().is_none());
        }
    }
    let binding = Invoker::new(Failure("example.application"));
    assert_eq!(
        binding
            .preflight(request(&source, &value))
            .await
            .unwrap_err()
            .code(),
        "example.application"
    );
    let mut call = binding.invoke(request(&source, &value));
    assert_eq!(
        call.next().await.unwrap().unwrap_err().code(),
        "example.application"
    );
    assert!(call.boundary_error().is_none());
}

// errors generated by SDK selection and validation keep their provenance
// and codes even though the provider-result path rejects those same spellings.
#[tokio::test]
async fn sdk_selection_and_validation_errors_still_surface() {
    let facade = operation::Invoker::new(
        document("{}")
            .validate()
            .unwrap()
            .conformant()
            .expect("conformant invocation document"),
        InvokeOnly,
    );
    for (request, code) in [
        (Request::operation("absent"), "ERR_OPERATION_NOT_FOUND"),
        (Request::binding("absent"), "ERR_BINDING_NOT_FOUND"),
    ] {
        let mut call = facade.invoke(request);
        assert_eq!(call.next().await.unwrap().unwrap_err().code(), code);
        assert_eq!(facade.preflight(request).await.unwrap_err().code(), code);
        assert!(call.boundary_error().is_none());
    }
    for (text, request, code) in [
        (
            document("{}")
                .value()
                .text()
                .replace(r#""source":"unsupported""#, r#""source":"s""#),
            Request::operation("run"),
            "ERR_BINDING_SELECTION_REQUIRED",
        ),
        (
            document("{}")
                .value()
                .text()
                .replace(r#""source":"s""#, r#""source":"missing""#),
            Request::binding("b"),
            "ERR_UNKNOWN_SOURCE",
        ),
    ] {
        let validation = Document::parse(text).unwrap().validate().unwrap();
        if code == "ERR_UNKNOWN_SOURCE" {
            assert_eq!(
                validation.report().evidence["OBI-07"],
                openbindings::Evidence::Violated
            );
            assert!(validation.conformant().is_none());
            continue;
        }
        let facade = operation::Invoker::new(validation.conformant().unwrap(), InvokeOnly);
        let mut call = facade.invoke(request);
        assert_eq!(call.next().await.unwrap().unwrap_err().code(), code);
        assert_eq!(facade.preflight(request).await.unwrap_err().code(), code);
        assert!(call.boundary_error().is_none());
    }
    struct Reads;
    impl BindingInvoker for Reads {
        type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
        fn supports_kind(&self, kind: &str) -> bool {
            InvokeOnly.supports_kind(kind)
        }
        fn invoke<'a>(
            &'a self,
            _: BindingRequest<'a>,
            mut io: ProviderIo,
            _: CancellationToken,
        ) -> Self::Invocation<'a> {
            Box::pin(async move {
                while io.next().await.is_some() {}
                Ok(())
            })
        }
    }
    for evaluated in [false, true] {
        let facade = operation::Invoker::new(
            document(r#"{"input":false}"#)
                .validate()
                .unwrap()
                .conformant()
                .expect("conformant invocation document"),
            Reads,
        );
        let facade = if evaluated {
            facade.with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        } else {
            facade
        };
        let mut call = facade.invoke(Request::binding("b"));
        let _ = call.write(JsonValue::null()).await;
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            if evaluated {
                "ERR_OPERATION_VALIDATION_FAILED"
            } else {
                "ERR_SCHEMA_UNRESOLVED"
            }
        );
        assert!(call.validation_failure().is_some());
        assert!(call.boundary_error().is_none());
    }
}

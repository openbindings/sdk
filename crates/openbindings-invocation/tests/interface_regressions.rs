use openbindings::{Document, JsonValue, ResourceSet, Source};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingRequest, BindingValue, ContextChallenge, InvocationError, ProviderIo,
    binding::{BindingInvoker, BindingPreflight, Invoker},
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

// all selection paths and both advisory paths use the specific code.
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
        assert_eq!(
            facade
                .preflight_with_provider(selection)
                .await
                .unwrap_err()
                .code(),
            "ERR_BINDING_NOT_FOUND"
        );
    }
}

fn send_default<'a, P: BindingInvoker + Sync>(
    facade: &'a Invoker<P>,
    request: BindingRequest<'a>,
) -> impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + Send + 'a {
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
    P: BindingPreflight + Sync,
    for<'b> P::Preflight<'b>: Send,
    for<'b> P::Invocation<'b>: Send,
{
    async move {
        assert!(
            binding
                .preflight_with_provider(request)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            operation
                .preflight_with_provider(Request::binding("b"))
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
}
impl BindingPreflight for SendProvider {
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn preflight<'a>(&'a self, _: BindingRequest<'a>, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Ok(None))
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
}
impl BindingPreflight for LocalProvider {
    type Preflight<'a> =
        Pin<Box<dyn Future<Output = Result<Option<ContextChallenge>, InvocationError>> + 'a>>;
    fn preflight<'a>(&'a self, _: BindingRequest<'a>, _: CancellationToken) -> Self::Preflight<'a> {
        Box::pin(async move {
            self.0.set(self.0.get() + 1);
            Ok(None)
        })
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
            .preflight_with_provider(request(&source, &value))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        operation
            .preflight_with_provider(Request::binding("b"))
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
#[test]
fn error_display_and_debug_show_code_and_redact_data() {
    let error = InvocationError::new(
        "ERR_OPERATION_VALIDATION_FAILED",
        Some(JsonValue::parse(r#""secret-data""#).unwrap()),
    )
    .unwrap();
    for text in [error.to_string(), format!("{error:?}")] {
        assert!(text.contains("ERR_OPERATION_VALIDATION_FAILED"));
        assert!(!text.contains("secret-data"));
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
}
impl BindingPreflight for Failure {
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn preflight<'a>(&'a self, _: BindingRequest<'a>, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Err(
            InvocationError::new(self.0, Some(JsonValue::null())).unwrap()
        ))
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
                .preflight_with_provider(request(&source, &value))
                .await
                .unwrap_err(),
            operation
                .preflight_with_provider(Request::binding("b"))
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
            .preflight_with_provider(request(&source, &value))
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

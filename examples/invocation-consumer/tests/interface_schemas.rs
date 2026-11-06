use invocation_consumer::Catalog;
use openbindings::{ConformantDocument, Document, ExternalSchemas, JsonValue};
use openbindings_invocation::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_source_pull::{PullLimits, PullRequest, SourcePullerExt};
use source_pull_consumer::{Embedded, Fixed};
use std::{
    future::{Future, Ready},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
fn j(s: &str) -> JsonValue {
    JsonValue::parse(s).unwrap()
}
fn doc(s: &str) -> ConformantDocument {
    Document::parse(s)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
async fn fixed() -> ConformantDocument {
    let request = PullRequest::new(
        doc(
            r#"{"openbindings":"0.2.0","operations":{},"sources":{"fixed":{"kind":"example.fixed@1"}}}"#,
        ),
        "fixed",
    );
    let result = (Fixed)
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &CancellationToken::new())
        .unwrap()
}
async fn exchange<F: Future<Output = Result<(), InvocationError>>>(
    call: Invocation<F>,
    values: Vec<JsonValue>,
) -> (Vec<JsonValue>, Result<(), InvocationError>) {
    let (mut input, mut output) = call.split();
    let send = async {
        for v in values {
            if input.send(v).await.is_err() {
                break;
            }
        }
        input.close();
    };
    let receive = async {
        let mut values = vec![];
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(v) => values.push(v),
                OutputEvent::Complete => {
                    assert!(output.next().await.is_none());
                    return (values, Ok(()));
                }
                OutputEvent::Error(e) => {
                    assert!(output.next().await.is_none());
                    return (values, Err(e));
                }
                OutputEvent::InputClosed => {}
                _ => panic!("unknown event"),
            }
        }
        panic!("missing terminal")
    };
    let (_, out) = tokio::join!(send, receive);
    out
}
struct Audited {
    catalog: Catalog,
    starts: Mutex<Vec<(String, Option<JsonValue>)>>,
    support: AtomicUsize,
}
impl Audited {
    fn new(catalog: Catalog) -> Self {
        Self {
            catalog,
            starts: Mutex::new(vec![]),
            support: AtomicUsize::new(0),
        }
    }
    fn record(&self, r: &BindingRequest) {
        self.starts.lock().unwrap().push((
            r.binding.clone(),
            r.context.as_ref().map(|c| c.value().value().to_owned()),
        ));
    }
}
impl InvocationSupport for Audited {
    fn can_invoke(&self, k: &str) -> bool {
        self.support.fetch_add(1, Ordering::Relaxed);
        self.catalog.can_invoke(k)
    }
}
impl BindingInvoker for Audited {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        r: BindingRequest,
        io: ProviderIo,
        c: CancellationToken,
    ) -> Self::Invoke<'a> {
        self.record(&r);
        self.catalog.handle_invoke(r, io, c)
    }
    fn handle_preflight<'a>(
        &'a self,
        r: BindingRequest,
        c: CancellationToken,
    ) -> Self::Preflight<'a> {
        self.record(&r);
        self.catalog.handle_preflight(r, c)
    }
}
#[tokio::test]
async fn output_mismatch_withholds_provider_value_and_retains_output_evidence() {
    let d = doc(
        r#"{"openbindings":"0.2.0","operations":{"echo":{"input":{"type":"string"},"output":{"type":"integer"}}},"sources":{"fixed":{"kind":"example.fixed@1"}},"bindings":{"b":{"source":"fixed","operation":"echo","data":{"item":"echo"}}}}"#,
    );
    let p = Arc::new(Catalog::default());
    let c = DefaultOperationInvoker::new(
        p.clone(),
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    let call = c
        .invoke(
            OperationRequest::operation(d, "echo"),
            CancellationToken::new(),
        )
        .unwrap();
    let state = call.control();
    let (values, result) = exchange(call, vec![j(r#""hello""#)]).await;
    assert!(values.is_empty());
    let e = result.unwrap_err();
    assert_eq!(e.code(), "ERR_OPERATION_VALIDATION_FAILED");
    assert!(e.data().is_none());
    assert_eq!(p.values(), 1);
    assert_eq!(
        state.validation_failure().unwrap().direction(),
        openbindings_invocation::operation::Direction::Output
    );
}
#[tokio::test]
async fn operation_forwards_exact_challenge_and_context_without_implicit_preflight() {
    let d = fixed().await;
    let p = Arc::new(Audited::new(Catalog::requiring_context()));
    let c = DefaultOperationInvoker::new(
        p.clone(),
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    let req = OperationRequest::operation(d, "echo");
    let challenge = match c
        .preflight(req.clone(), CancellationToken::new())
        .unwrap()
        .await
        .unwrap()
    {
        PreflightOutcome::Challenge(c) => c,
        _ => panic!("challenge"),
    };
    assert_eq!(p.catalog.values(), 0);
    let (out, err) = exchange(
        c.invoke(req.clone(), CancellationToken::new()).unwrap(),
        vec![j(r#""hello""#)],
    )
    .await;
    assert!(out.is_empty());
    let err = err.unwrap_err();
    assert_eq!(err.code(), "CONTEXT_REQUIRED");
    assert_eq!(err.data().unwrap().text(), challenge.value().value().text());
    assert_eq!(p.catalog.values(), 0);
    let exact = j(r#"{"permit":"approved","opaque":9007199254740993,"unicode":"\ud800"}"#);
    let ctx = Context::try_from(exact.clone()).unwrap();
    let (out, end) = exchange(
        c.invoke(req.with_context(ctx), CancellationToken::new())
            .unwrap(),
        vec![j(r#""hello""#)],
    )
    .await;
    end.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(p.catalog.predictions(), 1);
    let seen = p.starts.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[2].1.as_ref().unwrap().text(), exact.text());
}
#[tokio::test]
async fn selection_uses_exact_owned_codes_and_order_without_hidden_ranking() {
    let d = doc(
        r#"{"openbindings":"0.2.0","operations":{"echo":{"aliases":["say"]}},"sources":{"a":{"kind":"example.fixed@1"},"b":{"kind":"example.fixed@1"},"u":{"kind":"example.other@1"}},"bindings":{"a":{"source":"a","operation":"echo","preference":999,"data":{"item":"echo"}},"b":{"source":"b","operation":"echo","preference":-1,"data":{"item":"echo"}},"u":{"source":"u","operation":"echo","data":{"item":"echo"}}}}"#,
    );
    let p = Arc::new(Audited::new(Catalog::default()));
    let c = DefaultOperationInvoker::new(
        p.clone(),
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    for (r, code) in [
        (
            OperationRequest::operation(d.clone(), "missing"),
            "ERR_OPERATION_NOT_FOUND",
        ),
        (
            OperationRequest::binding(d.clone(), "missing"),
            "ERR_BINDING_NOT_FOUND",
        ),
        (
            OperationRequest::operation(d.clone(), "say"),
            "ERR_BINDING_SELECTION_REQUIRED",
        ),
        (
            OperationRequest::ordered(d.clone(), "say", vec!["a".into(), "missing".into()]),
            "ERR_BINDING_NOT_FOUND",
        ),
    ] {
        let (out, r) = exchange(
            c.invoke(r, CancellationToken::new()).unwrap(),
            vec![j("null")],
        )
        .await;
        assert!(out.is_empty());
        let e = r.unwrap_err();
        assert_eq!(e.code(), code);
        assert!(e.data().is_none());
    }
    assert!(p.starts.lock().unwrap().is_empty());
    let (out, end) = exchange(
        c.invoke(
            OperationRequest::ordered(d.clone(), "say", vec!["u".into(), "b".into(), "a".into()]),
            CancellationToken::new(),
        )
        .unwrap(),
        vec![j("9007199254740993")],
    )
    .await;
    end.unwrap();
    assert_eq!(out[0].text(), "9007199254740993");
    assert_eq!(p.starts.lock().unwrap()[0].0, "b");
    let before = p.support.load(Ordering::Relaxed);
    let (_, end) = exchange(
        c.invoke(OperationRequest::binding(d, "u"), CancellationToken::new())
            .unwrap(),
        vec![j("null")],
    )
    .await;
    assert_eq!(end.unwrap_err().code(), "example.unsupported-source");
    assert_eq!(p.support.load(Ordering::Relaxed), before);
}
#[tokio::test]
async fn embedded_kind_pull_adopt_and_binding_echo_keeps_exact_payload() {
    let original = doc(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"copy","operation":{"input":true,"output":true},"data":{"item":"copy","opaque":1e99999}}]}}}}"#,
    );
    let request = PullRequest::new(original.clone(), "catalog");
    let result = (Embedded)
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    let adopted = result
        .validate_for(&request, &PullLimits::default(), &CancellationToken::new())
        .unwrap();
    assert!(original.value().get("bindings").is_none());
    assert_eq!(
        adopted
            .binding("copy.catalog")
            .unwrap()
            .data()
            .unwrap()
            .get("opaque")
            .unwrap()
            .text(),
        "1e99999"
    );
    let c = Catalog::default();
    let value = j(r#"{"name":"\ud800","n":9007199254740993}"#);
    let (out, end) = exchange(
        c.invoke(
            BindingRequest::new(adopted, "copy.catalog"),
            CancellationToken::new(),
        )
        .unwrap(),
        vec![value.clone()],
    )
    .await;
    end.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text(), value.text());
}
struct Late;
impl BindingInvoker for Late {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = std::future::Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            if let Some(v) = io.next().await {
                io.send(v).await?;
            }
            Err(
                InvocationError::new("example.late", Some(j(r#"{"n":1e99999,"name":"\ud800"}"#)))
                    .unwrap(),
            )
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
impl InvocationSupport for Late {
    fn can_invoke(&self, _: &str) -> bool {
        true
    }
}
#[tokio::test]
async fn output_does_not_become_success_and_late_failure_payload_is_exact() {
    let d = fixed().await;
    let c = DefaultOperationInvoker::new(Late, DefaultEvaluator::new(), ExternalSchemas::default());
    let (out, end) = exchange(
        c.invoke(
            OperationRequest::operation(d, "echo"),
            CancellationToken::new(),
        )
        .unwrap(),
        vec![j(r#""hello""#)],
    )
    .await;
    assert_eq!(out.len(), 1);
    let e = end.unwrap_err();
    assert_eq!(e.code(), "example.late");
    assert_eq!(e.data().unwrap().text(), r#"{"n":1e99999,"name":"\ud800"}"#);
}
#[tokio::test]
async fn cancellation_before_dispatch_has_no_consumption_and_a_fused_terminal() {
    let d = fixed().await;
    let p = Arc::new(Catalog::default());
    let c = p.clone();
    let token = CancellationToken::new();
    let call = c
        .invoke(BindingRequest::new(d, "echo.fixed"), token.clone())
        .unwrap();
    token.cancel();
    let (out, end) = exchange(call, vec![j("null")]).await;
    assert!(out.is_empty());
    assert_eq!(end.unwrap_err().code(), "ERR_CANCELLED");
    assert_eq!(p.values(), 0);
}

#[tokio::test]
async fn unresolved_input_is_distinct_from_mismatch_and_decisive_branch_can_proceed() {
    for (schema, success) in [
        (r#"{"$ref":"https://unavailable.test/schema"}"#, false),
        (
            r#"{"anyOf":[true,{"$ref":"https://unavailable.test/schema"}]}"#,
            true,
        ),
    ] {
        let d = doc(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"echo":{{"input":{schema}}}}},"sources":{{"fixed":{{"kind":"example.fixed@1"}}}},"bindings":{{"b":{{"source":"fixed","operation":"echo","data":{{"item":"echo"}}}}}}}}"#
        ));
        let p = Arc::new(Catalog::default());
        let c = DefaultOperationInvoker::new(
            p.clone(),
            DefaultEvaluator::new(),
            ExternalSchemas::default(),
        );
        let call = c
            .invoke(
                OperationRequest::operation(d, "echo"),
                CancellationToken::new(),
            )
            .unwrap();
        let control = call.control();
        let (out, end) = exchange(call, vec![j("null")]).await;
        if success {
            end.unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].text(), "null");
            assert_eq!(p.values(), 1);
            assert!(control.validation_failure().is_none());
        } else {
            let e = end.unwrap_err();
            assert_eq!(e.code(), "ERR_SCHEMA_UNRESOLVED");
            assert!(e.data().is_none());
            assert!(out.is_empty());
            assert_eq!(p.values(), 0);
            assert_eq!(
                control.validation_failure().unwrap().direction(),
                openbindings_invocation::operation::Direction::Input
            );
        }
    }
}

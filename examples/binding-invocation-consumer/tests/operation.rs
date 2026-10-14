//! External-consumer checks: literal catalog expectations, public APIs only.
use binding_invocation_consumer::{Catalog, KIND, yield_once};
use openbindings::{
    Direction, JsonValue, NoVerdict, NoVerdictReason, ParsedDocument, PreparedSchema, ResourceSet,
    SchemaEvaluator, SchemaRequest, Source, ValueOutcome, ValueProblem,
};
use openbindings_interface_synthesis::{
    CoverageStatus, SynthesisRequest, SynthesisSource, Synthesizer,
};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingRequest, InvocationError, ProviderIo,
    binding::BindingInvoker,
    operation::{Attempt, Invoker, Request, ValidationStage},
};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn send<T: Send>(_: &T) {}
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn source(content: &str) -> Source {
    Source {
        kind: KIND.into(),
        content: Some(json(content)),
        ..Default::default()
    }
}
fn document(operation: &str, content: &str) -> openbindings::ValidatedDocument {
    ParsedDocument::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"run":{operation}}},"sources":{{"catalog":{{"kind":"{KIND}","content":{content}}}}},"bindings":{{"chosen":{{"operation":"run","source":"catalog","content":"manual"}}}}}}"#)).unwrap().validate().unwrap().validated().unwrap()
}

// Deliberately independent of the default evaluator. Unsupported schemas must
// stay distinguishable from false schemas, and preparation must remain lazy.
#[derive(Default)]
struct Booleans {
    preparations: AtomicUsize,
}
struct BooleanContract(bool);
impl SchemaEvaluator for Booleans {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn openbindings::WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let proof: &openbindings::ValidatedDocument = request.document();
        assert_eq!(proof.openbindings(), "0.2.0");
        self.preparations.fetch_add(1, Ordering::Relaxed);
        request
            .entry()
            .view()
            .as_bool()
            .map(|value| Arc::new(BooleanContract(value)) as Arc<dyn PreparedSchema>)
            .ok_or_else(|| {
                NoVerdict::new(
                    NoVerdictReason::UnsupportedCapability,
                    "consumer.boolean-only",
                    "only boolean schemas are supported",
                )
            })
    }
}
impl PreparedSchema for BooleanContract {
    fn validate(&self, _: &JsonValue, control: &dyn openbindings::WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        if self.0 {
            ValueOutcome::Satisfies
        } else {
            ValueOutcome::Fails {
                message: "value does not satisfy the contract".into(),
                problems: vec![ValueProblem {
                    details: None,
                    instance_pointer: String::new(),
                    schema_location: None,
                    code: "consumer.false".into(),
                    message: "false schema rejects the value".into(),
                }],
                problems_complete: true,
            }
        }
    }
}

#[tokio::test]
async fn inspect_synthesize_and_invoke_operation_preserves_exact_values() {
    let source = source(r#"{"units":[{"id":"空.🚲"},{"id":"other"}],"exact":9007199254740993}"#);
    let provider = Catalog::default();
    let inventory = openbindings_source_inspection::Inspector::new(&provider)
        .inspect(&source)
        .await
        .unwrap();
    assert_eq!(
        inventory
            .targets
            .iter()
            .map(|t| t.source_ref.as_str())
            .collect::<Vec<_>>(),
        ["空.🚲", "other"]
    );
    let sources = [SynthesisSource {
        name: Some("catalog"),
        source: &source,
    }];
    let synthesized = Synthesizer::new(&provider)
        .synthesize(&SynthesisRequest {
            sources: &sources,
            ..Default::default()
        })
        .await
        .unwrap();
    let evaluator = Arc::new(Booleans::default());
    let invoker = Invoker::new(synthesized.clone(), &provider)
        .with_evaluator(evaluator.clone(), ResourceSet::default());
    assert!(
        invoker
            .preflight(Request::operation("s0.u0"))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(evaluator.preparations.load(Ordering::Relaxed), 0);
    assert_eq!(provider.attempts.load(Ordering::Relaxed), 0);
    // The retained document owns its source; these authoring inputs may end now.
    drop(synthesized);
    drop(source);
    let expected = [
        "9007199254740993",
        "0.29000000000000001",
        "1e400",
        r#"{"nested":[-0,{"unicode":"空.🚲"}]}"#,
    ];
    let prepared = invoker.prepare("s0.u0").unwrap();
    let mut retained = Vec::new();
    for _ in 0..2 {
        let mut call = prepared.invoke(Attempt::new());
        send(&call);
        for value in expected {
            call.write(json(value)).await.unwrap();
        }
        call.close();
        for value in expected {
            let next = call.next();
            send(&next);
            let result = next.await.unwrap().unwrap();
            assert_eq!(result.text(), value);
            retained.push(result);
        }
        assert!(call.next().await.is_none());
        assert!(call.outcome().unwrap().is_ok());
        assert!(call.validation_failure().is_none());
    }
    assert_eq!(evaluator.preparations.load(Ordering::Relaxed), 1);
    assert_eq!(provider.effects.load(Ordering::Relaxed), 8);
    assert_eq!(
        provider.last_target.lock().unwrap().as_deref(),
        Some("空.🚲")
    );
    drop(prepared);
    drop(invoker);
    drop(provider);
    assert_eq!(retained[0].text(), "9007199254740993");
}

#[tokio::test]
async fn manual_alias_ordered_selection_and_invalid_suffix_are_explicit() {
    let doc = ParsedDocument::parse(format!(r#"{{"openbindings":"0.2.0",
        "operations":{{"run":{{"aliases":["go"]}},"other":{{}}}},
        "sources":{{"catalog":{{"kind":"{KIND}","content":{{"units":[{{"id":"first"}},{{"id":"second"}}]}}}},"unsupported":{{"kind":"urn:unsupported"}}}},
        "bindings":{{"first":{{"operation":"run","source":"catalog","content":"first"}},"second":{{"operation":"run","source":"catalog","content":"second"}},"skip":{{"operation":"run","source":"unsupported"}},"foreign":{{"operation":"other","source":"catalog","content":"first"}}}}}}"#)).unwrap();
    let provider = Catalog::default();
    let invoker = Invoker::new(doc.validate().unwrap().validated().unwrap(), &provider);
    assert_eq!(
        invoker
            .preflight(Request::operation("go"))
            .await
            .unwrap_err()
            .code(),
        "ERR_BINDING_SELECTION_REQUIRED"
    );
    for (request, code) in [
        (Request::operation("go"), "ERR_BINDING_SELECTION_REQUIRED"),
        (
            Request::ordered("go", &["second", "foreign"]),
            "ERR_BINDING_NOT_FOUND",
        ),
        (
            Request::ordered("go", &["second", "missing"]),
            "ERR_BINDING_NOT_FOUND",
        ),
        (Request::ordered("go", &[]), "ERR_REFUSED"),
        // RO-5: an unsupported explicit binding matches ordered selection.
        (Request::binding("skip"), "ERR_BINDING_NOT_FOUND"),
    ] {
        let mut call = invoker.invoke(request);
        call.close();
        assert_eq!(call.next().await.unwrap().unwrap_err().code(), code);
        assert!(call.next().await.is_none());
    }
    assert_eq!(provider.attempts.load(Ordering::Relaxed), 0);
    let mut call = invoker.invoke(Request::ordered("go", &["skip", "second", "first"]));
    call.write(json("9007199254740993")).await.unwrap();
    call.close();
    assert_eq!(
        call.next().await.unwrap().unwrap().text(),
        "9007199254740993"
    );
    assert!(call.next().await.is_none());
    assert_eq!(
        provider.last_target.lock().unwrap().as_deref(),
        Some("second")
    );
    let mut exact = invoker.invoke(Request::binding("first"));
    exact.write(JsonValue::null()).await.unwrap();
    exact.close();
    assert_eq!(exact.next().await.unwrap().unwrap().text(), "null");
    assert!(exact.next().await.is_none());
    assert_eq!(
        provider.last_target.lock().unwrap().as_deref(),
        Some("first")
    );
}

#[tokio::test]
async fn schema_rejection_withholds_values_and_retains_only_local_evidence() {
    for (operation, direction, stage, code, effects) in [
        (
            r#"{"input":false}"#,
            Direction::Input,
            ValidationStage::Evaluation,
            "ERR_OPERATION_VALIDATION_FAILED",
            0,
        ),
        (
            r#"{"output":false}"#,
            Direction::Output,
            ValidationStage::Evaluation,
            "ERR_OPERATION_VALIDATION_FAILED",
            1,
        ),
        (
            r#"{"input":{"type":"null"}}"#,
            Direction::Input,
            ValidationStage::Preparation,
            "ERR_SCHEMA_UNRESOLVED",
            0,
        ),
    ] {
        let provider = Catalog::default();
        let invoker = Invoker::new(
            document(operation, r#"{"units":[{"id":"manual"}]}"#),
            &provider,
        )
        .with_evaluator(Arc::new(Booleans::default()), ResourceSet::default());
        let mut call = invoker.invoke(Request::operation("run"));
        let control = call.control();
        assert!(call.validation_failure().is_none());
        call.write(JsonValue::null()).await.unwrap();
        call.close();
        let error = call.next().await.unwrap().unwrap_err();
        assert_eq!(error.code(), code);
        assert!(error.data().is_none());
        let evidence = call.validation_failure().unwrap();
        assert_eq!(evidence.direction(), direction);
        assert_eq!(evidence.stage(), stage);
        match evidence.outcome() {
            ValueOutcome::Fails {
                problems,
                problems_complete,
                ..
            } => {
                assert!(*problems_complete);
                assert_eq!(problems[0].code, "consumer.false");
            }
            ValueOutcome::NoVerdict { .. } => assert_eq!(stage, ValidationStage::Preparation),
            ValueOutcome::Satisfies => panic!("a successful value is not a rejection"),
        }
        let (_, mut outputs) = call.split();
        assert!(Arc::ptr_eq(
            &evidence,
            &outputs.validation_failure().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &evidence,
            &control.validation_failure().unwrap()
        ));
        assert!(outputs.next().await.is_none());
        assert_eq!(provider.effects.load(Ordering::Relaxed), effects);
        drop(outputs);
        drop(control);
        drop(invoker);
        drop(provider);
        assert_eq!(evidence.direction(), direction);
    }
}

#[tokio::test]
async fn declared_contract_needs_evaluator_but_absence_forwards_and_empty_is_lazy() {
    for operation in [r#"{"input":true}"#, r#"{"input":false}"#] {
        let provider = Catalog::default();
        let invoker = Invoker::new(
            document(operation, r#"{"units":[{"id":"manual"}]}"#),
            &provider,
        );
        assert!(
            invoker
                .preflight(Request::operation("run"))
                .await
                .unwrap()
                .is_none()
        );
        let mut empty = invoker.invoke(Request::operation("run"));
        empty.close();
        assert!(empty.next().await.is_none());
        assert!(empty.outcome().unwrap().is_ok());
        let mut call = invoker.invoke(Request::operation("run"));
        call.write(JsonValue::null()).await.unwrap();
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            "ERR_SCHEMA_UNRESOLVED"
        );
        assert_eq!(
            call.validation_failure().unwrap().stage(),
            ValidationStage::Preparation
        );
        assert_eq!(provider.effects.load(Ordering::Relaxed), 0);
    }
    let provider = Catalog::default();
    let invoker = Invoker::new(document("{}", r#"{"units":[{"id":"manual"}]}"#), &provider);
    let mut call = invoker.invoke(Request::operation("run"));
    call.write(json("1e400")).await.unwrap();
    call.close();
    assert_eq!(call.next().await.unwrap().unwrap().text(), "1e400");
    assert!(call.next().await.is_none());
    assert!(call.validation_failure().is_none());
}

#[tokio::test]
async fn invalid_synthesized_units_and_malformed_manual_catalog_never_effect() {
    let provider = Catalog::default();
    let invalid = source(r#"{"units":[{"id":"bad","status":"invalid"},{"id":"valid"}]}"#);
    let sources = [SynthesisSource {
        name: Some("catalog"),
        source: &invalid,
    }];
    let synthesized = Synthesizer::new(&provider)
        .synthesize_with_coverage(&SynthesisRequest {
            sources: &sources,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(synthesized.coverage().entries()[0].source_ref, "bad");
    assert_eq!(
        synthesized.coverage().entries()[0].status,
        CoverageStatus::Invalid
    );
    let invoker = Invoker::new(synthesized.document().clone(), &provider)
        .with_evaluator(Arc::new(Booleans::default()), ResourceSet::default());
    for request in [Request::operation("s0.u0"), Request::binding("s0.u0")] {
        let mut call = invoker.invoke(request);
        assert!(call.write(JsonValue::null()).await.is_err());
        call.close();
        assert!(call.next().await.unwrap().is_err());
    }
    assert_eq!(provider.attempts.load(Ordering::Relaxed), 0);
    for content in [
        r#"{"units":[{"id":"manual"},{"id":"manual"}]}"#,
        r#"{"units":[{"id":"manual"},{"id":"irrelevant","interaction":"invalid"}]}"#,
        r#"{"units":[{"id":"manual","status":"invalid"}]}"#,
        r#"{"units":[{"id":"manual"}],"unknownRemainder":"false"}"#,
    ] {
        let invoker = Invoker::new(document("{}", content), &provider);
        let mut call = invoker.invoke(Request::binding("chosen"));
        assert!(call.write(JsonValue::null()).await.is_err());
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            "ERR_REFUSED"
        );
        assert!(call.next().await.is_none());
        assert_eq!(provider.effects.load(Ordering::Relaxed), 0);
    }
}

struct Borrowed<'a> {
    calls: &'a AtomicUsize,
    context: &'a str,
}
impl BindingInvoker for Borrowed<'_> {
    type Invocation<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn invoke<'a>(
        &'a self,
        request: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            yield_once().await;
            assert_eq!(request.context.unwrap().text(), self.context);
            self.calls.fetch_add(1, Ordering::Relaxed);
            io.close_input();
            Ok(())
        })
    }
}
struct Local(Rc<AtomicUsize>);
impl BindingInvoker for Local {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            let retained = self.0.clone();
            yield_once().await;
            retained.fetch_add(1, Ordering::Relaxed);
            io.close_input();
            Ok(())
        })
    }
}

#[tokio::test]
async fn borrowed_provider_context_and_local_rc_survive_real_pending() {
    let doc = document("{}", r#"{"units":[{"id":"manual"}]}"#);
    let calls = AtomicUsize::new(0);
    let context = json(r#"{"number":9007199254740993,"opaque":{"nested":[1e400,-0]}}"#);
    let provider = Borrowed {
        calls: &calls,
        context: context.text(),
    };
    let invoker = Invoker::new(doc.clone(), &provider);
    let mut call = invoker.invoke(Request::binding("chosen").with_context(&context));
    send(&call);
    let next = call.next();
    send(&next);
    assert!(next.await.is_none());
    assert!(call.outcome().unwrap().is_ok());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let local_calls = Rc::new(AtomicUsize::new(0));
    let local = Invoker::new(doc, Rc::new(Local(local_calls.clone())));
    let mut local_call = local.invoke(Request::operation("run"));
    assert!(local_call.next().await.is_none());
    assert!(local_call.outcome().unwrap().is_ok());
    assert_eq!(local_calls.load(Ordering::Relaxed), 1);
}

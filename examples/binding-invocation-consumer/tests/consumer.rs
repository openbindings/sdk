use binding_invocation_consumer::{Catalog, KIND, yield_once};
use openbindings::{JsonValue, Source};
use openbindings_interface_synthesis::{SynthesisRequest, SynthesisSource, Synthesizer};
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingInvocationInput, BindingRequest, BindingValue, InvocationError, ProviderIo,
    binding::{BindingInvoker, Invoker},
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
fn source(text: &str) -> Source {
    Source {
        kind: KIND.into(),
        content: Some(JsonValue::parse(text).unwrap()),
        ..Default::default()
    }
}

#[tokio::test]
async fn inspect_synthesize_choose_explicitly_and_invoke_preserved_values() {
    // Literal target and output expectations do not derive from emitted coverage.
    let source = source(r#"{"units":[{"id":"空.🚲"},{"id":"other"}],"exact":9007199254740993}"#);
    let provider = Arc::new(Catalog::default());
    let inspection = openbindings_source_inspection::Inspector::new(&*provider)
        .inspect(&source)
        .await
        .unwrap();
    assert_eq!(
        inspection
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
    let synthesized = Synthesizer::new(&*provider)
        .synthesize(&SynthesisRequest {
            sources: &sources,
            ..Default::default()
        })
        .await
        .unwrap();
    let authored = synthesized.clone();
    // The caller explicitly chooses this known binding; invocation does no lookup.
    let chosen = authored.binding("s0.u0").unwrap();
    assert_eq!(chosen.content().unwrap().as_str(), Some("空.🚲"));
    let carrier = BindingValue::from_json_ref(chosen.value()).unwrap();
    let selected_source = &invocation_input(&authored, chosen.key()).source;
    assert_eq!(
        selected_source.content.as_ref().unwrap().text(),
        source.content.as_ref().unwrap().text()
    );
    let facade = Invoker::new(provider.clone());
    let mut retained = None;
    for _ in 0..3 {
        let mut call = facade.invoke(BindingRequest {
            source: selected_source,
            binding: &carrier,
            context: None,
        });
        send(&call);
        call.write(JsonValue::parse("9007199254740993").unwrap())
            .await
            .unwrap();
        call.close();
        retained = Some(call.next().await.unwrap().unwrap());
        assert!(call.next().await.is_none());
        assert!(call.outcome().unwrap().is_ok());
    }
    assert_eq!(provider.attempts.load(Ordering::Relaxed), 3);
    assert_eq!(provider.effects.load(Ordering::Relaxed), 3);
    assert_eq!(
        provider.last_target.lock().unwrap().as_deref(),
        Some("空.🚲")
    );
    drop(authored);
    drop(synthesized);
    drop(source);
    drop(facade);
    drop(provider);
    assert_eq!(retained.unwrap().text(), "9007199254740993");
}

#[tokio::test]
async fn handauthored_default_echo_is_bidi_and_new_modes_are_explicit() {
    let request = BindingInvocationInput::from_json(&JsonValue::parse(format!(r#"{{"source":{{"kind":"{KIND}","content":{{"units":[{{"id":"manual"}}]}}}},"binding":{{"content":"manual"}},"context":{{}}}}"#)).unwrap()).unwrap();
    let provider = Catalog::default();
    let facade = Invoker::new(&provider);
    let call = facade.invoke(request.request());
    assert_eq!(provider.attempts.load(Ordering::Relaxed), 0);
    let (mut input, mut output) = call.split();
    let write = async {
        for i in 0..25 {
            input
                .write(JsonValue::parse(i.to_string()).unwrap())
                .await
                .unwrap();
        }
        input.close();
    };
    let read = async {
        let mut count = 0;
        while let Some(value) = output.next().await {
            assert_eq!(value.unwrap().text(), count.to_string());
            count += 1;
        }
        assert_eq!(count, 25);
        assert!(output.outcome().unwrap().is_ok());
    };
    tokio::join!(write, read);
    assert_eq!(provider.effects.load(Ordering::Relaxed), 25);
    for (mode, expected) in [("unary", 1), ("server", 7), ("count", 1), ("empty", 0)] {
        let source = source(&format!(
            r#"{{"units":[{{"id":"target","interaction":"{mode}"}}]}}"#
        ));
        let binding = BindingValue {
            content: Some(JsonValue::string("target").unwrap()),
            ..Default::default()
        };
        let mut call = facade.invoke(BindingRequest {
            source: &source,
            binding: &binding,
            context: None,
        });
        if mode != "empty" {
            call.write(JsonValue::parse("null").unwrap()).await.unwrap();
        }
        call.close();
        let mut count = 0;
        while let Some(value) = call.next().await {
            value.unwrap();
            count += 1;
        }
        assert_eq!(count, expected);
        assert!(call.outcome().unwrap().is_ok());
    }
}

struct Borrowed<'a>(&'a AtomicUsize);
impl BindingInvoker for Borrowed<'_> {
    type Invocation<'a>
        = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
    where
        Self: 'a;
    fn supports_kind(&self, k: &str) -> bool {
        k == "test.borrowed"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            yield_once().await;
            self.0.fetch_add(1, Ordering::Relaxed);
            io.close_input();
            Ok(())
        })
    }
}
struct Local(Rc<AtomicUsize>);
impl BindingInvoker for Local {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == "test.borrowed"
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
async fn borrowed_nonclone_send_and_local_rc_real_await() {
    let calls = AtomicUsize::new(0);
    let source = Source {
        kind: "test.borrowed".into(),
        ..Default::default()
    };
    let binding = BindingValue::default();
    let request = BindingRequest {
        source: &source,
        binding: &binding,
        context: None,
    };
    let provider = Borrowed(&calls);
    let facade = Invoker::new(&provider);
    let mut invocation = facade.invoke(request);
    send(&invocation);
    let next = invocation.next();
    send(&next);
    assert!(next.await.is_none());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(facade.preflight(request).await.unwrap().is_none());
    let local_calls = Rc::new(AtomicUsize::new(0));
    let local = Invoker::new(Rc::new(Local(local_calls.clone())));
    let mut call = local.invoke(request);
    assert!(call.next().await.is_none());
    assert!(call.outcome().unwrap().is_ok());
    assert_eq!(local_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn synthesized_colliding_ids_execute_the_exact_scope() {
    let source = source(
        r#"{"units":[{"id":"same","scope":"dependency"},{"id":"same","scope":"projection","status":"lossy","interaction":"count"},{"id":"same","scope":"alternative","interaction":"server"},{"id":"same","interaction":"unary"}]}"#,
    );
    let provider = Catalog::default();
    let inspection = openbindings_source_inspection::Inspector::new(&provider)
        .inspect(&source)
        .await
        .unwrap();
    assert_eq!(inspection.targets.len(), 1);
    assert_eq!(inspection.targets[0].source_ref, "same");
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
    let authored = synthesized.clone();
    let bindings: std::collections::BTreeMap<_, _> = authored
        .bindings()
        .unwrap()
        .into_iter()
        .map(|v| (v.key().to_owned(), v))
        .collect();
    assert!(!bindings.contains_key("s0.u0"));
    assert_eq!(bindings["s0.u3"].content().unwrap().as_str(), Some("same"));
    for (key, scope, input, values) in [
        ("s0.u1", "projection", "7", vec!["1"]),
        ("s0.u2", "alternative", "9", vec!["9"; 7]),
        ("s0.u3", "target", "7", vec!["7"]),
    ] {
        let chosen = &bindings[key];
        if scope != "target" {
            let content = chosen.content().unwrap();
            assert_eq!(content.get("scope").unwrap().as_str(), Some(scope));
            assert_eq!(content.get("id").unwrap().as_str(), Some("same"));
        }
        let binding = BindingValue::from_json_ref(chosen.value()).unwrap();
        let source = &invocation_input(&authored, chosen.key()).source;
        let facade = Invoker::new(&provider);
        let mut call = facade.invoke(BindingRequest {
            source,
            binding: &binding,
            context: None,
        });
        let before = provider.effects.load(Ordering::Relaxed);
        call.write(JsonValue::parse(input).unwrap()).await.unwrap();
        call.close();
        let mut actual = Vec::new();
        while let Some(value) = call.next().await {
            actual.push(value.unwrap().text().to_owned());
        }
        assert_eq!(actual, values);
        assert!(call.outcome().unwrap().is_ok());
        assert_eq!(provider.effects.load(Ordering::Relaxed), before + 1);
        assert_eq!(
            provider.last_target.lock().unwrap().as_deref(),
            Some("same")
        );
        assert_eq!(provider.last_scope.lock().unwrap().as_deref(), Some(scope));
    }
}

#[tokio::test]
async fn dependency_first_cannot_shadow_a_lossy_target_or_impose_operation_schema() {
    let source =
        source(r#"{"units":[{"id":"same","scope":"dependency"},{"id":"same","status":"lossy"}]}"#);
    let provider = Catalog::default();
    let inspection = openbindings_source_inspection::Inspector::new(&provider)
        .inspect(&source)
        .await
        .unwrap();
    assert_eq!(inspection.targets.len(), 1);
    assert_eq!(inspection.targets[0].source_ref, "same");
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
    let authored = synthesized.clone();
    assert_eq!(
        authored
            .resolve_operation("s0.u1")
            .unwrap()
            .input()
            .unwrap()
            .text(),
        r#"{"type":"null"}"#
    );
    let chosen = authored.binding("s0.u1").unwrap();
    let binding = BindingValue::from_json_ref(chosen.value()).unwrap();
    let source = &invocation_input(&authored, chosen.key()).source;
    let facade = Invoker::new(&provider);
    let mut call = facade.invoke(BindingRequest {
        source,
        binding: &binding,
        context: None,
    });
    // By-value invocation does not receive or enforce the generated operation schema.
    call.write(JsonValue::parse("9007199254740993").unwrap())
        .await
        .unwrap();
    call.close();
    assert_eq!(
        call.next().await.unwrap().unwrap().text(),
        "9007199254740993"
    );
    assert!(call.next().await.is_none());
    assert!(call.outcome().unwrap().is_ok());
    assert_eq!(provider.effects.load(Ordering::Relaxed), 1);
    assert_eq!(
        provider.last_scope.lock().unwrap().as_deref(),
        Some("target")
    );
}

#[tokio::test]
async fn malformed_kind_fields_and_duplicate_targets_refuse_without_effects() {
    for text in [
        r#"{"units":[{"id":"u","status":false}]}"#,
        r#"{"units":[{"id":"u","scope":false}]}"#,
        r#"{"units":[{"id":"u","interaction":false}]}"#,
        r#"{"units":[{"id":"u","interaction":42}]}"#,
        r#"{"units":[{"id":"u","interaction":null}]}"#,
        r#"{"units":[{"id":"u","interaction":[]}]}"#,
        r#"{"units":[{"id":"u","interaction":{}}]}"#,
        r#"{"units":[{"id":"u","interaction":""}]}"#,
        r#"{"units":[{"id":"u","status":"unknown"}]}"#,
        r#"{"units":[{"id":"u","scope":"unknown"}]}"#,
        r#"{"units":[{"id":"u","interaction":"unknown"}]}"#,
        r#"{"units":[{"id":"u"},{"id":"u","scope":"target"}]}"#,
        r#"{"units":[{"id":"u"},{"id":"other","status":null}]}"#,
        r#"{"units":[{"id":"u"},{"id":"other","interaction":42}]}"#,
        r#"{"units":[{"id":"u"},{"id":"other","scope":"dependency","interaction":"unknown"}]}"#,
        r#"{"units":[{"id":"u"},{"id":""}]}"#,
        r#"{"units":[{"id":"u"}],"unknownRemainder":"yes"}"#,
    ] {
        let source = source(text);
        let binding = BindingValue {
            content: Some(JsonValue::string("u").unwrap()),
            ..Default::default()
        };
        let provider = Catalog::default();
        let inspection = openbindings_source_inspection::Inspector::new(&provider)
            .inspect(&source)
            .await
            .unwrap_err();
        assert_eq!(
            inspection.kind(),
            openbindings_source_inspection::InspectionErrorKind::UnreadableSource,
            "{text}"
        );
        let sources = [SynthesisSource {
            name: Some("catalog"),
            source: &source,
        }];
        let request = SynthesisRequest {
            sources: &sources,
            ..Default::default()
        };
        let synthesizer = Synthesizer::new(&provider);
        for error in [
            synthesizer.synthesize(&request).await.unwrap_err(),
            synthesizer
                .synthesize_with_coverage(&request)
                .await
                .unwrap_err(),
        ] {
            assert_eq!(
                error.kind(),
                openbindings_interface_synthesis::SynthesisErrorKind::UnreadableSource,
                "{text}"
            );
            assert!(error.partial().is_none());
        }
        let facade = Invoker::new(&provider);
        let mut call = facade.invoke(BindingRequest {
            source: &source,
            binding: &binding,
            context: None,
        });
        let _ = call.write(JsonValue::parse("7").unwrap()).await;
        call.close();
        assert_eq!(
            call.next().await.unwrap().unwrap_err().code(),
            "ERR_REFUSED"
        );
        assert!(call.next().await.is_none());
        assert_eq!(provider.effects.load(Ordering::Relaxed), 0);
        assert!(provider.last_target.lock().unwrap().is_none());
    }
}

#[tokio::test]
async fn every_admitted_interaction_can_be_inspected_synthesized_and_invoked() {
    // Literal mode/output expectations are independent of both source readers.
    for mode in [
        None,
        Some("echo"),
        Some("unary"),
        Some("server"),
        Some("count"),
        Some("empty"),
    ] {
        for scope in ["target", "alternative", "projection"] {
            for status in ["represented", "lossy"] {
                let interaction =
                    mode.map_or(String::new(), |m| format!(r#", "interaction":"{m}""#));
                let source = source(&format!(
                    r#"{{"units":[{{"id":"unit","scope":"{scope}","status":"{status}"{interaction}}}]}}"#
                ));
                let provider = Catalog::default();
                let inventory = openbindings_source_inspection::Inspector::new(&provider)
                    .inspect(&source)
                    .await
                    .unwrap();
                assert!(inventory.exhaustive);
                assert_eq!(inventory.targets.len(), usize::from(scope == "target"));
                let sources = [SynthesisSource {
                    name: Some("catalog"),
                    source: &source,
                }];
                let request = SynthesisRequest {
                    sources: &sources,
                    ..Default::default()
                };
                let result = Synthesizer::new(&provider)
                    .synthesize_with_coverage(&request)
                    .await
                    .unwrap();
                assert!(result.coverage().strict_eligible());
                assert_eq!(
                    result.coverage().fully_represented(),
                    status == "represented"
                );
                let authored = result.document().clone();
                let chosen = authored.binding("s0.u0").unwrap();
                if let Some(target) = inventory.targets.first() {
                    assert_eq!(target.source_ref, "unit");
                    assert_eq!(
                        target.binding.content.as_ref().unwrap().text(),
                        chosen.content().unwrap().text()
                    );
                }
                let binding = BindingValue::from_json_ref(chosen.value()).unwrap();
                let emitted_source = &invocation_input(&authored, chosen.key()).source;
                let facade = Invoker::new(&provider);
                let (mut input, mut output) = facade
                    .invoke(BindingRequest {
                        source: emitted_source,
                        binding: &binding,
                        context: None,
                    })
                    .split();
                let value = if status == "lossy" {
                    "null"
                } else {
                    "9007199254740993"
                };
                let count = match mode {
                    Some("empty") => 0,
                    Some("unary" | "server") => 1,
                    _ => 3,
                };
                let send = async {
                    for _ in 0..count {
                        input.write(JsonValue::parse(value).unwrap()).await.unwrap();
                    }
                    input.close();
                };
                let receive = async {
                    let mut values = Vec::new();
                    while let Some(value) = output.next().await {
                        values.push(value.unwrap().text().to_owned());
                    }
                    assert!(output.outcome().unwrap().is_ok());
                    values
                };
                let ((), values) = tokio::join!(send, receive);
                let expected = match mode {
                    Some("empty") => vec![],
                    Some("count") => vec!["3"],
                    Some("server") => vec![value; 7],
                    _ => vec![value; count],
                };
                assert_eq!(
                    values, expected,
                    "mode={mode:?}, scope={scope}, status={status}"
                );
                assert_eq!(provider.effects.load(Ordering::Relaxed), count);
                assert_eq!(provider.last_scope.lock().unwrap().as_deref(), Some(scope));
            }
        }
    }
}

// Materialize the role's by-value request through its existing exact carrier API.
fn invocation_input(
    document: &openbindings::Conformant<openbindings::Document>,
    key: &str,
) -> BindingInvocationInput {
    let binding = document.binding(key).unwrap();
    let source = document.source(binding.source()).unwrap();
    let value = JsonValue::parse(format!(
        r#"{{"source":{},"binding":{}}}"#,
        source.value().text(),
        binding.value().text()
    ))
    .unwrap();
    BindingInvocationInput::from_json(&value).unwrap()
}

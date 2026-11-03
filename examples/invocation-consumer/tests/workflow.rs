use invocation_consumer::Catalog;
use openbindings::{ConformantDocument, Document, Edit, ExternalSchemas, JsonValue};
use openbindings_invocation::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_source_pull::{PullClient, PullLimits, PullRequest};
use source_pull_consumer::Fixed;
use std::{future::Future, sync::Arc};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
async fn authored() -> ConformantDocument {
    let original = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"sources":{"fixed":{"kind":"example.fixed@1"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
    let request = PullRequest::new(original.clone(), "fixed");
    let result = PullClient::new(Fixed)
        .pull_source(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(original.value().get("bindings").is_none());
    // This caller deliberately chooses all additions. Pulling never adopts them.
    result
        .validate_for(&request, &PullLimits::default(), &CancellationToken::new())
        .unwrap()
}

async fn exchange<F: Future<Output = Result<(), InvocationError>>>(
    call: Invocation<F>,
    values: Vec<JsonValue>,
) -> (
    Vec<JsonValue>,
    Result<(), InvocationError>,
    Result<(), WriteError>,
) {
    let (mut input, mut output) = call.split();
    let sending = async {
        for value in values {
            input.send(value).await?;
        }
        input.close();
        Ok(())
    };
    let receiving = async {
        let mut values = Vec::new();
        while let Some(event) = output.next().await {
            match event {
                OutputEvent::Output(value) => values.push(value),
                OutputEvent::Complete => return (values, Ok(())),
                OutputEvent::Error(error) => return (values, Err(error)),
                _ => {}
            }
        }
        (values, Err(InvocationError::transport_closed()))
    };
    let (sent, (values, terminal)) = tokio::join!(sending, receiving);
    (values, terminal, sent)
}

#[tokio::test]
async fn pull_adopt_then_explicit_binding_echo_preserves_exact_values_and_queue_progress() {
    let original = authored().await;
    let provider = Arc::new(Catalog::default());
    let client = BindingClient::new(provider.clone());
    let mut values = vec![
        json("9007199254740993"),
        json("1e99999"),
        json("null"),
        json(r#"{"id":"空.🚲"}"#),
    ];
    values.extend((0..64).map(|n| json(&n.to_string())));
    let expected = values
        .iter()
        .map(|v| v.text().to_owned())
        .collect::<Vec<_>>();
    let call = client
        .invoke(
            BindingRequest::new(original, "echo.fixed"),
            CancellationToken::new(),
        )
        .unwrap();
    fn send<T: Send>(_: &T) {}
    send(&call);
    let (output, terminal, sent) = exchange(call, values).await;
    terminal.unwrap();
    sent.unwrap();
    assert_eq!(
        output.iter().map(|v| v.text()).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(provider.values(), 68);
}

#[tokio::test]
async fn operation_alias_and_contracts_use_the_adopted_original_document() {
    let original = authored().await;
    let document = original
        .document()
        .edit(&[Edit::set(
            &["operations", "echo", "aliases"],
            json(r#"["say"]"#),
        )])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let provider = Arc::new(Catalog::default());
    let client = OperationClient::new(
        provider.clone(),
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    let call = client
        .invoke(
            OperationRequest::operation(document.clone(), "say"),
            CancellationToken::new(),
        )
        .unwrap();
    let (values, terminal, sent) = exchange(call, vec![json(r#""hello""#)]).await;
    terminal.unwrap();
    sent.unwrap();
    assert_eq!(values[0].view().as_str(), Some("hello"));
    let consumed = provider.values();
    let call = client
        .invoke(
            OperationRequest::binding(document, "echo.fixed"),
            CancellationToken::new(),
        )
        .unwrap();
    let (values, terminal, _) = exchange(call, vec![json("42")]).await;
    assert!(values.is_empty());
    assert!(terminal.is_err());
    assert_eq!(
        provider.values(),
        consumed,
        "invalid input reached the realization"
    );
}

#[tokio::test]
async fn preflight_and_context_require_a_separate_explicit_attempt() {
    let document = authored().await;
    let provider = Arc::new(Catalog::requiring_context());
    let client = BindingClient::new(provider.clone());
    let request = BindingRequest::new(document, "echo.fixed");
    let prediction = client
        .preflight(request.clone(), CancellationToken::new())
        .unwrap()
        .await
        .unwrap();
    assert!(matches!(prediction, PreflightOutcome::Challenge(_)));
    assert_eq!(provider.predictions(), 1);
    assert_eq!(provider.values(), 0);
    let call = client
        .invoke(request.clone(), CancellationToken::new())
        .unwrap();
    let (values, terminal, _) = exchange(call, vec![json(r#""hello""#)]).await;
    assert!(values.is_empty());
    assert_eq!(terminal.unwrap_err().code(), "CONTEXT_REQUIRED");
    assert_eq!(provider.values(), 0);
    let context =
        Context::try_from(json(r#"{"permit":"approved","opaque":9007199254740993}"#)).unwrap();
    let call = client
        .invoke(request.with_context(context), CancellationToken::new())
        .unwrap();
    let (_, terminal, sent) = exchange(call, vec![json(r#""hello""#)]).await;
    terminal.unwrap();
    sent.unwrap();
    assert_eq!(provider.values(), 1);
    assert_eq!(
        provider.predictions(),
        1,
        "invocation must not add a preflight call"
    );
}

#[tokio::test]
async fn invalid_selection_and_missing_binding_produce_no_operation_effects() {
    let original = authored().await;
    let provider = Arc::new(Catalog::default());
    let client = OperationClient::new(
        provider.clone(),
        DefaultEvaluator::new(),
        ExternalSchemas::default(),
    );
    let requests = [
        OperationRequest::binding(original.clone(), "missing"),
        OperationRequest::ordered(
            original,
            "echo",
            vec!["echo.fixed".into(), "missing".into()],
        ),
    ];
    for request in requests {
        let call = client.invoke(request, CancellationToken::new()).unwrap();
        let (values, terminal, _) = exchange(call, vec![json(r#""hello""#)]).await;
        assert!(values.is_empty());
        assert!(terminal.is_err());
    }
    assert_eq!(provider.values(), 0);
}

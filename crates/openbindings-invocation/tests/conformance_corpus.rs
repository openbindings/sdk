//! All current pinned selection cases through lazy/prepared invocation and preflight.
use openbindings::{Document, ExternalSchemas, JsonValue};
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, DefaultOperationInvoker, InvocationError,
    InvocationSupport, OperationRequest, OutputEvent, PreflightOutcome, ProviderIo,
    operation::BindingChoice,
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::Value;
use std::{
    collections::HashSet,
    future::{Future, Ready, ready},
    pin::Pin,
};
const SELECTION: [(&str, &str); 4] = [
    (
        "automatic-resolution",
        include_str!("fixtures/conformance/selection/automatic-resolution.json"),
    ),
    (
        "default-supported",
        include_str!("fixtures/conformance/selection/default-supported.json"),
    ),
    (
        "explicit-binding",
        include_str!("fixtures/conformance/selection/explicit-binding.json"),
    ),
    (
        "override-selection",
        include_str!("fixtures/conformance/selection/override-selection.json"),
    ),
];

struct Corpus {
    supported: HashSet<String>,
}
impl InvocationSupport for Corpus {
    fn can_invoke(&self, kind: &str) -> bool {
        self.supported.contains(kind)
    }
}
impl BindingInvoker for Corpus {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move { io.send(JsonValue::string(&request.binding).unwrap()).await })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction))
    }
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
async fn outcome<F: Future<Output = Result<(), InvocationError>>>(
    call: openbindings_invocation::Invocation<F>,
) -> Result<String, String> {
    let (mut input, mut output) = call.split();
    input.close();
    let result = match output.next().await.unwrap() {
        OutputEvent::Output(value) => Ok(value.view().as_str().unwrap().to_owned()),
        OutputEvent::Error(error) => Err(error.code().to_owned()),
        other => panic!("{other:?}"),
    };
    if result.is_ok() {
        assert!(matches!(output.next().await, Some(OutputEvent::Complete)));
    }
    assert!(output.next().await.is_none());
    result
}
#[tokio::test]
async fn published_selection_cases_resolve_through_invocation_preparation_and_preflight() {
    let mut total = 0;
    for (cluster, text) in SELECTION {
        let fixture: Value = serde_json::from_str(text).unwrap();
        assert_eq!(fixture["cluster"], cluster);
        for case in fixture["tests"].as_array().unwrap() {
            total += 1;
            let description = case["description"].as_str().unwrap();
            let interface = Document::parse(case["document"].to_string())
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .unwrap();
            let provider = Corpus {
                supported: strings(&case["supported"]).into_iter().collect(),
            };
            let client = DefaultOperationInvoker::new(
                &provider,
                DefaultEvaluator::default(),
                ExternalSchemas::default(),
            );
            let name = case["operation"].as_str().unwrap();
            let (request, choice) = if let Some(key) = case.get("binding").and_then(Value::as_str) {
                (
                    OperationRequest::binding(interface.clone(), key),
                    BindingChoice::Binding(key.into()),
                )
            } else if let Some(keys) = case.get("selection") {
                let keys = strings(keys);
                (
                    OperationRequest::ordered(interface.clone(), name, keys.clone()),
                    BindingChoice::Ordered(keys),
                )
            } else {
                (
                    OperationRequest::operation(interface.clone(), name),
                    BindingChoice::Sole,
                )
            };
            let expected = &case["expected"];
            let want = match expected.get("binding") {
                Some(key) => Ok(key.as_str().unwrap().to_owned()),
                None => Err(match expected["kind"].as_str().unwrap() {
                    "ambiguous" => "ERR_BINDING_SELECTION_REQUIRED",
                    "unknown-binding" | "no-candidate" => "ERR_BINDING_NOT_FOUND",
                    kind => panic!("{kind}"),
                }
                .to_owned()),
            };
            assert_eq!(
                outcome(
                    client
                        .invoke(request.clone(), CancellationToken::new())
                        .unwrap()
                )
                .await,
                want,
                "{cluster}: {description}"
            );
            let prepared = client
                .prepare(interface, name, &CancellationToken::new())
                .unwrap();
            assert_eq!(
                outcome(
                    prepared
                        .invoke(&provider, choice, None, CancellationToken::new())
                        .unwrap()
                )
                .await,
                want,
                "{cluster} prepared: {description}"
            );
            let prediction = client
                .preflight(request, CancellationToken::new())
                .unwrap()
                .await
                .map(|p| assert!(matches!(p, PreflightOutcome::NoPrediction)))
                .map_err(|e| e.code().to_owned());
            assert_eq!(
                prediction,
                want.map(drop),
                "{cluster} preflight: {description}"
            );
        }
    }
    assert_eq!(total, 20);
}

//! Prepared schemas outlive their compiler and enter caller-owned spawned tasks.
use openbindings::{Document, ExternalSchemas, JsonValue, SchemaValidator, ValueOutcome};
use openbindings_invocation::ContextChallenge;
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, DefaultOperationInvoker, InvocationError,
    InvocationSupport, OutputEvent, ProviderIo,
    operation::{BindingChoice, PreparedOperation},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    sync::Arc,
};
struct Echo;
impl InvocationSupport for Echo {
    fn can_invoke(&self, kind: &str) -> bool {
        kind == "example.echo"
    }
}
impl BindingInvoker for Echo {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        _: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        ready(Ok(None))
    }
}
fn shareable<T: Send + Sync + 'static>(_: &T) {}
fn spawn_attempts<P, V>(
    provider: Arc<P>,
    prepared: &PreparedOperation<V>,
    values: &[&str],
) -> Vec<tokio::task::JoinHandle<Result<String, InvocationError>>>
where
    P: BindingInvoker + InvocationSupport + Send + Sync + 'static,
    for<'a> P::Invoke<'a>: Send,
    V: SchemaValidator + Clone + Send + 'static,
{
    values
        .iter()
        .map(|value| {
            let provider = provider.clone();
            let prepared = prepared.clone();
            let value = JsonValue::parse(*value).unwrap();
            tokio::spawn(async move {
                let (mut input, mut output) = prepared
                    .invoke(
                        &*provider,
                        BindingChoice::Sole,
                        None,
                        CancellationToken::new(),
                    )
                    .unwrap()
                    .split();
                input.send(value).await.unwrap();
                input.close();
                let mut returned = None;
                while let Some(event) = output.next().await {
                    match event {
                        OutputEvent::Output(value) => {
                            assert!(returned.is_none(), "exactly one output per attempt");
                            returned = Some(value.text().to_owned());
                        }
                        OutputEvent::Complete => return Ok(returned.expect("one output")),
                        OutputEvent::Error(error) => return Err(error),
                        OutputEvent::InputClosed => (),
                        _ => panic!("unknown event"),
                    }
                }
                panic!("terminal required")
            })
        })
        .collect()
}
#[tokio::test]
async fn prepared_operation_moves_into_spawned_tasks_and_outlives_its_invoker() {
    let document=Document::parse(r#"{"openbindings":"0.2.0","operations":{"echo":{"input":{"type":"integer"},"output":{"type":"integer"}}},"sources":{"s":{"kind":"example.echo"}},"bindings":{"b":{"operation":"echo","source":"s"}}}"#).unwrap().validate().unwrap().conformant().unwrap();
    let client = DefaultOperationInvoker::new(
        Echo,
        DefaultEvaluator::default(),
        ExternalSchemas::default(),
    );
    let prepared = client
        .prepare(document, "echo", &CancellationToken::new())
        .unwrap();
    shareable(&prepared);
    drop(client);
    assert!(matches!(
        prepared.input().unwrap().evaluate(&JsonValue::integer(1)),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepared.output().unwrap().evaluate(&JsonValue::null()),
        ValueOutcome::Fails { .. }
    ));
    let provider = Arc::new(Echo);
    let mut outputs = Vec::new();
    for task in spawn_attempts(provider.clone(), &prepared, &["1", "9007199254740993", "3"]) {
        outputs.push(task.await.unwrap().unwrap());
    }
    assert_eq!(outputs, ["1", "9007199254740993", "3"]);
    let rejected = spawn_attempts(provider, &prepared, &[r#""text""#])
        .pop()
        .unwrap();
    assert_eq!(
        rejected.await.unwrap().unwrap_err().code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
}

#[test]
fn prepared_schema_accessors_preserve_absent_directions() {
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"absent":{},"input":{"input":false},"output":{"output":true}}}"#)
        .unwrap().validate().unwrap().conformant().unwrap();
    let client =
        DefaultOperationInvoker::new(Echo, DefaultEvaluator::new(), ExternalSchemas::default());
    for name in ["absent", "input", "output"] {
        let prepared = client
            .prepare(document.clone(), name, &CancellationToken::new())
            .unwrap();
        assert_eq!(prepared.input().is_some(), name == "input");
        assert_eq!(prepared.output().is_some(), name == "output");
        if let Some(input) = prepared.input() {
            assert!(matches!(
                input.evaluate(&JsonValue::null()),
                ValueOutcome::Fails { .. }
            ));
        }
        if let Some(output) = prepared.output() {
            assert!(matches!(
                output.evaluate(&JsonValue::null()),
                ValueOutcome::Satisfies
            ));
        }
    }
}

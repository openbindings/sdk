//! A long-running service prepares an operation once and shares the handle
//! with spawned tasks, so attempts reuse its compiled contracts.
use openbindings::{Document, JsonValue, ResourceSet};
use openbindings_invocation::{
    BindingRequest, CancellationToken, InvocationError, ProviderIo,
    binding::BindingInvoker,
    operation::{Attempt, Invoker, PreparedOperation},
};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::{future::Future, pin::Pin, sync::Arc};

struct Echo;
impl BindingInvoker for Echo {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "example.echo"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.emit(value).await?;
            }
            Ok(())
        })
    }
}

fn shareable<T: Send + Sync + 'static>(_: &T) {}

// The generic shape a service uses: no leaked or static invoker is needed.
fn spawn_attempts<P>(
    prepared: Arc<PreparedOperation<P>>,
    values: &[&str],
) -> Vec<tokio::task::JoinHandle<Result<String, InvocationError>>>
where
    P: BindingInvoker + Send + Sync + 'static,
    for<'a> P::Invocation<'a>: Send,
{
    values
        .iter()
        .map(|value| {
            let prepared = prepared.clone();
            let value = JsonValue::parse(*value).unwrap();
            tokio::spawn(async move {
                let mut call = prepared.invoke(Attempt::new());
                call.write(value).await.expect("input accepted");
                call.close();
                let output = call.next().await.expect("one output")?;
                assert!(call.next().await.is_none());
                Ok(output.text().to_owned())
            })
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn prepared_operation_moves_into_spawned_tasks_and_outlives_its_invoker() {
    let document = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"echo":{"input":{"type":"integer"},"output":{"type":"integer"}}},"sources":{"s":{"kind":"example.echo"}},"bindings":{"b":{"operation":"echo","source":"s"}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .expect("conformant invocation document");
    let invoker = Arc::new(
        Invoker::new(document, Echo)
            .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default()),
    );
    let prepared = Arc::new(invoker.prepare("echo").unwrap());
    shareable(&*prepared);
    shareable(&*invoker);
    drop(invoker);
    let mut outputs = Vec::new();
    for task in spawn_attempts(prepared.clone(), &["1", "9007199254740993", "3"]) {
        outputs.push(task.await.unwrap().unwrap());
    }
    assert_eq!(outputs, ["1", "9007199254740993", "3"]);
    // The shared contracts still gate every attempt after the invoker is gone.
    let rejected = spawn_attempts(prepared, &["\"text\""]).pop().unwrap();
    assert_eq!(
        rejected.await.unwrap().unwrap_err().code(),
        "ERR_OPERATION_VALIDATION_FAILED"
    );
}

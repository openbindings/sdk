//! Host-owned cleanup survives cancellation, even with transport-owned input.
use openbindings::JsonValue;
use openbindings_invocation::{
    BindingInvoker, BindingInvokerExt, BindingRequest, CancellationToken, ContextChallenge,
    InvocationError, OutputEvent, ProviderIo,
};
use std::{
    future::{Future, Ready, pending, ready},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Host {
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    closed: AtomicBool,
    input_dropped: AtomicBool,
}

// The guard stays in the provider future, even if the transport owns its input.
// A real connection here supplies a WebSocket close, HTTP/2 reset or MCP notice.
struct CloseOnDrop {
    runtime: tokio::runtime::Handle,
    host: Arc<Host>,
}
impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        let host = self.host.clone();
        let task = self.runtime.spawn(async move {
            // Stand-in for asynchronous protocol cleanup. The host owns its
            // executor lifetime and any cleanup deadline/buffering policy.
            tokio::task::yield_now().await;
            host.closed.store(true, Ordering::SeqCst);
        });
        self.host.tasks.lock().unwrap().push(task);
    }
}

struct Subscription {
    runtime: tokio::runtime::Handle,
    host: Arc<Host>,
}
impl BindingInvoker for Subscription {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        _: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            let _close = CloseOnDrop {
                runtime: self.runtime.clone(),
                host: self.host.clone(),
            };
            let (mut input, mut output) = io.split();
            let host = self.host.clone();
            let task = self.runtime.spawn(async move {
                // The supplied hook token exists for independently polled work.
                // Dropping the provider alone cannot drop input after this move.
                loop {
                    tokio::select! {
                        _ = control.cancelled() => break,
                        value = input.next() => if value.is_none() { break; },
                    }
                }
                drop(input);
                host.input_dropped.store(true, Ordering::SeqCst);
            });
            self.host.tasks.lock().unwrap().push(task);
            for n in 0..3 {
                output.send(JsonValue::integer(n)).await?;
            }
            pending().await
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

#[tokio::test]
async fn cancel_drains_committed_outputs_and_closes_detached_transport() {
    let host = Arc::new(Host::default());
    let adapter = Subscription {
        runtime: tokio::runtime::Handle::current(),
        host: host.clone(),
    };
    let interface = openbindings::Document::parse(
        r#"{
        "openbindings":"0.2.0","operations":{"watch":{}},
        "sources":{"s":{"kind":"example.subscription@1"}},
        "bindings":{"b":{"source":"s","operation":"watch"}}
    }"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let cancellation = CancellationToken::new();
    let (input, mut output) = adapter
        .invoke(BindingRequest::new(interface, "b"), cancellation.clone())
        .unwrap()
        .split();
    assert!(matches!(output.next().await, Some(OutputEvent::Output(_))));
    cancellation.cancel();
    let mut count = 1;
    let mut cancelled = false;
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(_) => count += 1,
            OutputEvent::Error(error) => {
                assert_eq!(error.code(), "ERR_CANCELLED");
                cancelled = true;
            }
            OutputEvent::Complete => panic!("cancellation is not completion"),
            _ => (),
        }
    }
    assert_eq!(count, 3);
    assert!(cancelled);
    // The caller has deliberately kept input open throughout cancellation.
    let tasks = std::mem::take(&mut *host.tasks.lock().unwrap());
    for task in tasks {
        task.await.unwrap();
    }
    assert!(host.input_dropped.load(Ordering::SeqCst));
    assert!(host.closed.load(Ordering::SeqCst));
    drop(input);
}

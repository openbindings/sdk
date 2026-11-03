use openbindings::{Document, JsonValue};
use openbindings_source_pull::*;
use source_pull_consumer::{CatalogError, Embedded};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context as TaskContext, Poll, Waker},
};

fn request() -> PullRequest {
    PullRequest::new(Document::parse(r#"{"openbindings":"0.2.0","operations":{},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{},"data":{"item":"a","exact":9007199254740993,"id":"空.🚲","present":null}}]}}}}"#).unwrap().validate().unwrap().conformant().unwrap(), "catalog")
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut TaskContext::from_waker(Waker::noop()))
}
struct Borrowed<'a>(&'a AtomicUsize);
impl SourcePuller for Borrowed<'_> {
    type Error = CatalogError;
    type Pull<'a>
        = Pin<Box<dyn Future<Output = Result<PullResult, PullFailure<Self::Error>>> + Send + 'a>>
    where
        Self: 'a;
    fn pull_source(&self, request: PullRequest, control: CancellationToken) -> Self::Pull<'_> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            Embedded.pull_source(request, control).await
        })
    }
}

#[test]
fn borrowed_provider_suspends_and_retained_result_outlives_every_originating_owner() {
    let calls = AtomicUsize::new(0);
    let result = {
        let provider = Borrowed(&calls);
        let client = PullClient::new(&provider);
        let mut future = Box::pin(client.pull_source(request(), CancellationToken::new()));
        fn send<T: Send>(_: &T) {}
        send(&future);
        assert!(poll(future.as_mut()).is_pending());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        let Poll::Ready(Ok(result)) = poll(future.as_mut()) else {
            panic!("finite local observation")
        };
        result
    };
    let binding = result
        .additions()
        .bindings()
        .unwrap()
        .value()
        .get("a.catalog")
        .unwrap()
        .to_owned();
    drop(result);
    let data = binding.get("data").unwrap();
    assert_eq!(
        data.get("exact").unwrap().number_text(),
        Some("9007199254740993")
    );
    assert_eq!(data.get("id").unwrap().as_str(), Some("空.🚲"));
    assert_eq!(data.get("present").unwrap().text(), "null");
    assert!(data.get("absent").is_none());
}

#[test]
fn shared_provider_and_local_native_errors_need_no_shared_legacy_interface_types() {
    let provider = Arc::new(Embedded);
    let client = PullClient::new(provider.clone());
    let mut future = Box::pin(client.pull_source(request(), CancellationToken::new()));
    assert!(poll(future.as_mut()).is_pending());
    assert!(matches!(poll(future.as_mut()), Poll::Ready(Ok(_))));
    struct Local(Rc<JsonValue>);
    impl SourcePuller for Local {
        type Error = Rc<JsonValue>;
        type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;
        fn pull_source(&self, _: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
            ready(Err(PullFailure::Operational(self.0.clone())))
        }
    }
    let native = Rc::new(JsonValue::parse(r#"{"native":9007199254740993}"#).unwrap());
    let client = PullClient::new(Rc::new(Local(native.clone())));
    let mut future = Box::pin(client.pull_source(request(), CancellationToken::new()));
    let Poll::Ready(Err(ClientError::Provider(PullFailure::Operational(returned)))) =
        poll(future.as_mut())
    else {
        panic!("native error")
    };
    assert!(Rc::ptr_eq(&native, &returned));
}

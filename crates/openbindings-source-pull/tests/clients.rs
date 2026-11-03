use openbindings::{Document, JsonValue};
use openbindings_source_pull::*;
use std::{
    cell::{Cell, RefCell},
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll, Waker},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn request() -> PullRequest {
    PullRequest::new(
        Document::parse(
            r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#,
        )
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap(),
        "s",
    )
}
fn result() -> PullResult {
    PullResult::from_json(json(
        r#"{"additions":{},"report":{"exhaustive":true,"added":[]}}"#,
    ))
    .unwrap()
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut TaskContext::from_waker(Waker::noop()))
}
fn complete<F: Future>(future: F) -> F::Output {
    match poll(Box::pin(future).as_mut()) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready"),
    }
}

enum Reply {
    Result,
    Challenge,
    Operational(Rc<str>),
    Cancelled,
}
struct Provider {
    calls: Cell<usize>,
    seen: RefCell<Option<PullRequest>>,
    child: RefCell<Option<CancellationToken>>,
    reply: Reply,
}
impl Provider {
    fn new(reply: Reply) -> Self {
        Self {
            calls: Cell::new(0),
            seen: RefCell::new(None),
            child: RefCell::new(None),
            reply,
        }
    }
}
impl SourcePuller for Provider {
    type Error = Rc<str>;
    type Pull<'a> = Ready<Result<PullResult, PullFailure<Self::Error>>>;
    fn pull_source(&self, request: PullRequest, control: CancellationToken) -> Self::Pull<'_> {
        self.calls.set(self.calls.get() + 1);
        *self.seen.borrow_mut() = Some(request);
        *self.child.borrow_mut() = Some(control);
        ready(match &self.reply {
            Reply::Result => Ok(result()),
            Reply::Challenge => Err(PullFailure::ContextRequired(ContextChallenge::try_from(json(r#"{"message":"supply context","schema":{"type":"object","x-exact":9007199254740993}}"#)).unwrap())),
            Reply::Operational(error) => Err(PullFailure::Operational(error.clone())),
            Reply::Cancelled => Err(PullFailure::Cancelled),
        })
    }
}

#[test]
fn dispatch_is_lazy_and_preconditions_do_not_construct_provider_work() {
    let provider = Provider::new(Reply::Result);
    let client = PullClient::new(&provider);
    let pending = client.pull_source(request(), CancellationToken::new());
    assert_eq!(provider.calls.get(), 0);
    drop(pending);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        complete(client.pull_source(request(), cancelled)),
        Err(ClientError::Cancelled)
    ));
    let missing = PullRequest::new(request().interface().clone(), "missing");
    assert!(matches!(
        complete(client.pull_source(missing, CancellationToken::new())),
        Err(ClientError::Validation(PullValidationError::MissingSource))
    ));
    let client = PullClient::with_limits(
        &provider,
        PullLimits {
            max_request_bytes: 0,
            ..PullLimits::default()
        },
    );
    assert!(matches!(
        complete(client.pull_source(request(), CancellationToken::new())),
        Err(ClientError::Validation(PullValidationError::LimitExceeded(
            "request-bytes"
        )))
    ));
    assert_eq!(provider.calls.get(), 0);
}

#[test]
fn original_owner_context_and_native_local_errors_are_preserved_without_retry() {
    let provider = Rc::new(Provider::new(Reply::Result));
    let client = PullClient::new(provider.clone());
    let context = Context::try_from(json(
        r#"{"token":9007199254740993,"opaque":"\ud800","same":1,"same":2}"#,
    ))
    .unwrap();
    let supplied = request().with_context(context.clone());
    let original = supplied.interface().as_bytes().to_vec();
    let external = CancellationToken::new();
    complete(client.pull_source(supplied, external.clone())).unwrap();
    let seen = provider.seen.borrow();
    let seen = seen.as_ref().unwrap();
    assert_eq!(seen.interface().as_bytes(), original);
    assert_eq!(
        seen.context().unwrap().value().value().text(),
        context.value().value().text()
    );
    assert!(seen.interface().value().get("context").is_none());
    assert!(provider.child.borrow().as_ref().unwrap().is_cancelled());
    assert!(!external.is_cancelled());
    assert_eq!(provider.calls.get(), 1);
    let native: Rc<str> = Rc::from("native transport detail");
    let provider = Provider::new(Reply::Operational(native.clone()));
    let client = PullClient::with_limits(
        &provider,
        PullLimits {
            max_response_bytes: 0,
            ..PullLimits::default()
        },
    );
    let error = complete(client.pull_source(request(), CancellationToken::new())).unwrap_err();
    let ClientError::Provider(PullFailure::Operational(returned)) = error else {
        panic!("native failure")
    };
    assert!(Rc::ptr_eq(&native, &returned));
    assert_eq!(provider.calls.get(), 1);
}

#[test]
fn challenges_are_exact_bounded_and_never_automatically_satisfied() {
    let provider = Provider::new(Reply::Challenge);
    let client = PullClient::new(&provider);
    let error = complete(client.pull_source(request(), CancellationToken::new())).unwrap_err();
    let ClientError::Provider(PullFailure::ContextRequired(challenge)) = error else {
        panic!("challenge")
    };
    assert_eq!(
        challenge
            .schema()
            .value()
            .get("x-exact")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    assert_eq!(provider.calls.get(), 1);
    let client = PullClient::with_limits(
        &provider,
        PullLimits {
            max_response_bytes: 0,
            ..PullLimits::default()
        },
    );
    assert!(matches!(
        complete(client.pull_source(request(), CancellationToken::new())),
        Err(ClientError::Validation(PullValidationError::LimitExceeded(
            "response-bytes"
        )))
    ));
    assert_eq!(provider.calls.get(), 2);
    let provider = Provider::new(Reply::Cancelled);
    assert!(matches!(
        complete(PullClient::new(provider).pull_source(request(), CancellationToken::new())),
        Err(ClientError::Cancelled)
    ));
}

struct PendingProvider {
    child: RefCell<Option<CancellationToken>>,
    dropped_cancelled: Cell<bool>,
    external: Option<CancellationToken>,
    ready: bool,
}
struct Pending<'a>(&'a PendingProvider);
impl Future for Pending<'_> {
    type Output = Result<PullResult, PullFailure<()>>;
    fn poll(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<Self::Output> {
        if self.0.ready {
            Poll::Ready(Ok(result()))
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0
            .dropped_cancelled
            .set(self.0.child.borrow().as_ref().unwrap().is_cancelled());
        if let Some(external) = &self.0.external {
            external.cancel();
        }
    }
}
impl SourcePuller for PendingProvider {
    type Error = ();
    type Pull<'a> = Pending<'a>;
    fn pull_source(&self, _: PullRequest, control: CancellationToken) -> Self::Pull<'_> {
        *self.child.borrow_mut() = Some(control);
        Pending(self)
    }
}
#[test]
fn pending_drop_and_ready_destructor_cancellation_preserve_cleanup_order() {
    for ready in [false, true] {
        let external = CancellationToken::new();
        let provider = PendingProvider {
            child: RefCell::new(None),
            dropped_cancelled: Cell::new(false),
            external: ready.then(|| external.clone()),
            ready,
        };
        let client = PullClient::new(&provider);
        let mut work = Box::pin(client.pull_source(request(), external.clone()));
        if ready {
            assert!(matches!(
                poll(work.as_mut()),
                Poll::Ready(Err(ClientError::Cancelled))
            ));
        } else {
            assert!(poll(work.as_mut()).is_pending());
        }
        drop(work);
        assert!(provider.dropped_cancelled.get());
        assert_eq!(external.is_cancelled(), ready);
    }
}

#[test]
fn send_futures_can_borrow_non_static_providers_in_each_ownership_form() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Borrowed<'env>(&'env AtomicUsize);
    impl SourcePuller for Borrowed<'_> {
        type Error = ();
        type Pull<'a>
            =
            Pin<Box<dyn Future<Output = Result<PullResult, PullFailure<Self::Error>>> + Send + 'a>>
        where
            Self: 'a;
        fn pull_source(&self, _: PullRequest, _: CancellationToken) -> Self::Pull<'_> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(result())
            })
        }
    }
    fn drive<'a, P: SourcePuller + Sync + 'a>(client: &'a PullClient<P>)
    where
        P::Pull<'a>: Send,
    {
        fn send<T: Send>(_: &T) {}
        let mut future = Box::pin(client.pull_source(request(), CancellationToken::new()));
        send(&future);
        assert!(matches!(poll(future.as_mut()), Poll::Ready(Ok(_))));
    }
    let calls = AtomicUsize::new(0);
    let provider = Borrowed(&calls);
    drive(&PullClient::new(&provider));
    drive(&PullClient::new(Borrowed(&calls)));
    drive(&PullClient::new(Box::new(Borrowed(&calls))));
    drive(&PullClient::new(Arc::new(Borrowed(&calls))));
    assert_eq!(calls.load(Ordering::Relaxed), 4);
}

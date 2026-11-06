use openbindings::JsonValue;
use openbindings_kind_description::*;
use std::{
    cell::{Cell, RefCell},
    convert::Infallible,
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll, Waker},
};

fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn describe(kind: &str) -> DescribeKindRequest {
    DescribeKindRequest::from_json(value(&serde_json::json!({"kind":kind}).to_string())).unwrap()
}
fn check(text: &str) -> CheckSourceRequest {
    CheckSourceRequest::from_json(value(text)).unwrap()
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut TaskContext::from_waker(Waker::noop()))
}
fn complete<F: Future>(future: F) -> F::Output {
    match poll(Box::pin(future).as_mut()) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready provider"),
    }
}

#[derive(Default)]
struct Kinds {
    queries: Cell<usize>,
    checks: Cell<usize>,
    mismatch: bool,
}
impl KindDescriptor for Kinds {
    type Error = Infallible;
    type List<'a> = Ready<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Ready<Result<KindDescription, Self::Error>>;
    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        self.queries.set(self.queries.get() + 1);
        ready(Ok(["example.text@1", "example.embedded@1"].map(|kind| {
            KindSummary::from_json(value(&serde_json::json!({"kind":kind,"supports":["openbindings.kind-descriptor.checkSource"]}).to_string())).unwrap()
        }).to_vec()))
    }
    fn handle_describe_kind(
        &self,
        request: DescribeKindRequest,
        _: CancellationToken,
    ) -> Self::Describe<'_> {
        self.queries.set(self.queries.get() + 1);
        let kind = if self.mismatch {
            "wrong"
        } else {
            request.kind().as_str().unwrap()
        };
        let mut description = serde_json::json!({"kind":kind,"description":"illustrative kind","supports":["openbindings.kind-descriptor.checkSource"],"check":{"description":"reads only the supplied proposal","executes":false}});
        if kind == "example.embedded@1" {
            description["sourceData"] = serde_json::json!({"type":"object"});
        }
        ready(Ok(KindDescription::from_json(value(
            &description.to_string(),
        ))
        .unwrap()))
    }
}
impl SourceChecker for Kinds {
    type Error = Infallible;
    type Check<'a> = Ready<Result<CheckedSource, CheckFailure>>;
    fn handle_check_source(
        &self,
        request: CheckSourceRequest,
        _: CancellationToken,
    ) -> Self::Check<'_> {
        self.checks.set(self.checks.get() + 1);
        let kind = if self.mismatch {
            "wrong"
        } else {
            request.kind().as_str().unwrap()
        };
        if kind == "example.embedded@1" && request.context().is_none() {
            return ready(Err(CheckFailure::ContextRequired(ContextChallenge::try_from(value(r#"{"message":"provide the required context","schema":{"type":"object","required":["key"],"properties":{"key":{"type":"string"}}}}"#)).unwrap())));
        }
        let output = if kind == "example.embedded@1" {
            format!(
                r#"{{"kind":"{kind}","description":"embedded source","data":{}}}"#,
                request.data().unwrap().text()
            )
        } else {
            serde_json::json!({"kind":kind,"description":"text source"}).to_string()
        };
        ready(Ok(CheckedSource::from_json(value(&output)).unwrap()))
    }
}

#[test]
fn two_kinds_keep_queries_checks_context_and_authored_data_separate() {
    let kinds = Rc::new(Kinds::default());
    let client = kinds.clone();
    let listed = complete(client.list_kinds(CancellationToken::new())).unwrap();
    assert_eq!(listed.len(), 2);
    let description =
        complete(client.describe_kind(describe("example.text@1"), CancellationToken::new()))
            .unwrap();
    assert!(description.source_data().is_none());
    assert!(!description.check().unwrap().executes());
    let embedded =
        complete(client.describe_kind(describe("example.embedded@1"), CancellationToken::new()))
            .unwrap();
    assert!(embedded.source_data().is_some());
    assert_eq!(
        kinds.checks.get(),
        0,
        "capability queries perform no checks"
    );
    let checker = &*kinds;
    let text = complete(checker.check_source(
        check(r#"{"kind":"example.text@1"}"#),
        CancellationToken::new(),
    ))
    .unwrap();
    assert!(text.data().is_none());
    let outcome = complete(checker.check_source(
        check(r#"{"kind":"example.embedded@1","data":{"n":9007199254740993}}"#),
        CancellationToken::new(),
    ));
    assert!(matches!(
        outcome,
        Err(KindError::Provider(CheckFailure::ContextRequired(_)))
    ));
    assert_eq!(
        kinds.checks.get(),
        2,
        "no implicit context acquisition or retry"
    );
    let source = complete(checker.check_source(check(r#"{"kind":"example.embedded@1","data":{"n":9007199254740993},"context":{"key":"explicit"}}"#), CancellationToken::new())).unwrap();
    assert_eq!(
        source.data().unwrap().get("n").unwrap().number_text(),
        Some("9007199254740993")
    );
    assert!(source.value().value().get("context").is_none());
}

#[test]
fn clients_enforce_exact_request_response_kind_identity() {
    let provider = Kinds {
        mismatch: true,
        ..Default::default()
    };
    assert!(matches!(
        complete(provider.describe_kind(describe("expected"), CancellationToken::new())),
        Err(KindError::KindMismatch)
    ));
    assert!(matches!(
        complete(provider.check_source(check(r#"{"kind":"expected"}"#), CancellationToken::new())),
        Err(KindError::KindMismatch)
    ));
}

#[test]
fn dispatch_is_lazy_and_pre_cancelled_calls_do_not_reach_providers() {
    let provider = Kinds::default();
    let client = &provider;
    let future = client.list_kinds(CancellationToken::new());
    drop(future);
    assert_eq!(provider.queries.get(), 0);
    let control = CancellationToken::new();
    control.cancel();
    assert!(matches!(
        complete(client.describe_kind(describe("k"), control.clone())),
        Err(KindError::Cancelled)
    ));
    assert!(matches!(
        complete(client.list_kinds(control.clone())),
        Err(KindError::Cancelled)
    ));
    assert!(matches!(
        complete(provider.check_source(check(r#"{"kind":"k"}"#), control)),
        Err(KindError::Cancelled)
    ));
    assert_eq!(provider.queries.get() + provider.checks.get(), 0);
}

struct Waiting {
    called: Cell<usize>,
    child: RefCell<Option<CancellationToken>>,
    dropped: Rc<Cell<bool>>,
}
struct PendingCheck(Rc<Cell<bool>>, CancellationToken);
impl Future for PendingCheck {
    type Output = Result<CheckedSource, CheckFailure>;
    fn poll(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for PendingCheck {
    fn drop(&mut self) {
        assert!(self.1.is_cancelled(), "cancel before dropping pending work");
        self.0.set(true);
    }
}
impl SourceChecker for Waiting {
    type Error = Infallible;
    type Check<'a> = PendingCheck;
    fn handle_check_source(
        &self,
        _: CheckSourceRequest,
        control: CancellationToken,
    ) -> Self::Check<'_> {
        self.called.set(self.called.get() + 1);
        *self.child.borrow_mut() = Some(control.clone());
        PendingCheck(self.dropped.clone(), control)
    }
}
#[test]
fn external_cancel_and_future_drop_stop_only_the_local_attempt() {
    for external_cancel in [true, false] {
        let provider = Waiting {
            called: Cell::new(0),
            child: RefCell::new(None),
            dropped: Rc::new(Cell::new(false)),
        };
        let caller = CancellationToken::new();
        let client = &provider;
        let mut future = Box::pin(client.check_source(check(r#"{"kind":"k"}"#), caller.clone()));
        assert!(poll(future.as_mut()).is_pending());
        assert_eq!(provider.called.get(), 1);
        let child = provider.child.borrow().as_ref().unwrap().clone();
        assert!(!child.is_cancelled());
        if external_cancel {
            caller.cancel();
            assert!(
                child.is_cancelled(),
                "synchronous relay before polling again"
            );
            assert!(matches!(
                poll(future.as_mut()),
                Poll::Ready(Err(KindError::Cancelled))
            ));
        }
        drop(future);
        assert!(child.is_cancelled());
        assert!(provider.dropped.get());
        assert_eq!(
            caller.is_cancelled(),
            external_cancel,
            "local completion never cancels caller"
        );
    }
}

#[test]
fn finite_handoff_limits_are_local_failures_and_do_not_dispatch_oversized_requests() {
    let provider = Kinds::default();
    let limits = DescriptionLimits {
        max_request_bytes: 1,
        ..Default::default()
    };
    assert!(matches!(
        complete(provider.describe_kind_with_limits(
            describe("k"),
            CancellationToken::new(),
            limits
        )),
        Err(KindError::LimitExceeded("request-bytes"))
    ));
    assert!(matches!(
        complete(provider.check_source_with_limits(
            check(r#"{"kind":"k"}"#),
            CancellationToken::new(),
            limits
        )),
        Err(KindError::LimitExceeded("request-bytes"))
    ));
    assert_eq!(provider.queries.get() + provider.checks.get(), 0);
    let client = &provider;
    let client_limits = DescriptionLimits {
        max_listed_kinds: 1,
        ..Default::default()
    };
    assert!(matches!(
        complete(client.list_kinds_with_limits(CancellationToken::new(), client_limits)),
        Err(KindError::LimitExceeded("listed-kinds"))
    ));
    let client = &provider;
    let client_limits = DescriptionLimits {
        max_response_bytes: 1,
        ..Default::default()
    };
    assert!(matches!(
        complete(client.describe_kind_with_limits(
            describe("k"),
            CancellationToken::new(),
            client_limits
        )),
        Err(KindError::LimitExceeded("response-bytes"))
    ));
}

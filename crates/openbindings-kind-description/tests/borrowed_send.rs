use openbindings::JsonValue;
use openbindings_kind_description::*;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

struct Borrowed<'env>(&'env AtomicUsize);
type Query<'a, T> = Pin<Box<dyn Future<Output = Result<T, ()>> + Send + 'a>>;
impl KindDescriptor for Borrowed<'_> {
    type Error = ();
    type List<'a>
        = Query<'a, Vec<KindSummary>>
    where
        Self: 'a;
    type Describe<'a>
        = Query<'a, KindDescription>
    where
        Self: 'a;
    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(vec![
                KindSummary::from_json(
                    JsonValue::parse(r#"{"kind":"example","supports":[]}"#).unwrap(),
                )
                .unwrap(),
            ])
        })
    }
    fn handle_describe_kind(
        &self,
        _: DescribeKindRequest,
        _: CancellationToken,
    ) -> Self::Describe<'_> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(KindDescription::from_json(
                JsonValue::parse(r#"{"kind":"example","description":"local kind","supports":[]}"#)
                    .unwrap(),
            )
            .unwrap())
        })
    }
}
impl SourceChecker for Borrowed<'_> {
    type Error = ();
    type Check<'a>
        =
        Pin<Box<dyn Future<Output = Result<CheckedSource, CheckFailure<Self::Error>>> + Send + 'a>>
    where
        Self: 'a;
    fn handle_check_source(&self, _: CheckSourceRequest, _: CancellationToken) -> Self::Check<'_> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(
                CheckedSource::from_json(JsonValue::parse(r#"{"kind":"example"}"#).unwrap())
                    .unwrap(),
            )
        })
    }
}
fn send<T: Send>(_: &T) {}
fn complete<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready local provider"),
    }
}
fn queries<'a, P: KindDescriptor + Sync + 'a>(client: &'a P)
where
    P::List<'a>: Send,
    P::Describe<'a>: Send,
    P::Error: Send,
{
    let list = client.list_kinds(CancellationToken::new());
    send(&list);
    assert!(complete(list).is_ok());
    let request =
        DescribeKindRequest::from_json(JsonValue::parse(r#"{"kind":"example"}"#).unwrap()).unwrap();
    let describe = client.describe_kind(request, CancellationToken::new());
    send(&describe);
    assert!(complete(describe).is_ok());
}
fn checks<'a, P: SourceChecker + Sync + 'a>(client: &'a P)
where
    P::Check<'a>: Send,
    P::Error: Send,
{
    let request =
        CheckSourceRequest::from_json(JsonValue::parse(r#"{"kind":"example"}"#).unwrap()).unwrap();
    let check = client.check_source(request, CancellationToken::new());
    send(&check);
    assert!(complete(check).is_ok());
}
#[test]
fn all_three_interfaces_accept_non_static_send_futures_for_each_provider_ownership() {
    let calls = AtomicUsize::new(0);
    let provider = Borrowed(&calls);
    queries(&&provider);
    queries(&Borrowed(&calls));
    queries(&Box::new(Borrowed(&calls)));
    queries(&Arc::new(Borrowed(&calls)));
    checks(&&provider);
    checks(&Borrowed(&calls));
    checks(&Box::new(Borrowed(&calls)));
    checks(&Arc::new(Borrowed(&calls)));
    assert_eq!(calls.load(Ordering::Relaxed), 12);
}

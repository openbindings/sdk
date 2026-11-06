use openbindings::JsonValue;
use openbindings_kind_description::*;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn request() -> CheckSourceRequest {
    CheckSourceRequest::from_json(json(r#"{"kind":"k"}"#)).unwrap()
}
fn complete<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected immediate provider"),
    }
}

#[derive(Clone, Copy)]
enum Boundary {
    None,
    Constructor,
    Poll,
    Drop,
}
struct Finished<T> {
    outcome: Option<T>,
    caller: CancellationToken,
    child: CancellationToken,
    boundary: Boundary,
}
impl<T: Unpin> Future for Finished<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        if matches!(self.boundary, Boundary::Poll) {
            self.caller.cancel();
        }
        Poll::Ready(self.outcome.take().unwrap())
    }
}
impl<T> Drop for Finished<T> {
    fn drop(&mut self) {
        assert!(
            self.child.is_cancelled(),
            "cancel before releasing provider work"
        );
        if matches!(self.boundary, Boundary::Drop) {
            self.caller.cancel();
        }
    }
}
struct Provider {
    caller: CancellationToken,
    boundary: Boundary,
    mode: usize,
}
impl Provider {
    fn finish<T>(&self, outcome: T, child: CancellationToken) -> Finished<T> {
        if matches!(self.boundary, Boundary::Constructor) {
            self.caller.cancel();
        }
        Finished {
            outcome: Some(outcome),
            caller: self.caller.clone(),
            child,
            boundary: self.boundary,
        }
    }
}
impl KindDescriptor for Provider {
    type Error = CarrierError;
    type List<'a> = Finished<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Finished<Result<KindDescription, Self::Error>>;
    fn handle_list_kinds(&self, control: CancellationToken) -> Self::List<'_> {
        self.finish(
            if self.mode == 0 {
                Ok(vec![])
            } else {
                Err(CarrierError::ExpectedObject)
            },
            control,
        )
    }
    fn handle_describe_kind(
        &self,
        _: DescribeKindRequest,
        control: CancellationToken,
    ) -> Self::Describe<'_> {
        self.finish(
            if self.mode == 0 {
                KindDescription::from_json(json(
                    r#"{"kind":"k","description":"known kind","supports":[]}"#,
                ))
            } else {
                Err(CarrierError::ExpectedObject)
            },
            control,
        )
    }
}
impl SourceChecker for Provider {
    type Error = CarrierError;
    type Check<'a> = Finished<Result<CheckedSource, CheckFailure<Self::Error>>>;
    fn handle_check_source(
        &self,
        _: CheckSourceRequest,
        control: CancellationToken,
    ) -> Self::Check<'_> {
        let outcome = match self.mode {
            0 => CheckedSource::from_json(json(r#"{"kind":"k"}"#)).map_err(CheckFailure::Operational),
            1 => Err(CheckFailure::Rejected(CheckRejection::from_json(json(
                r#"{"code":"NO_ENTRY","message":"Entry is unavailable","data":9007199254740993}"#,
            )).unwrap())),
            2 => Err(CheckFailure::ContextRequired(ContextChallenge::try_from(json(
                r#"{"message":"Provide context","schema":{"type":"object"}}"#,
            )).unwrap())),
            3 => CheckedSource::from_json(json(r#"{"kind":"k","context":{}}"#)).map_err(CheckFailure::Operational),
            _ => Err(CheckFailure::Cancelled),
        };
        self.finish(outcome, control)
    }
}

#[test]
fn cancellation_precedes_every_success_and_failure_after_provider_cleanup() {
    for boundary in [Boundary::Constructor, Boundary::Poll, Boundary::Drop] {
        for mode in 0..5 {
            for operation in 0..3 {
                let caller = CancellationToken::new();
                let provider = Provider {
                    caller: caller.clone(),
                    boundary,
                    mode,
                };
                match operation {
                    0 => assert!(matches!(
                        complete(provider.list_kinds(caller)),
                        Err(KindError::Cancelled)
                    )),
                    1 => assert!(matches!(
                        complete(provider.describe_kind(
                            DescribeKindRequest::from_json(json(r#"{"kind":"k"}"#)).unwrap(),
                            caller
                        )),
                        Err(KindError::Cancelled)
                    )),
                    _ => assert!(matches!(
                        complete(provider.check_source(request(), caller)),
                        Err(KindError::Cancelled)
                    )),
                }
            }
        }
    }
}

#[test]
fn malformed_provider_outputs_remain_operational_failures_and_never_kind_rejections() {
    let caller = CancellationToken::new();
    let provider = Provider {
        caller: caller.clone(),
        boundary: Boundary::None,
        mode: 3,
    };
    let error = complete((provider).check_source(request(), caller.clone())).unwrap_err();
    assert!(matches!(
        error,
        KindError::Provider(CheckFailure::Operational(CarrierError::UnexpectedMember))
    ));
    assert!(!caller.is_cancelled());
    // Diagnostic formatting never prints native provider details implicitly.
    let secret = CheckFailure::Operational("private endpoint or credential");
    assert!(!format!("{secret}: {secret:?}").contains("private"));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn check_failures_keep_exact_payloads_and_are_subject_to_carrier_handoff_bounds() {
    for mode in [1, 2] {
        let caller = CancellationToken::new();
        let provider = Provider {
            caller: caller.clone(),
            boundary: Boundary::None,
            mode,
        };
        let error = complete(provider.check_source(request(), caller.clone())).unwrap_err();
        match error {
            KindError::Provider(CheckFailure::Rejected(value)) => {
                assert_eq!(value.data().unwrap().text(), "9007199254740993")
            }
            KindError::Provider(CheckFailure::ContextRequired(value)) => {
                assert!(value.value().value().get("schema").is_some())
            }
            other => panic!("unexpected error: {other:?}"),
        }
        let limited = &provider;
        let limited_limits = DescriptionLimits {
            max_response_bytes: 1,
            ..Default::default()
        };
        assert!(matches!(
            complete(limited.check_source_with_limits(request(), caller.clone(), limited_limits)),
            Err(KindError::LimitExceeded("response-bytes"))
        ));
        assert!(!caller.is_cancelled());
    }
}

//! A provider with no inspection, synthesis, HTTP, evaluator or executor dependency.

use openbindings_invocation::CancellationToken;
use openbindings_invocation::{
    BindingRequest, InvocationError, ProviderIo, binding::BindingInvoker,
};
use std::future::{Ready, ready};
pub struct InvokerOnly;
impl BindingInvoker for InvokerOnly {
    type Invocation<'a> = Ready<Result<(), InvocationError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == "example.no-output"
    }
    fn invoke<'a>(
        &'a self,
        _: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        io.close_input();
        ready(Ok(()))
    }
}
#[cfg(test)]
#[test]
fn invocation_only_default_preflight_and_no_output() {
    use openbindings::Source;
    use openbindings_invocation::{BindingValue, binding::Invoker};
    use std::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };
    fn ready_now<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("ready-only provider unexpectedly parked"),
        }
    }
    let source = Source {
        kind: "example.no-output".into(),
        ..Default::default()
    };
    let binding = BindingValue::default();
    let request = BindingRequest {
        source: &source,
        binding: &binding,
        context: None,
    };
    let facade = Invoker::new(InvokerOnly);
    assert!(ready_now(facade.preflight(request)).unwrap().is_none());
    let mut invocation = facade.invoke(request);
    assert!(ready_now(invocation.next()).is_none());
    assert!(invocation.outcome().unwrap().is_ok());
}

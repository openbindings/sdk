//! Binding invocation and advisory preflight over an explicitly supplied provider.
use crate::{
    BindingRequest, BoundaryFailure, CancellationToken, ContextChallenge, Invocation,
    InvocationError, InvocationLimits, ProviderIo, channel::Shared,
};
use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::Arc,
    task::Poll,
};

/// Execute an explicitly addressed binding. Neither method selects a binding or
/// validates operation values. Futures may borrow provider configuration and need
/// not implement Send. The provider owns kind-defined effects and cleanup.
/// See [writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md)
/// for a complete implementation and the future-drop cancellation model.
pub trait BindingInvoker {
    /// Lazy invocation work, completed with exactly one terminal result.
    type Invoke<'a>: Future<Output = Result<(), InvocationError>>
    where
        Self: 'a;
    /// Separate advisory work. A provider without a prediction can return Ready.
    type Preflight<'a>: Future<Output = Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;
    /// Run the selected binding. CONTEXT_REQUIRED precedes every output and
    /// observable operation effect; ERR_REFUSED precedes operation effects.
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a>;
    /// Report a prediction without consuming input, emitting output, dispatching
    /// the requested operation or spending its approval. Ordinary errors carry
    /// no prediction; `ERR_REFUSED` predicts refusal of the same input.
    /// `Ok(None)` means no prediction, not a promise invocation will proceed.
    /// With `type Preflight<'a> = std::future::Ready<Result<Option<ContextChallenge>, InvocationError>>`,
    /// a provider with no prediction implements the body as `std::future::ready(Ok(None))`.
    /// The [binding-invoker interface](https://github.com/openbindings/interfaces/blob/67f707c31aecbdf0551385e369d8024097db6278/binding-invoker/README.md)
    /// states that `null` is always conformant, even if invocation needs context.
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a>;
}
/// Optional SDK routing knowledge for operation-name selection. This is not a
/// portable kind-support query. Explicit binding invocation does not consult it.
/// A provider without routing knowledge implements `can_invoke` as `false`,
/// never a blanket `true`. Operation-name selection then fails closed with
/// `ERR_BINDING_NOT_FOUND`; explicit bindings are unaffected.
pub trait InvocationSupport {
    /// Whether this configuration can attempt the source's exact kind identifier.
    fn can_invoke(&self, kind: &str) -> bool;
}
macro_rules! forward { ($($wrapper:ty),+) => { $(
    impl<P: BindingInvoker + ?Sized> BindingInvoker for $wrapper {
        type Invoke<'a> = P::Invoke<'a> where Self: 'a;
        type Preflight<'a> = P::Preflight<'a> where Self: 'a;
        fn handle_invoke<'a>(&'a self, request: BindingRequest, io: ProviderIo,
            control: CancellationToken) -> Self::Invoke<'a> { (**self).handle_invoke(request, io, control) }
        fn handle_preflight<'a>(&'a self, request: BindingRequest,
            control: CancellationToken) -> Self::Preflight<'a> { (**self).handle_preflight(request, control) }
    }
    impl<P: InvocationSupport + ?Sized> InvocationSupport for $wrapper {
        fn can_invoke(&self, kind: &str) -> bool { (**self).can_invoke(kind) }
    }
)+ }; }
forward!(&P, Box<P>, Arc<P>, std::rc::Rc<P>);

pub(crate) fn require_binding(request: &BindingRequest) -> Result<(), InvocationError> {
    if request.interface.binding(&request.binding).is_some() {
        Ok(())
    } else {
        Err(InvocationError::binding_not_found())
    }
}
pub(crate) fn error(code: &str) -> InvocationError {
    InvocationError::new(code, None).expect("SDK-owned nonempty error code")
}
pub(crate) fn provider_result(
    result: Result<(), InvocationError>,
    shared: &Shared,
) -> Result<(), InvocationError> {
    result.map_err(|error| {
        error.admit_provider_code().unwrap_or_else(|finding| {
            shared.record_finding(finding);
            InvocationError::execution_failed()
        })
    })
}
pub(crate) fn preflight<'a, P: BindingInvoker + ?Sized>(
    provider: &'a P,
    limits: InvocationLimits,
    request: Result<BindingRequest, InvocationError>,
    external: CancellationToken,
) -> impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + 'a + use<'a, P> {
    let local = CancellationToken::new();
    let provider_control = local.clone();
    preflight_job(
        move || {
            let request = request?;
            require_binding(&request)?;
            Ok(provider.handle_preflight(request, provider_control))
        },
        limits,
        external,
        local,
    )
}
async fn preflight_job<F: Future<Output = Result<Option<ContextChallenge>, InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    limits: InvocationLimits,
    external: CancellationToken,
    local: CancellationToken,
) -> Result<Option<ContextChallenge>, InvocationError> {
    let cancel_on_drop = CancelOnDrop(local.clone());
    let _relay = cancellation_relay(&external, &local);
    if local.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(local.cancelled());
    let mut job = pin!(start()?);
    // Protect constructor panics, then cancel before the provider future drops.
    let _cancel_before_job_drop = cancel_on_drop;
    let result = poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(InvocationError::cancelled()));
        }
        let result = job.as_mut().poll(cx);
        if local.is_cancelled() {
            Poll::Ready(Err(InvocationError::cancelled()))
        } else {
            result
        }
    })
    .await
    .map_err(|error| {
        // Requirements belong only to the successful advisory result.
        if error.code() == "CONTEXT_REQUIRED" {
            InvocationError::execution_failed()
        } else {
            error
                .admit_provider_code()
                .unwrap_or_else(|_| InvocationError::execution_failed())
        }
    });
    let bounds = openbindings::JsonLimits {
        max_bytes: limits.max_error_bytes,
        ..Default::default()
    };
    let bounded = match &result {
        Ok(None) => true,
        Ok(Some(challenge)) => crate::boundary::encode(challenge.value().value(), bounds).is_ok(),
        Err(error) => error.to_json_with_limits(bounds).is_ok(),
    };
    if bounded {
        result
    } else {
        Err(InvocationError::execution_failed())
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
// Shared with the other asynchronous companions; scoped to this call.
pub(crate) use openbindings_internal_cancellation::relay as cancellation_relay;

/// External cancellation revokes shared handoffs synchronously. Independently
/// polled provider handles cannot release accepted values after cancel returns.
pub(crate) fn invocation_relay(
    external: &CancellationToken,
    shared: &Arc<Shared>,
) -> crate::Cancellation {
    struct Cancel(Arc<Shared>);
    impl std::task::Wake for Cancel {
        fn wake(self: Arc<Self>) {
            self.0.settle(Err(InvocationError::cancelled()));
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.settle(Err(InvocationError::cancelled()));
        }
    }
    let waker = std::task::Waker::from(Arc::new(Cancel(shared.clone())));
    let mut registration = external.cancelled();
    if std::pin::Pin::new(&mut registration)
        .poll(&mut std::task::Context::from_waker(&waker))
        .is_ready()
    {
        shared.settle(Err(InvocationError::cancelled()));
    }
    registration
}

/// Checked binding calls. Import this trait to call a [`BindingInvoker`].
/// Implementers provide `handle_*` hooks; this blanket implementation owns admission,
/// cancellation and result checks. Explicit hook calls bypass these checks.
pub trait BindingInvokerExt: BindingInvoker {
    /// Admit a request and return a lazy, output-driven invocation.
    /// # Errors
    /// Returns local request admission failure before any provider work.
    fn invoke(
        &self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>;
    /// Invoke with caller-selected handoff limits.
    /// # Errors
    /// Rejects invalid limits or malformed requests before any provider work.
    fn invoke_with_limits(
        &self,
        request: BindingRequest,
        control: CancellationToken,
        limits: InvocationLimits,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>;
    /// Admit an advisory request without constructing invocation I/O.
    /// `None` means no prediction; it does not promise invocation will proceed.
    /// # Errors
    /// Returns local request admission failure before advisory work.
    fn preflight(
        &self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    >;
    /// Preflight with caller-selected error/challenge encoding limits.
    /// # Errors
    /// Rejects invalid limits or malformed requests before advisory work.
    fn preflight_with_limits(
        &self,
        request: BindingRequest,
        control: CancellationToken,
        limits: InvocationLimits,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    >;
}
impl<P: BindingInvoker + ?Sized> BindingInvokerExt for P {
    fn invoke(
        &self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>
    {
        invoke(self, InvocationLimits::default(), request, control)
    }
    fn invoke_with_limits(
        &self,
        request: BindingRequest,
        control: CancellationToken,
        limits: InvocationLimits,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>
    {
        invoke(self, limits.validate()?, request, control)
    }
    fn preflight(
        &self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    > {
        preflight_call(self, InvocationLimits::default(), request, control)
    }
    fn preflight_with_limits(
        &self,
        request: BindingRequest,
        control: CancellationToken,
        limits: InvocationLimits,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    > {
        preflight_call(self, limits.validate()?, request, control)
    }
}

pub(crate) fn invoke<'a, P: BindingInvoker + ?Sized>(
    provider: &'a P,
    limits: InvocationLimits,
    request: BindingRequest,
    control: CancellationToken,
) -> Result<
    Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>>,
    BoundaryFailure,
> {
    request.admit()?;
    let shared = Shared::new(limits, [false; 2]);
    let local = shared.control();
    let io = ProviderIo::new(shared.clone());
    let future = crate::channel::driver::drive(
        move || {
            require_binding(&request)?;
            Ok(provider.handle_invoke(request, io, local.clone()))
        },
        control,
        shared.clone(),
        provider_result,
    );
    Ok(Invocation::from_parts(shared, future))
}

pub(crate) fn preflight_call<'a, P: BindingInvoker + ?Sized>(
    provider: &'a P,
    limits: InvocationLimits,
    request: BindingRequest,
    control: CancellationToken,
) -> Result<
    impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + 'a + use<'a, P>,
    BoundaryFailure,
> {
    request.admit()?;
    Ok(preflight(provider, limits, Ok(request), control))
}

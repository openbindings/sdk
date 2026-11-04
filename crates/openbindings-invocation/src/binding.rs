//! Binding invocation and advisory preflight over an explicitly supplied provider.
use crate::{
    BindingRequest, BoundaryFailure, CancellationToken, ContextChallenge, Invocation,
    InvocationError, InvocationLimits, LimitError, ProviderIo, channel::Shared,
};
use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::Arc,
    task::Poll,
};

/// Advisory requirements for this request only. Invocation resolves its own needs.
#[derive(Clone, Debug)]
pub enum PreflightOutcome {
    /// No requirements predicted. This does not promise an invocation will proceed.
    NoPrediction,
    /// Context the provider can already identify, without dispatching the operation.
    Challenge(ContextChallenge),
}
/// Execute an explicitly addressed binding. Neither method selects a binding or
/// validates operation values. Futures may borrow provider configuration and need
/// not implement Send. The provider owns kind-defined effects and cleanup.
pub trait BindingInvoker {
    /// Lazy invocation work, completed with exactly one terminal result.
    type Invoke<'a>: Future<Output = Result<(), InvocationError>>
    where
        Self: 'a;
    /// Separate advisory work. A provider without a prediction can return Ready.
    type Preflight<'a>: Future<Output = Result<PreflightOutcome, InvocationError>>
    where
        Self: 'a;
    /// Run the selected binding. CONTEXT_REQUIRED precedes every output and
    /// observable operation effect; ERR_REFUSED precedes operation effects.
    fn invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a>;
    /// Report a prediction without consuming input, emitting output, dispatching
    /// the requested operation or spending its approval. Ordinary errors carry
    /// no prediction; `ERR_REFUSED` predicts refusal of the same input.
    fn preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Self::Preflight<'a>;
}
/// Optional SDK routing knowledge for operation-name selection. This is not a
/// portable kind-support query. Explicit binding invocation does not consult it.
pub trait InvocationSupport {
    /// Whether this configuration can attempt the source's exact kind identifier.
    fn can_invoke(&self, kind: &str) -> bool;
}
macro_rules! forward { ($($wrapper:ty),+) => { $(
    impl<P: BindingInvoker + ?Sized> BindingInvoker for $wrapper {
        type Invoke<'a> = P::Invoke<'a> where Self: 'a;
        type Preflight<'a> = P::Preflight<'a> where Self: 'a;
        fn invoke<'a>(&'a self, request: BindingRequest, io: ProviderIo,
            control: CancellationToken) -> Self::Invoke<'a> { (**self).invoke(request, io, control) }
        fn preflight<'a>(&'a self, request: BindingRequest,
            control: CancellationToken) -> Self::Preflight<'a> { (**self).preflight(request, control) }
    }
    impl<P: InvocationSupport + ?Sized> InvocationSupport for $wrapper {
        fn can_invoke(&self, kind: &str) -> bool { (**self).can_invoke(kind) }
    }
)+ }; }
forward!(&P, Box<P>, Arc<P>, std::rc::Rc<P>);

/// Checked binding entry point without a registry, implicit executor or evaluator.
pub struct BindingClient<P> {
    provider: P,
    limits: InvocationLimits,
}
impl<P> std::fmt::Debug for BindingClient<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BindingClient").finish_non_exhaustive()
    }
}
impl<P: BindingInvoker> BindingClient<P> {
    /// Retain an explicit provider, using the default finite handoff policy.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            limits: InvocationLimits::default(),
        }
    }
    /// Select finite queue and encoding bounds for subsequent attempts.
    /// # Errors
    /// Rejects zero capacities or insufficient terminal encoding space.
    pub fn with_limits(mut self, limits: InvocationLimits) -> Result<Self, LimitError> {
        self.limits = limits.validate()?;
        Ok(self)
    }
    /// Admit an owned request and return a lazy attempt driven only by its output.
    /// # Errors
    /// A malformed request is a local failure; a missing binding becomes a terminal.
    pub fn invoke<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<
        Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>>,
        BoundaryFailure,
    > {
        request.admit()?;
        let shared = Shared::new(self.limits, [false; 2]);
        let local = shared.control();
        let io = ProviderIo::new(shared.clone());
        let provider = &self.provider;
        let future = checked(
            move || {
                require_binding(&request)?;
                Ok(provider.invoke(request, io, local.clone()))
            },
            control,
            shared.clone(),
        );
        Ok(Invocation::new(shared, future))
    }
    /// Admit an advisory request. The resulting future borrows only the provider;
    /// it owns the request and cancellation and never constructs invocation I/O.
    /// # Errors
    /// Returns local admission failure before starting advisory work.
    pub fn preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<PreflightOutcome, InvocationError>> + 'a + use<'a, P>,
        BoundaryFailure,
    > {
        request.admit()?;
        Ok(preflight(&self.provider, self.limits, Ok(request), control))
    }
}
pub(crate) fn require_binding(request: &BindingRequest) -> Result<(), InvocationError> {
    if request.interface.binding(&request.binding).is_some() {
        Ok(())
    } else {
        Err(error("ERR_BINDING_NOT_FOUND"))
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
/// Keep the GAT projection outside the state machine to preserve conditional Send.
pub(crate) async fn checked<F: Future<Output = Result<(), InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    external: CancellationToken,
    shared: Arc<Shared>,
) -> Result<(), InvocationError> {
    let local = shared.control();
    let _relay = invocation_relay(&external, &shared);
    if local.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(local.cancelled());
    let mut job = pin!(start()?);
    poll_fn(|cx| {
        if let Some(outcome) = shared.outcome() {
            return Poll::Ready(outcome);
        }
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(InvocationError::cancelled()));
        }
        let result = job.as_mut().poll(cx);
        if let Some(outcome) = shared.outcome() {
            Poll::Ready(outcome)
        } else if local.is_cancelled() {
            Poll::Ready(Err(InvocationError::cancelled()))
        } else {
            result.map(|result| provider_result(result, &shared))
        }
    })
    .await
}
pub(crate) fn preflight<'a, P: BindingInvoker>(
    provider: &'a P,
    limits: InvocationLimits,
    request: Result<BindingRequest, InvocationError>,
    external: CancellationToken,
) -> impl Future<Output = Result<PreflightOutcome, InvocationError>> + 'a + use<'a, P> {
    let local = CancellationToken::new();
    let provider_control = local.clone();
    preflight_job(
        move || {
            let request = request?;
            require_binding(&request)?;
            Ok(provider.preflight(request, provider_control))
        },
        limits,
        external,
        local,
    )
}
async fn preflight_job<F: Future<Output = Result<PreflightOutcome, InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    limits: InvocationLimits,
    external: CancellationToken,
    local: CancellationToken,
) -> Result<PreflightOutcome, InvocationError> {
    let _cancel_on_drop = CancelOnDrop(local.clone());
    let _relay = cancellation_relay(&external, &local);
    if local.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(local.cancelled());
    let mut job = pin!(start()?);
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
        Ok(PreflightOutcome::NoPrediction) => true,
        Ok(PreflightOutcome::Challenge(challenge)) => {
            crate::boundary::encode(challenge.value().value(), bounds).is_ok()
        }
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

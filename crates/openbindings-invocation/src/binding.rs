//! Binding-kind provider seam and checked consumer entry point.
use crate::CancellationToken;
use crate::{
    BindingRequest, BoundaryError, ContextChallenge, Invocation, InvocationError, InvocationLimits,
    LimitError, ProviderIo, lifecycle::Shared,
};
use openbindings_internal_interfaces::carrier::{BoundaryCode, boundary_error};

// One definition of the records every kind-facing interface shares.
pub use openbindings_internal_interfaces::{KindSupportVerdict, SupportedKind};

use std::{
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};

/// Implement binding-kind execution. Support checks must be pure; invocation
/// starts lazily and runs once after carrier and exact-kind admission.
pub trait BindingInvoker {
    /// Ordinary provider job, borrowing configuration if needed. Send is optional.
    type Invocation<'a>: Future<Output = Result<(), InvocationError>>
    where
        Self: 'a;
    /// Pure exact-kind verdict for this invocation implementation/configuration.
    fn supports_kind(&self, kind: &str) -> bool;
    /// Advisory list. An unlisted kind may still be supported.
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        Vec::new()
    }
    /// Execute once after checked admission. Return one terminal outcome.
    /// CONTEXT_REQUIRED requires no prior output or observable operation effects;
    /// ERR_REFUSED requires no observable operation effects. The provider owns
    /// those effect guarantees and all external acquisition/cleanup.
    fn invoke<'a>(
        &'a self,
        request: BindingRequest<'a>,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invocation<'a>;
    /// The provider's advisory preflight hook; none by default.
    ///
    /// Return a job that predicts what invoking this request would require,
    /// from the source and the supplied context, or `None` to make no
    /// prediction. The facades' `preflight` always runs its own admission and
    /// support checks first, then runs the job, without input, output or
    /// dispatch, under the preflight's cancellation. Complete it with `Ok(())`
    /// for no prediction, with the `CONTEXT_REQUIRED` challenge invocation would
    /// raise to report requirements, or with another error when the hook cannot
    /// answer; `ERR_REFUSED` means invocation would refuse the same request.
    /// The job is a [`Self::Invocation`], so a bound a caller places on that
    /// future, such as `Send`, covers preflight too. Invocation never calls this
    /// hook.
    ///
    /// Sharing the type has a cost. A prediction travels in the error channel,
    /// as `Err(InvocationError::challenge(..))`, the terminal a context-requiring
    /// invocation would end with; the facades return it as `Ok(Some(challenge))`.
    /// A provider whose invocation future is a boxed or `std::future::Ready`
    /// future builds the job freely, as below. One whose future is a type it
    /// writes by hand, such as a state machine over its transport, gives that
    /// type a preflight state: one that completes with the prediction without
    /// reading input, emitting output or dispatching.
    ///
    /// ```
    /// # use openbindings_invocation::{BindingRequest, CancellationToken, ContextChallenge, InvocationError, ProviderIo, binding::BindingInvoker};
    /// # use std::future::{Ready, ready};
    /// struct Bearer;
    /// impl BindingInvoker for Bearer {
    ///     type Invocation<'a> = Ready<Result<(), InvocationError>>;
    ///     fn supports_kind(&self, kind: &str) -> bool { kind == "urn:example:bearer:v1" }
    ///     fn invoke<'a>(&'a self, _: BindingRequest<'a>, _: ProviderIo, _: CancellationToken)
    ///         -> Self::Invocation<'a> { ready(Ok(())) }
    ///     fn preflight<'a>(&'a self, request: BindingRequest<'a>, _: CancellationToken)
    ///         -> Option<Self::Invocation<'a>>
    ///     {
    ///         // Without context, invocation would ask for a bearer token.
    ///         request.context.is_none().then(|| {
    ///             let challenge = ContextChallenge::new(openbindings::JsonValue::parse(
    ///                 r#"{"target":"api","alternatives":[{"requirements":[{"type":"auth.bearer"}]}]}"#,
    ///             ).unwrap()).unwrap();
    ///             ready(Err(InvocationError::challenge(challenge)))
    ///         })
    ///     }
    /// }
    /// ```
    ///
    /// A hand-written future carries the prediction as one of its states:
    ///
    /// ```
    /// # use openbindings_invocation::{BindingRequest, CancellationToken, ContextChallenge, InvocationError, ProviderIo, binding::BindingInvoker};
    /// # use std::{future::Future, pin::Pin, task::{Context, Poll}};
    /// enum Job {
    ///     // A real provider keeps its request state machine here.
    ///     Call(ProviderIo),
    ///     // Preflight: no input, output or dispatch, just the prediction.
    ///     Predict(Option<ContextChallenge>),
    /// }
    /// impl Future for Job {
    ///     type Output = Result<(), InvocationError>;
    ///     fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
    ///         match self.get_mut() {
    ///             Job::Call(_) => Poll::Ready(Ok(())),
    ///             Job::Predict(prediction) => Poll::Ready(match prediction.take() {
    ///                 Some(challenge) => Err(InvocationError::challenge(challenge)),
    ///                 None => Ok(()),
    ///             }),
    ///         }
    ///     }
    /// }
    /// struct Native;
    /// impl BindingInvoker for Native {
    ///     type Invocation<'a> = Job;
    ///     fn supports_kind(&self, kind: &str) -> bool { kind == "urn:example:native:v1" }
    ///     fn invoke<'a>(&'a self, _: BindingRequest<'a>, io: ProviderIo, _: CancellationToken)
    ///         -> Job { Job::Call(io) }
    ///     fn preflight<'a>(&'a self, _: BindingRequest<'a>, _: CancellationToken) -> Option<Job> {
    ///         Some(Job::Predict(None))
    ///     }
    /// }
    /// ```
    fn preflight<'a>(
        &'a self,
        request: BindingRequest<'a>,
        control: CancellationToken,
    ) -> Option<Self::Invocation<'a>> {
        let _ = (request, control);
        None
    }
}

macro_rules! forward { ($($wrapper:ty),+) => { $(
impl<P: BindingInvoker + ?Sized> BindingInvoker for $wrapper {
    type Invocation<'a> = P::Invocation<'a> where Self: 'a;
    fn supports_kind(&self, kind: &str) -> bool { (**self).supports_kind(kind) }
    fn supported_kinds(&self) -> Vec<SupportedKind> { (**self).supported_kinds() }
    fn invoke<'a>(&'a self, request: BindingRequest<'a>, io: ProviderIo, control: CancellationToken) -> Self::Invocation<'a> { (**self).invoke(request, io, control) }
    fn preflight<'a>(&'a self, request: BindingRequest<'a>, control: CancellationToken) -> Option<Self::Invocation<'a>> { (**self).preflight(request, control) }
}
)+ }; }
forward!(&P, std::sync::Arc<P>, std::rc::Rc<P>);

/// Reusable checked facade. No registry, operation selection, retries or executor.
pub struct Invoker<P> {
    pub(crate) provider: P,
    pub(crate) limits: InvocationLimits,
}
impl<P> std::fmt::Debug for Invoker<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invoker").finish_non_exhaustive()
    }
}

impl<P: BindingInvoker> Invoker<P> {
    /// Wrap a provider with the default finite handoff policy.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            limits: InvocationLimits::default(),
        }
    }
    /// Wrap a provider with an explicitly checked resource policy.
    ///
    /// # Errors
    /// Returns `LimitError` if a queue capacity or the value byte limit is zero, or the error byte limit is below 256 bytes.
    pub fn with_limits(provider: P, limits: InvocationLimits) -> Result<Self, LimitError> {
        Ok(Self {
            provider,
            limits: limits.validate()?,
        })
    }
    fn supports_kind(&self, kind: &str) -> bool {
        !kind.is_empty() && self.provider.supports_kind(kind)
    }
    /// One pure verdict per unique requested kind, in first-occurrence order.
    ///
    /// # Errors
    /// Returns an error if any requested kind is empty.
    pub fn check_kind_support<K: AsRef<str>>(
        &self,
        kinds: impl IntoIterator<Item = K>,
    ) -> Result<Vec<KindSupportVerdict>, BoundaryError> {
        let kinds: Vec<_> = kinds.into_iter().collect();
        if kinds.iter().any(|k| k.as_ref().is_empty()) {
            return Err(boundary_error(BoundaryCode::EmptyKind, "/kinds"));
        }
        Ok(openbindings_internal_interfaces::kind_support_verdicts(
            kinds.iter().map(AsRef::as_ref),
            |kind| self.supports_kind(kind),
        ))
    }
    /// Validate the advisory list against exact support; omission means nothing.
    ///
    /// # Errors
    /// Returns an error if the provider lists an empty kind or a kind it does not support.
    /// Duplicate supported kinds are preserved in provider order.
    pub fn list_supported_kinds(&self) -> Result<Vec<SupportedKind>, BoundaryError> {
        let kinds = self.provider.supported_kinds();
        if kinds.iter().any(|k| !self.supports_kind(&k.kind)) {
            return Err(boundary_error(BoundaryCode::AdvisoryList, ""));
        }
        Ok(kinds)
    }
    /// Construct one lazy attempt. Work begins only when an I/O operation is polled.
    pub fn invoke<'a>(
        &'a self,
        request: BindingRequest<'a>,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        self.invoke_with_control(request, &CancellationToken::new())
    }
    /// Caller control is observed; cancelling this invocation never cancels it or siblings.
    pub fn invoke_with_control<'a>(
        &'a self,
        request: BindingRequest<'a>,
        control: &CancellationToken,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        let shared = Shared::new(self.limits);
        let local = shared.control();
        let external = control.clone();
        let io = ProviderIo::new(shared.clone());
        let provider = &self.provider;
        let diagnostics = shared.clone();
        let provider_control = local.clone();
        let future = checked(
            move || {
                request.admit().map_err(|error| {
                    diagnostics.record_finding(error);
                    InvocationError::refused()
                })?;
                if !provider.supports_kind(&request.source.kind) {
                    return Err(InvocationError::refused());
                }
                Ok(provider.invoke(request, io, provider_control))
            },
            external,
            local,
            shared.clone(),
        );
        Invocation::new(shared, future)
    }
    /// Admit an advisory preflight, then ask the provider's hook when it has
    /// one; a provider without a hook makes no prediction. The future is Send
    /// when the provider is Sync and its Invocation future is Send.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. Provider refusals and malformed provider challenges also return an error.
    pub async fn preflight(
        &self,
        request: BindingRequest<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        self.preflight_with_control(request, &CancellationToken::new())
            .await
    }
    /// Advisory preflight with caller cancellation, isolated from sibling attempts.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. Provider refusals and malformed provider challenges also return an error.
    pub async fn preflight_with_control(
        &self,
        request: BindingRequest<'_>,
        control: &CancellationToken,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        preflight(&self.provider, self.limits, request, control).await
    }
}

// The binding and operation facades share checked admission, the scoped
// cancellation relay and the bounded relay of the provider hook's answer.
pub(crate) async fn preflight<P: BindingInvoker>(
    provider: &P,
    limits: InvocationLimits,
    request: BindingRequest<'_>,
    control: &CancellationToken,
) -> Result<Option<ContextChallenge>, InvocationError> {
    if control.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    request.admit().map_err(|_| InvocationError::refused())?;
    if !provider.supports_kind(&request.source.kind) {
        return Err(InvocationError::refused());
    }
    let local = CancellationToken::new();
    let _relay = cancellation_relay(control, &local);
    let mut cancelled = pin!(control.cancelled());
    let mut job = pin!(provider.preflight(request, local.clone()));
    let result = poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            local.cancel();
            return Poll::Ready(Err(InvocationError::cancelled()));
        }
        // A provider without a hook makes no prediction.
        let result = match job.as_mut().as_pin_mut() {
            Some(job) => job.poll(cx),
            None => Poll::Ready(Ok(())),
        };
        if control.is_cancelled() {
            local.cancel();
            Poll::Ready(Err(InvocationError::cancelled()))
        } else {
            // this branch alone has provider provenance.
            result.map(|r| {
                r.map_err(|e| {
                    e.admit_provider_code()
                        .unwrap_or_else(|_| InvocationError::execution_failed())
                })
            })
        }
    })
    .await;
    // The hook reports requirements as the challenge invocation would raise;
    // preflight returns them as its successful result, never as an invocation
    // CONTEXT_REQUIRED terminal (which carries a retry path).
    let result = match result {
        Ok(()) => Ok(None),
        Err(error) => error.context_required().map(Some).ok_or(error),
    };
    let limits = openbindings::JsonLimits {
        max_bytes: limits.max_error_bytes,
        ..Default::default()
    };
    let within_limit = match &result {
        Ok(Some(challenge)) => challenge.to_json_with_limits(limits).is_ok(),
        Err(error) => error.to_json_with_limits(limits).is_ok(),
        Ok(None) => true,
    };
    if within_limit {
        result
    } else {
        Err(InvocationError::execution_failed())
    }
}

// Keep the provider’s GAT projection outside this async state machine. This
// preserves conditional Send for providers borrowing non-static configuration.
pub(crate) async fn checked<F: Future<Output = Result<(), InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    external: CancellationToken,
    local: CancellationToken,
    diagnostics: std::sync::Arc<Shared>,
) -> Result<(), InvocationError> {
    if external.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let _relay = cancellation_relay(&external, &local);
    if external.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(external.cancelled());
    let mut job = pin!(start()?);
    poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            local.cancel();
            return Poll::Ready(Err(InvocationError::cancelled()));
        }
        let result = job.as_mut().poll(cx);
        if external.is_cancelled() {
            local.cancel();
            Poll::Ready(Err(InvocationError::cancelled()))
        } else {
            result.map(|r| {
                r.map_err(|e| {
                    e.admit_provider_code().unwrap_or_else(|finding| {
                        diagnostics.record_finding(finding);
                        InvocationError::execution_failed()
                    })
                })
            })
        }
    })
    .await
}

// Retain a scoped second registration so cancellation reaches synchronous
// cooperative provider/evaluator work while its current poll is still running.
// The ordinary cancelled() poll remains responsible for waking the driver.
fn cancellation_relay(
    external: &CancellationToken,
    local: &CancellationToken,
) -> crate::Cancellation {
    struct Cancel(CancellationToken);
    impl std::task::Wake for Cancel {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.cancel();
        }
        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.0.cancel();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(Cancel(local.clone())));
    let mut registration = external.cancelled();
    if std::pin::Pin::new(&mut registration)
        .poll(&mut std::task::Context::from_waker(&waker))
        .is_ready()
    {
        local.cancel();
    }
    registration
}

#[cfg(test)]
mod cancellation_ownership_tests {
    use super::*;
    use std::{
        sync::Arc,
        task::{Context, Wake, Waker},
    };

    struct Driver;
    #[allow(clippy::manual_noop_waker)] // The weak owner witnesses registration release.
    impl Wake for Driver {
        fn wake(self: Arc<Self>) {}
    }

    #[test]
    fn driver_and_relay_release_on_early_return_and_abandonment() {
        for phase in 0..3 {
            let external = CancellationToken::new();
            let local = CancellationToken::new();
            let owner = Arc::new(Driver);
            let weak = Arc::downgrade(&owner);
            let waker = Waker::from(owner);
            let mut future = Box::pin(checked(
                || {
                    if phase == 0 {
                        Err(InvocationError::refused())
                    } else {
                        Ok(std::future::poll_fn(|_| {
                            if phase == 1 {
                                Poll::Ready(Ok(()))
                            } else {
                                Poll::Pending
                            }
                        }))
                    }
                },
                external.clone(),
                local.clone(),
                Shared::new(InvocationLimits::default()),
            ));
            let result = future.as_mut().poll(&mut Context::from_waker(&waker));
            assert_eq!(result.is_ready(), phase < 2);
            drop(waker);
            if phase < 2 {
                assert!(weak.upgrade().is_none());
            }
            drop(future);
            assert!(weak.upgrade().is_none());
            external.cancel();
            assert!(
                !local.is_cancelled(),
                "finished attempt must not retain a relay"
            );
        }
    }
}

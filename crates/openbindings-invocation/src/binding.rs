//! Binding-kind provider seam and checked consumer entry point.
use crate::CancellationToken;
use crate::{
    BindingRequest, BoundaryError, ContextChallenge, Invocation, InvocationError, InvocationLimits,
    LimitError, ProviderIo, lifecycle::Shared,
};

use serde::Serialize;
use std::collections::HashSet;

/// Advisory kind description; omission conveys no negative support verdict.
#[derive(Clone, Debug, Serialize)]
pub struct SupportedKind {
    /// Exact opaque kind label.
    pub kind: String,
    /// Optional human-facing description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
/// Exact support for this provider’s invocation job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindSupportVerdict {
    /// Exact requested kind, unchanged.
    pub kind: String,
    /// Authoritative support for this job; not a guarantee every case is handled.
    pub supported: bool,
}
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
}

/// Optional advisory preparation for a binding provider. Invocation never calls
/// this hook. Providers may return local futures or futures that implement Send.
pub trait BindingPreflight: BindingInvoker {
    /// Borrowed advisory job; generic consumers can place bounds on this future.
    type Preflight<'a>: Future<Output = Result<Option<ContextChallenge>, InvocationError>>
    where
        Self: 'a;
    /// Predict context requirements without input/output, dispatch or approval
    /// spending. `Ok(None)` makes no prediction.
    fn preflight<'a>(
        &'a self,
        request: BindingRequest<'a>,
        control: CancellationToken,
    ) -> Self::Preflight<'a>;
}

macro_rules! forward { ($($wrapper:ty),+) => { $(
impl<P: BindingInvoker + ?Sized> BindingInvoker for $wrapper {
    type Invocation<'a> = P::Invocation<'a> where Self: 'a;
    fn supports_kind(&self, kind: &str) -> bool { (**self).supports_kind(kind) }
    fn supported_kinds(&self) -> Vec<SupportedKind> { (**self).supported_kinds() }
    fn invoke<'a>(&'a self, request: BindingRequest<'a>, io: ProviderIo, control: CancellationToken) -> Self::Invocation<'a> { (**self).invoke(request, io, control) }
}
impl<P: BindingPreflight + ?Sized> BindingPreflight for $wrapper {
    type Preflight<'a> = P::Preflight<'a> where Self: 'a;
    fn preflight<'a>(&'a self, request: BindingRequest<'a>, control: CancellationToken) -> Self::Preflight<'a> { (**self).preflight(request, control) }
}
)+ }; }
forward!(&P, std::sync::Arc<P>, std::rc::Rc<P>);

/// Reusable checked facade. No registry, operation selection, retries or executor.
pub struct Invoker<P> {
    pub(crate) provider: P,
    pub(crate) limits: InvocationLimits,
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
    pub fn check_kind_support<K: AsRef<str>>(
        &self,
        kinds: impl IntoIterator<Item = K>,
    ) -> Result<Vec<KindSupportVerdict>, BoundaryError> {
        let kinds: Vec<_> = kinds.into_iter().collect();
        if kinds.iter().any(|k| k.as_ref().is_empty()) {
            return Err(BoundaryError::new("empty-kind", "/kinds"));
        }
        let mut seen = HashSet::new();
        Ok(kinds
            .iter()
            .map(AsRef::as_ref)
            .filter(|k| seen.insert(*k))
            .map(|kind| KindSupportVerdict {
                kind: kind.into(),
                supported: self.supports_kind(kind),
            })
            .collect())
    }
    /// Validate the advisory list against exact support; omission means nothing.
    pub fn list_supported_kinds(&self) -> Result<Vec<SupportedKind>, BoundaryError> {
        let kinds = self.provider.supported_kinds();
        if kinds.iter().any(|k| !self.supports_kind(&k.kind)) {
            return Err(BoundaryError::new("advisory-list", ""));
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
    /// Admit advisory preflight and make no prediction. This default path needs
    /// only an invoke provider; use `preflight_with_provider` to opt into its hook.
    pub async fn preflight(
        &self,
        request: BindingRequest<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        self.preflight_with_control(request, &CancellationToken::new())
            .await
    }
    /// Advisory preflight with caller cancellation, isolated from sibling attempts.
    pub async fn preflight_with_control(
        &self,
        request: BindingRequest<'_>,
        control: &CancellationToken,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        self.check_preflight(request, control, |_| std::future::ready(Ok(None)))
            .await
    }
    /// Call the optional provider preflight hook after checked admission.
    /// The future is Send when the provider is Sync and its Preflight is Send.
    pub async fn preflight_with_provider(
        &self,
        request: BindingRequest<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError>
    where
        P: BindingPreflight,
    {
        self.preflight_with_provider_and_control(request, &CancellationToken::new())
            .await
    }
    /// Call the optional provider preflight hook with caller cancellation.
    pub async fn preflight_with_provider_and_control(
        &self,
        request: BindingRequest<'_>,
        control: &CancellationToken,
    ) -> Result<Option<ContextChallenge>, InvocationError>
    where
        P: BindingPreflight,
    {
        self.check_preflight(request, control, |local| {
            self.provider.preflight(request, local)
        })
        .await
    }
    async fn check_preflight<F>(
        &self,
        request: BindingRequest<'_>,
        control: &CancellationToken,
        start: impl FnOnce(CancellationToken) -> F,
    ) -> Result<Option<ContextChallenge>, InvocationError>
    where
        F: Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
    {
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        request.admit().map_err(|_| InvocationError::refused())?;
        if !self.provider.supports_kind(&request.source.kind) {
            return Err(InvocationError::refused());
        }
        let local = CancellationToken::new();
        let _relay = cancellation_relay(control, &local);
        let mut cancelled = pin!(control.cancelled());
        let mut job = pin!(start(local.clone()));
        let result = poll_fn(|cx| {
            if cancelled.as_mut().poll(cx).is_ready() {
                local.cancel();
                return Poll::Ready(Err(InvocationError::cancelled()));
            }
            let result = job.as_mut().poll(cx);
            if control.is_cancelled() {
                local.cancel();
                Poll::Ready(Err(InvocationError::cancelled()))
            } else {
                // RO-8: this branch alone has provider provenance.
                result.map(|r| {
                    r.map_err(|e| {
                        e.admit_provider_code()
                            .unwrap_or_else(|_| InvocationError::execution_failed())
                    })
                })
            }
        })
        .await;
        // Preflight reports requirements as its successful result, never as an
        // invocation CONTEXT_REQUIRED terminal (which carries a retry path).
        if result
            .as_ref()
            .err()
            .is_some_and(|e| e.code() == "CONTEXT_REQUIRED")
        {
            return Err(InvocationError::execution_failed());
        }
        let limits = openbindings::JsonLimits {
            max_bytes: self.limits.max_error_bytes,
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

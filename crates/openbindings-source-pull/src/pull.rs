use crate::{
    CancellationToken, ContextChallenge, PullLimits, PullRequest, PullResult, PullValidationError,
    budget::Attempt, validation,
};
use std::{fmt, future::Future};

/// Provider outcome separate from local carrier/relationship validation.
/// The publication prescribes no universal operational-error JSON envelope.
pub enum PullFailure<E> {
    /// The provider requires additional separately supplied context.
    ContextRequired(ContextChallenge),
    /// Provider-defined transport, observation, projection or admission inability.
    Operational(E),
    /// The provider cancelled the attempt.
    Cancelled,
}
impl<E> fmt::Debug for PullFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ContextRequired(_) => "ContextRequired { .. }",
            Self::Operational(_) => "Operational { .. }",
            Self::Cancelled => "Cancelled",
        })
    }
}
/// Explicit kind-owned source observation and projection; no source operation is invoked.
pub trait SourcePuller {
    /// Native operational error, including inability to produce a truthful report.
    type Error;
    /// Borrowed finite work, without an async runtime, Send or Sync requirement.
    type Pull<'a>: Future<Output = Result<PullResult, PullFailure<Self::Error>>>
    where
        Self: 'a;
    /// Observe the selected source and return additions/reports against the original document.
    /// Context acquisition, retries and applying proposals belong to the caller.
    fn handle_pull_source(
        &self,
        request: PullRequest,
        control: CancellationToken,
    ) -> Self::Pull<'_>;
}
macro_rules! forward {
    ($($wrapper:ty),+) => { $(
        impl<P: SourcePuller + ?Sized> SourcePuller for $wrapper {
            type Error = P::Error;
            type Pull<'a> = P::Pull<'a> where Self: 'a;
            fn handle_pull_source(&self, request: PullRequest, control: CancellationToken) -> Self::Pull<'_> {
                (**self).handle_pull_source(request, control)
            }
        }
    )+ };
}
forward!(&P, Box<P>, std::sync::Arc<P>, std::rc::Rc<P>);

/// Checked caller failure. Native provider errors remain distinct from local checks.
pub enum PullError<E> {
    /// The provider reported a challenge or operational inability.
    Provider(PullFailure<E>),
    /// Request handoff, result relationships or prospective conformance failed.
    Validation(PullValidationError),
    /// Local or provider cancellation; the original document remains unchanged.
    Cancelled,
}
impl<E> fmt::Debug for PullError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(e) => f.debug_tuple("Provider").field(e).finish(),
            Self::Validation(e) => f.debug_tuple("Validation").field(e).finish(),
            Self::Cancelled => f.write_str("Cancelled"),
        }
    }
}
impl<E> fmt::Display for PullError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source pull failed: {self:?}")
    }
}
impl<E> std::error::Error for PullError<E> {}
impl<E> From<PullValidationError> for PullError<E> {
    fn from(value: PullValidationError) -> Self {
        match value {
            PullValidationError::Cancelled => Self::Cancelled,
            other => Self::Validation(other),
        }
    }
}

// Keep the async state generic over the concrete future, rather than a provider
// GAT projection. This preserves conditional Send for providers borrowing local
// state without introducing a higher-ranked 'static requirement.
async fn checked_pull<E, F: Future<Output = Result<PullResult, PullFailure<E>>>>(
    limits: PullLimits,
    request: PullRequest,
    control: CancellationToken,
    start: impl FnOnce(PullRequest, CancellationToken) -> F,
) -> Result<PullResult, PullError<E>> {
    let mut attempt = Attempt::new(limits, &control);
    attempt.request(&request)?;
    let original = request.clone();
    let response = openbindings_internal_cancellation::run_attempt(control.clone(), |local| {
        start(request, local)
    })
    .await
    .ok_or(PullError::Cancelled)?;
    attempt.check()?;
    let result = match response {
        Ok(result) => result,
        Err(PullFailure::Cancelled) => return Err(PullError::Cancelled),
        Err(PullFailure::ContextRequired(challenge)) => {
            attempt.response(challenge.value().value().text().len())?;
            return Err(PullError::Provider(PullFailure::ContextRequired(challenge)));
        }
        Err(error) => return Err(PullError::Provider(error)),
    };
    validation::validate(&result, &original, &mut attempt)?;
    attempt.check()?;
    Ok(result)
}

/// Checked source pulling. Import this trait to call a [`SourcePuller`].
/// Its blanket implementation owns admission, cancellation and result checks;
/// explicit `handle_pull_source` calls bypass those checks.
pub trait SourcePullerExt: SourcePuller {
    /// Lazily pull and validate the result against the original request.
    fn pull_source(
        &self,
        request: PullRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<PullResult, PullError<Self::Error>>>;
    /// Pull with caller-selected limits; all checks remain lazy.
    fn pull_source_with_limits(
        &self,
        request: PullRequest,
        control: CancellationToken,
        limits: PullLimits,
    ) -> impl Future<Output = Result<PullResult, PullError<Self::Error>>>;
}
impl<P: SourcePuller + ?Sized> SourcePullerExt for P {
    fn pull_source<'a>(
        &'a self,
        request: PullRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<PullResult, PullError<P::Error>>> {
        checked_pull(
            PullLimits::default(),
            request,
            control,
            |request, local| -> P::Pull<'a> { self.handle_pull_source(request, local) },
        )
    }
    fn pull_source_with_limits<'a>(
        &'a self,
        request: PullRequest,
        control: CancellationToken,
        limits: PullLimits,
    ) -> impl Future<Output = Result<PullResult, PullError<P::Error>>> {
        checked_pull(limits, request, control, |request, local| -> P::Pull<'a> {
            self.handle_pull_source(request, local)
        })
    }
}

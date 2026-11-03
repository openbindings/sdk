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
    /// Borrowed finite work, without an executor, Send or Sync requirement.
    type Pull<'a>: Future<Output = Result<PullResult, PullFailure<Self::Error>>>
    where
        Self: 'a;
    /// Observe the selected source and return additions/reports against the original document.
    /// Context acquisition, retries and applying proposals belong to the caller.
    fn pull_source(&self, request: PullRequest, control: CancellationToken) -> Self::Pull<'_>;
}
macro_rules! forward {
    ($($wrapper:ty),+) => { $(
        impl<P: SourcePuller + ?Sized> SourcePuller for $wrapper {
            type Error = P::Error;
            type Pull<'a> = P::Pull<'a> where Self: 'a;
            fn pull_source(&self, request: PullRequest, control: CancellationToken) -> Self::Pull<'_> {
                (**self).pull_source(request, control)
            }
        }
    )+ };
}
forward!(&P, Box<P>, std::sync::Arc<P>, std::rc::Rc<P>);

/// Checked caller failure. Native provider errors remain distinct from local checks.
pub enum ClientError<E> {
    /// The provider reported a challenge or operational inability.
    Provider(PullFailure<E>),
    /// Request handoff, result relationships or prospective conformance failed.
    Validation(PullValidationError),
    /// Local or provider cancellation; the original document remains unchanged.
    Cancelled,
}
impl<E> fmt::Debug for ClientError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(e) => f.debug_tuple("Provider").field(e).finish(),
            Self::Validation(e) => f.debug_tuple("Validation").field(e).finish(),
            Self::Cancelled => f.write_str("Cancelled"),
        }
    }
}
impl<E> fmt::Display for ClientError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source pull failed: {self:?}")
    }
}
impl<E> std::error::Error for ClientError<E> {}
impl<E> From<PullValidationError> for ClientError<E> {
    fn from(value: PullValidationError) -> Self {
        match value {
            PullValidationError::Cancelled => Self::Cancelled,
            other => Self::Validation(other),
        }
    }
}

/// Lazy checked caller. Pulling neither applies additions/proposals nor collects
/// context, checks the source, executes source operations or retries implicitly.
pub struct PullClient<P> {
    provider: P,
    limits: PullLimits,
}
impl<P> fmt::Debug for PullClient<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PullClient")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
impl<P: SourcePuller> PullClient<P> {
    /// Retain an explicit provider with default finite local limits.
    pub fn new(provider: P) -> Self {
        Self::with_limits(provider, PullLimits::default())
    }
    /// Retain a provider and caller-selected finite local limits.
    pub fn with_limits(provider: P, limits: PullLimits) -> Self {
        Self { provider, limits }
    }
    /// Lazily pull once, then check the result against the exact original request.
    /// Dropping pending work cancels its child token before dropping provider work.
    /// A missing source or exhausted request policy prevents provider construction.
    /// Successful handoff establishes prospective core conformance only, not the
    /// truth of kind-owned claims. The returned report is still producer evidence.
    pub async fn pull_source(
        &self,
        request: PullRequest,
        control: CancellationToken,
    ) -> Result<PullResult, ClientError<P::Error>> {
        let mut attempt = Attempt::new(self.limits, &control);
        attempt.request(&request)?;
        let original = request.clone();
        let response = openbindings_internal_cancellation::run_attempt(control.clone(), |local| {
            self.provider.pull_source(request, local)
        })
        .await
        .ok_or(ClientError::Cancelled)?;
        attempt.check()?;
        let result = match response {
            Ok(result) => result,
            Err(PullFailure::Cancelled) => return Err(ClientError::Cancelled),
            Err(PullFailure::ContextRequired(challenge)) => {
                attempt.response(challenge.value().value().text().len())?;
                return Err(ClientError::Provider(PullFailure::ContextRequired(
                    challenge,
                )));
            }
            Err(error) => return Err(ClientError::Provider(error)),
        };
        validation::validate(&result, &original, &mut attempt)?;
        attempt.check()?;
        Ok(result)
    }
}

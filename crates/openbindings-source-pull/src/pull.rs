use crate::{
    CancellationToken, ContextChallenge, PullLimits, PullRequest, PullResult, PullValidationError,
    budget::Attempt, validation,
};
use openbindings::ConformantDocument;
use std::{fmt, future::Future};

/// Provider outcome separate from local carrier/relationship validation.
/// The publication prescribes no universal operational-error JSON envelope.
///
/// New failure categories may be added. Handle unknown categories by refusing
/// the attempt or propagating the original error. Exhaustive downstream matches
/// are intentionally rejected:
///
/// ```compile_fail,E0004
/// use openbindings_source_pull::PullFailure;
/// fn classify(error: PullFailure<()>) {
///     match error {
///         PullFailure::ContextRequired(_) => (),
///         PullFailure::Operational(_) => (),
///         PullFailure::Cancelled => (),
///     }
/// }
/// ```
#[non_exhaustive]
pub enum PullFailure<E> {
    /// The provider requires additional separately supplied context.
    ContextRequired(ContextChallenge),
    /// Provider-defined transport, observation, projection or admission inability.
    Operational(E),
    /// The provider cancelled the attempt.
    Cancelled,
}
impl<E> PullFailure<E> {
    /// Transform only the native operational error, preserving context and cancellation.
    #[inline]
    pub fn map_error<F>(self, map: impl FnOnce(E) -> F) -> PullFailure<F> {
        match self {
            Self::Operational(error) => PullFailure::Operational(map(error)),
            Self::ContextRequired(challenge) => PullFailure::ContextRequired(challenge),
            Self::Cancelled => PullFailure::Cancelled,
        }
    }
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
impl<E: fmt::Display> fmt::Display for PullFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextRequired(_) => f.write_str("source pulling requires context"),
            Self::Operational(_) => f.write_str("source pulling could not complete"),
            Self::Cancelled => f.write_str("source pulling was cancelled"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PullFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Operational(error) => Some(error),
            _ => None,
        }
    }
}
/// Explicit kind-owned source observation and projection; no source operation is invoked.
/// See [writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md)
/// for implementation, report obligations and checked calls.
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
///
/// New failure categories may be added. Handle unknown categories by refusing
/// the attempt or propagating the original error. Exhaustive downstream matches
/// are intentionally rejected:
///
/// ```compile_fail,E0004
/// use openbindings_source_pull::PullError;
/// fn classify(error: PullError<()>) {
///     match error {
///         PullError::Provider(_) => (),
///         PullError::Validation(_) => (),
///         PullError::Cancelled => (),
///     }
/// }
/// ```
#[non_exhaustive]
pub enum PullError<E> {
    /// The provider reported a challenge or operational inability.
    Provider(PullFailure<E>),
    /// Request handoff, result relationships or prospective conformance failed.
    Validation(PullValidationError),
    /// Local or provider cancellation; the original document remains unchanged.
    Cancelled,
}
impl<E> PullError<E> {
    /// Transform only the native provider error, preserving all local failures.
    /// The closure is not called for context challenges or cancellation.
    #[inline]
    pub fn map_error<F>(self, map: impl FnOnce(E) -> F) -> PullError<F> {
        match self {
            Self::Provider(PullFailure::Operational(error)) => {
                PullError::Provider(PullFailure::Operational(map(error)))
            }
            Self::Provider(PullFailure::ContextRequired(challenge)) => {
                PullError::Provider(PullFailure::ContextRequired(challenge))
            }
            Self::Provider(PullFailure::Cancelled) => PullError::Provider(PullFailure::Cancelled),
            Self::Validation(error) => PullError::Validation(error),
            Self::Cancelled => PullError::Cancelled,
        }
    }
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
impl<E: fmt::Display> fmt::Display for PullError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(_) => f.write_str("source pull provider failed"),
            Self::Validation(_) => f.write_str("source pull validation failed"),
            Self::Cancelled => f.write_str("source pull was cancelled"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PullError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::Cancelled => None,
        }
    }
}
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
) -> Result<(PullResult, ConformantDocument), PullError<E>> {
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
    let prospective = validation::validate(&result, &original, &mut attempt)?;
    attempt.check()?;
    Ok((result, prospective))
}

/// Checked source pulling. Import this trait to call a [`SourcePuller`].
/// Its blanket implementation owns admission, cancellation and result checks;
/// explicit `handle_pull_source` calls bypass those checks.
pub trait SourcePullerExt: SourcePuller {
    /// Lazily pull and validate the result against the original request.
    /// Returns the exact result and the conformant snapshot containing all additions.
    /// Stale proposals are not applied; the caller still chooses what to adopt.
    fn pull_source(
        &self,
        request: PullRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<(PullResult, ConformantDocument), PullError<Self::Error>>>;
    /// Pull with caller-selected limits; all checks remain lazy.
    /// Returns the same result and all-additions snapshot as [`Self::pull_source`].
    fn pull_source_with_limits(
        &self,
        request: PullRequest,
        control: CancellationToken,
        limits: PullLimits,
    ) -> impl Future<Output = Result<(PullResult, ConformantDocument), PullError<Self::Error>>>;
}
impl<P: SourcePuller + ?Sized> SourcePullerExt for P {
    fn pull_source<'a>(
        &'a self,
        request: PullRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<(PullResult, ConformantDocument), PullError<P::Error>>> {
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
    ) -> impl Future<Output = Result<(PullResult, ConformantDocument), PullError<P::Error>>> {
        checked_pull(limits, request, control, |request, local| -> P::Pull<'a> {
            self.handle_pull_source(request, local)
        })
    }
}

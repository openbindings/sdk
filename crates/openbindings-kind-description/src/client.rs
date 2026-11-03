use crate::{
    CheckFailure, CheckSourceRequest, CheckedSource, ClientError, DescribeKindRequest,
    KindDescription, KindSummary,
};
use openbindings_internal_cancellation::CancellationToken;
use std::{fmt, future::Future};

/// Provider of kind listings and descriptions. Queries perform no operation under a kind.
pub trait KindDescriptor {
    /// Provider-defined query failure; the interface does not prescribe a query error record.
    type Error;
    /// Borrowed listing work, without an implicit executor.
    type List<'a>: Future<Output = Result<Vec<KindSummary>, Self::Error>>
    where
        Self: 'a;
    /// Borrowed description work, separate from listing and source checking.
    type Describe<'a>: Future<Output = Result<KindDescription, Self::Error>>
    where
        Self: 'a;
    /// Return operation-specific claims for the kinds this implementation describes.
    fn list_kinds(&self, control: CancellationToken) -> Self::List<'_>;
    /// Describe the exact requested kind, without executing under it.
    fn describe_kind(
        &self,
        request: DescribeKindRequest,
        control: CancellationToken,
    ) -> Self::Describe<'_>;
}
/// Provider that checks proposed source entries under their kinds.
pub trait SourceChecker {
    /// Native transport, decoding or admission failure, distinct from a kind rejection.
    type Error;
    /// Borrowed check work. The kind owns how it reads source data.
    type Check<'a>: Future<Output = Result<CheckedSource, CheckFailure<Self::Error>>>
    where
        Self: 'a;
    /// Check the proposal without changing a document or performing source operations.
    /// Any check execution is disclosed by the corresponding kind description;
    /// the application chooses whether to call it.
    fn check_source(
        &self,
        request: CheckSourceRequest,
        control: CancellationToken,
    ) -> Self::Check<'_>;
}

macro_rules! forward {
    ($($wrapper:ty),+) => { $(
        impl<P: KindDescriptor + ?Sized> KindDescriptor for $wrapper {
            type Error = P::Error;
            type List<'a> = P::List<'a> where Self: 'a;
            type Describe<'a> = P::Describe<'a> where Self: 'a;
            fn list_kinds(&self, control: CancellationToken) -> Self::List<'_> {
                (**self).list_kinds(control)
            }
            fn describe_kind(&self, request: DescribeKindRequest, control: CancellationToken) -> Self::Describe<'_> {
                (**self).describe_kind(request, control)
            }
        }
        impl<P: SourceChecker + ?Sized> SourceChecker for $wrapper {
            type Error = P::Error;
            type Check<'a> = P::Check<'a> where Self: 'a;
            fn check_source(&self, request: CheckSourceRequest, control: CancellationToken) -> Self::Check<'_> {
                (**self).check_source(request, control)
            }
        }
    )+ };
}
forward!(&P, Box<P>, std::sync::Arc<P>, std::rc::Rc<P>);

/// Finite SDK carrier handoff policy, not limits on a kind's external implementation.
#[derive(Clone, Copy, Debug)]
pub struct DescriptionLimits {
    /// Maximum original JSON bytes in a request; default 32 MiB.
    pub max_request_bytes: usize,
    /// Maximum response JSON bytes, summed over a listing; default 32 MiB.
    /// Native provider errors are not JSON carriers and are not serialized here.
    pub max_response_bytes: usize,
    /// Maximum entries returned by listKinds; default 100,000.
    pub max_listed_kinds: usize,
}
impl Default for DescriptionLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 32 * 1024 * 1024,
            max_response_bytes: 32 * 1024 * 1024,
            max_listed_kinds: 100_000,
        }
    }
}

async fn call<T, E, F: Future<Output = Result<T, E>>>(
    external: CancellationToken,
    start: impl FnOnce(CancellationToken) -> F,
) -> Result<T, ClientError<E>> {
    openbindings_internal_cancellation::run_attempt(external, start)
        .await
        .ok_or(ClientError::Cancelled)?
        .map_err(ClientError::Provider)
}

fn bound<E>(bytes: usize, maximum: usize, resource: &'static str) -> Result<(), ClientError<E>> {
    if bytes > maximum {
        Err(ClientError::LimitExceeded(resource))
    } else {
        Ok(())
    }
}

/// Checked caller for the two kind capability queries. It performs no source check.
pub struct KindClient<P> {
    provider: P,
    limits: DescriptionLimits,
}
impl<P> fmt::Debug for KindClient<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindClient")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
impl<P: KindDescriptor> KindClient<P> {
    /// Retain an explicit provider with default carrier limits.
    pub fn new(provider: P) -> Self {
        Self::with_limits(provider, DescriptionLimits::default())
    }
    /// Retain an explicit provider and caller-selected carrier limits.
    pub fn with_limits(provider: P, limits: DescriptionLimits) -> Self {
        Self { provider, limits }
    }
    /// Lazily list kinds. Cancellation before polling does not invoke the provider.
    pub fn list_kinds<'a>(
        &'a self,
        control: CancellationToken,
    ) -> impl Future<Output = Result<Vec<KindSummary>, ClientError<P::Error>>> {
        list(self.limits, control, |local| -> P::List<'a> {
            self.provider.list_kinds(local)
        })
    }
    /// Lazily describe one exact kind and check the response's kind identity.
    pub fn describe_kind<'a>(
        &'a self,
        request: DescribeKindRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<KindDescription, ClientError<P::Error>>> {
        describe(
            self.limits,
            request,
            control,
            |request, local| -> P::Describe<'a> { self.provider.describe_kind(request, local) },
        )
    }
}

/// Checked caller for an explicitly requested source check. No description query,
/// execution approval, context collection or retry is performed automatically.
pub struct SourceCheckClient<P> {
    provider: P,
    limits: DescriptionLimits,
}
impl<P> fmt::Debug for SourceCheckClient<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceCheckClient")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
impl<P: SourceChecker> SourceCheckClient<P> {
    /// Retain an explicit source checker with default carrier limits.
    pub fn new(provider: P) -> Self {
        Self::with_limits(provider, DescriptionLimits::default())
    }
    /// Retain an explicit source checker and caller-selected carrier limits.
    pub fn with_limits(provider: P, limits: DescriptionLimits) -> Self {
        Self { provider, limits }
    }
    /// Lazily check one proposal, keeping context separate from the returned source.
    pub fn check_source<'a>(
        &'a self,
        request: CheckSourceRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<CheckedSource, ClientError<CheckFailure<P::Error>>>> {
        check(
            self.limits,
            request,
            control,
            |request, local| -> P::Check<'a> { self.provider.check_source(request, local) },
        )
    }
}

// Instantiate provider GATs before constructing async state. Concrete future
// parameters preserve conditional Send for non-static borrowed providers.
async fn list<E, F: Future<Output = Result<Vec<KindSummary>, E>>>(
    limits: DescriptionLimits,
    control: CancellationToken,
    start: impl FnOnce(CancellationToken) -> F,
) -> Result<Vec<KindSummary>, ClientError<E>> {
    let values = call(control.clone(), start).await?;
    bound(values.len(), limits.max_listed_kinds, "listed-kinds")?;
    let mut bytes = 2usize;
    for (index, value) in values.iter().enumerate() {
        if control.is_cancelled() {
            return Err(ClientError::Cancelled);
        }
        bytes = bytes
            .saturating_add(usize::from(index != 0))
            .saturating_add(value.value().value().text().len());
        bound(bytes, limits.max_response_bytes, "response-bytes")?;
    }
    bound(bytes, limits.max_response_bytes, "response-bytes")?;
    if control.is_cancelled() {
        return Err(ClientError::Cancelled);
    }
    Ok(values)
}
async fn describe<E, F: Future<Output = Result<KindDescription, E>>>(
    limits: DescriptionLimits,
    request: DescribeKindRequest,
    control: CancellationToken,
    start: impl FnOnce(DescribeKindRequest, CancellationToken) -> F,
) -> Result<KindDescription, ClientError<E>> {
    if control.is_cancelled() {
        return Err(ClientError::Cancelled);
    }
    bound(
        request.value().value().text().len(),
        limits.max_request_bytes,
        "request-bytes",
    )?;
    let expected = request
        .value()
        .value()
        .get("kind")
        .expect("admitted kind")
        .to_owned();
    let value = call(control.clone(), |local| start(request, local)).await?;
    if control.is_cancelled() {
        return Err(ClientError::Cancelled);
    }
    bound(
        value.value().value().text().len(),
        limits.max_response_bytes,
        "response-bytes",
    )?;
    if value.kind() != expected.view().as_string().expect("kind") {
        return Err(ClientError::KindMismatch);
    }
    Ok(value)
}
async fn check<E, F: Future<Output = Result<CheckedSource, CheckFailure<E>>>>(
    limits: DescriptionLimits,
    request: CheckSourceRequest,
    control: CancellationToken,
    start: impl FnOnce(CheckSourceRequest, CancellationToken) -> F,
) -> Result<CheckedSource, ClientError<CheckFailure<E>>> {
    if control.is_cancelled() {
        return Err(ClientError::Cancelled);
    }
    bound(
        request.value().value().text().len(),
        limits.max_request_bytes,
        "request-bytes",
    )?;
    let expected = request
        .value()
        .value()
        .get("kind")
        .expect("admitted kind")
        .to_owned();
    let value = match call(control.clone(), |local| start(request, local)).await {
        Err(ClientError::Provider(CheckFailure::Cancelled)) => {
            return Err(ClientError::Cancelled);
        }
        Err(ClientError::Provider(CheckFailure::Rejected(rejection))) => {
            bound(
                rejection.to_json().value().text().len(),
                limits.max_response_bytes,
                "response-bytes",
            )?;
            return Err(ClientError::Provider(CheckFailure::Rejected(rejection)));
        }
        Err(ClientError::Provider(CheckFailure::ContextRequired(challenge))) => {
            bound(
                challenge.value().value().text().len(),
                limits.max_response_bytes,
                "response-bytes",
            )?;
            return Err(ClientError::Provider(CheckFailure::ContextRequired(
                challenge,
            )));
        }
        other => other?,
    };
    if control.is_cancelled() {
        return Err(ClientError::Cancelled);
    }
    bound(
        value.value().value().text().len(),
        limits.max_response_bytes,
        "response-bytes",
    )?;
    if value.kind() != expected.view().as_string().expect("kind") {
        return Err(ClientError::KindMismatch);
    }
    Ok(value)
}

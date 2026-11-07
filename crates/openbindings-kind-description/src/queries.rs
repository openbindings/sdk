use crate::{
    CheckFailure, CheckSourceRequest, CheckedSource, DescribeKindRequest, KindDescription,
    KindError, KindSummary,
};
use openbindings_internal_cancellation::CancellationToken;
use std::future::Future;

/// Provider of kind listings and descriptions. Queries perform no operation under a kind.
/// See [writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md)
/// for a complete implementation and support disclosures.
pub trait KindDescriptor {
    /// Provider-defined query failure; the interface does not prescribe a query error record.
    type Error;
    /// Borrowed listing work, without an implicit async runtime.
    type List<'a>: Future<Output = Result<Vec<KindSummary>, Self::Error>>
    where
        Self: 'a;
    /// Borrowed description work, separate from listing and source checking.
    type Describe<'a>: Future<Output = Result<KindDescription, Self::Error>>
    where
        Self: 'a;
    /// Return operation-specific claims for the kinds this implementation describes.
    fn handle_list_kinds(&self, control: CancellationToken) -> Self::List<'_>;
    /// Describe the exact requested kind, without executing under it.
    fn handle_describe_kind(
        &self,
        request: DescribeKindRequest,
        control: CancellationToken,
    ) -> Self::Describe<'_>;
}
/// Provider that checks proposed source entries under their kinds.
/// See [writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md)
/// for source-data ownership, context challenges and checked calls.
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
    fn handle_check_source(
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
            fn handle_list_kinds(&self, control: CancellationToken) -> Self::List<'_> {
                (**self).handle_list_kinds(control)
            }
            fn handle_describe_kind(&self, request: DescribeKindRequest, control: CancellationToken) -> Self::Describe<'_> {
                (**self).handle_describe_kind(request, control)
            }
        }
        impl<P: SourceChecker + ?Sized> SourceChecker for $wrapper {
            type Error = P::Error;
            type Check<'a> = P::Check<'a> where Self: 'a;
            fn handle_check_source(&self, request: CheckSourceRequest, control: CancellationToken) -> Self::Check<'_> {
                (**self).handle_check_source(request, control)
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
) -> Result<T, KindError<E>> {
    openbindings_internal_cancellation::run_attempt(external, start)
        .await
        .ok_or(KindError::Cancelled)?
        .map_err(KindError::Provider)
}

fn bound<E>(bytes: usize, maximum: usize, resource: &'static str) -> Result<(), KindError<E>> {
    if bytes > maximum {
        Err(KindError::LimitExceeded(resource))
    } else {
        Ok(())
    }
}

// Instantiate provider GATs before constructing async state. Concrete future
// parameters preserve conditional Send for non-static borrowed providers.
async fn list<E, F: Future<Output = Result<Vec<KindSummary>, E>>>(
    limits: DescriptionLimits,
    control: CancellationToken,
    start: impl FnOnce(CancellationToken) -> F,
) -> Result<Vec<KindSummary>, KindError<E>> {
    let values = call(control.clone(), start).await?;
    bound(values.len(), limits.max_listed_kinds, "listed-kinds")?;
    let mut bytes = 2usize;
    for (index, value) in values.iter().enumerate() {
        if control.is_cancelled() {
            return Err(KindError::Cancelled);
        }
        bytes = bytes
            .saturating_add(usize::from(index != 0))
            .saturating_add(value.value().value().text().len());
        bound(bytes, limits.max_response_bytes, "response-bytes")?;
    }
    bound(bytes, limits.max_response_bytes, "response-bytes")?;
    if control.is_cancelled() {
        return Err(KindError::Cancelled);
    }
    Ok(values)
}
async fn describe<E, F: Future<Output = Result<KindDescription, E>>>(
    limits: DescriptionLimits,
    request: DescribeKindRequest,
    control: CancellationToken,
    start: impl FnOnce(DescribeKindRequest, CancellationToken) -> F,
) -> Result<KindDescription, KindError<E>> {
    if control.is_cancelled() {
        return Err(KindError::Cancelled);
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
        return Err(KindError::Cancelled);
    }
    bound(
        value.value().value().text().len(),
        limits.max_response_bytes,
        "response-bytes",
    )?;
    if value.kind() != expected.view().as_string().expect("kind") {
        return Err(KindError::KindMismatch);
    }
    Ok(value)
}
async fn check<E, F: Future<Output = Result<CheckedSource, CheckFailure<E>>>>(
    limits: DescriptionLimits,
    request: CheckSourceRequest,
    control: CancellationToken,
    start: impl FnOnce(CheckSourceRequest, CancellationToken) -> F,
) -> Result<CheckedSource, KindError<CheckFailure<E>>> {
    if control.is_cancelled() {
        return Err(KindError::Cancelled);
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
        Err(KindError::Provider(CheckFailure::Cancelled)) => {
            return Err(KindError::Cancelled);
        }
        Err(KindError::Provider(CheckFailure::Rejected(rejection))) => {
            bound(
                rejection.to_json().value().text().len(),
                limits.max_response_bytes,
                "response-bytes",
            )?;
            return Err(KindError::Provider(CheckFailure::Rejected(rejection)));
        }
        Err(KindError::Provider(CheckFailure::ContextRequired(challenge))) => {
            bound(
                challenge.value().value().text().len(),
                limits.max_response_bytes,
                "response-bytes",
            )?;
            return Err(KindError::Provider(CheckFailure::ContextRequired(
                challenge,
            )));
        }
        other => other?,
    };
    if control.is_cancelled() {
        return Err(KindError::Cancelled);
    }
    bound(
        value.value().value().text().len(),
        limits.max_response_bytes,
        "response-bytes",
    )?;
    if value.kind() != expected.view().as_string().expect("kind") {
        return Err(KindError::KindMismatch);
    }
    Ok(value)
}

/// Checked calls. Import this trait to call a [`KindDescriptor`].
/// This blanket implementation owns checks; explicit `handle_*` calls bypass them.
pub trait KindDescriptorExt: KindDescriptor {
    /// Call lazily with the default caller limits.
    fn list_kinds(
        &self,
        control: CancellationToken,
    ) -> impl Future<Output = Result<Vec<KindSummary>, KindError<Self::Error>>>;
    /// Call lazily with caller-selected limits.
    fn list_kinds_with_limits(
        &self,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<Vec<KindSummary>, KindError<Self::Error>>>;
    /// Call lazily with the default caller limits.
    fn describe_kind(
        &self,
        request: DescribeKindRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<KindDescription, KindError<Self::Error>>>;
    /// Call lazily with caller-selected limits.
    fn describe_kind_with_limits(
        &self,
        request: DescribeKindRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<KindDescription, KindError<Self::Error>>>;
}
impl<P: KindDescriptor + ?Sized> KindDescriptorExt for P {
    fn list_kinds<'a>(
        &'a self,
        control: CancellationToken,
    ) -> impl Future<Output = Result<Vec<KindSummary>, KindError<P::Error>>> {
        list(
            DescriptionLimits::default(),
            control,
            |local| -> P::List<'a> { self.handle_list_kinds(local) },
        )
    }
    fn list_kinds_with_limits<'a>(
        &'a self,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<Vec<KindSummary>, KindError<P::Error>>> {
        list(limits, control, |local| -> P::List<'a> {
            self.handle_list_kinds(local)
        })
    }
    fn describe_kind<'a>(
        &'a self,
        request: DescribeKindRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<KindDescription, KindError<P::Error>>> {
        describe(
            DescriptionLimits::default(),
            request,
            control,
            |request, local| -> P::Describe<'a> { self.handle_describe_kind(request, local) },
        )
    }
    fn describe_kind_with_limits<'a>(
        &'a self,
        request: DescribeKindRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<KindDescription, KindError<P::Error>>> {
        describe(
            limits,
            request,
            control,
            |request, local| -> P::Describe<'a> { self.handle_describe_kind(request, local) },
        )
    }
}

/// Checked calls. Import this trait to call a [`SourceChecker`].
/// This blanket implementation owns checks; explicit `handle_*` calls bypass them.
pub trait SourceCheckerExt: SourceChecker {
    /// Call lazily with the default caller limits.
    fn check_source(
        &self,
        request: CheckSourceRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<CheckedSource, KindError<CheckFailure<Self::Error>>>>;
    /// Call lazily with caller-selected limits.
    fn check_source_with_limits(
        &self,
        request: CheckSourceRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<CheckedSource, KindError<CheckFailure<Self::Error>>>>;
}
impl<P: SourceChecker + ?Sized> SourceCheckerExt for P {
    fn check_source<'a>(
        &'a self,
        request: CheckSourceRequest,
        control: CancellationToken,
    ) -> impl Future<Output = Result<CheckedSource, KindError<CheckFailure<P::Error>>>> {
        check(
            DescriptionLimits::default(),
            request,
            control,
            |request, local| -> P::Check<'a> { self.handle_check_source(request, local) },
        )
    }
    fn check_source_with_limits<'a>(
        &'a self,
        request: CheckSourceRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> impl Future<Output = Result<CheckedSource, KindError<CheckFailure<P::Error>>>> {
        check(limits, request, control, |request, local| -> P::Check<'a> {
            self.handle_check_source(request, local)
        })
    }
}

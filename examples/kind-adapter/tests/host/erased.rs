//! The shims. The SDK's role traits carry associated future types and, for three of
//! them, an associated error, so none can be a trait object. Each trait here is the
//! object-safe twin of one role, with one blanket implementation over every adapter.
use openbindings::ConformantDocument;
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, ContextChallenge, InvocationError,
    InvocationSupport, ProviderIo,
};
use openbindings_kind_description::{
    CheckFailure, CheckSourceRequest, CheckedSource, DescribeKindRequest, DescriptionLimits,
    KindDescription, KindDescriptor, KindDescriptorExt, KindError, KindSummary, SourceChecker,
    SourceCheckerExt,
};
use openbindings_source_pull::{
    PullError, PullLimits, PullRequest, PullResult, SourcePuller, SourcePullerExt,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// This host chooses local futures. A thread-spawning host needs a Send twin
/// and Send bounds on every forwarded associated future.
pub type Work<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// What the host needs of a kind's native error. The role traits ask nothing of it.
pub trait NativeError: Error + Send + Sync + 'static {}
impl<E: Error + Send + Sync + 'static> NativeError for E {}

/// Any kind's native error behind one type.
///
/// `Box<dyn Error>` does not implement `Error`, so `PullError<Box<dyn Error>>` has no
/// `source()`. This newtype is an `Error` whose source is the adapter's own value.
/// Its `Display` is fixed text; the adapter's wording is reached only through `source()`.
pub struct KindFault(Box<dyn Error + Send + Sync + 'static>);
impl KindFault {
    /// Keep an adapter's own error for the host's error chain.
    pub fn new(native: impl NativeError) -> Self {
        Self(Box::new(native))
    }
}
impl fmt::Debug for KindFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KindFault { .. }")
    }
}
impl fmt::Display for KindFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the kind's adapter reported a failure of its own")
    }
}
impl Error for KindFault {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.0)
    }
}

/// `KindDescriptor`, checked, with the native error erased.
pub trait DynDescriptor {
    fn list<'a>(
        &'a self,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<Vec<KindSummary>, KindError<KindFault>>>;
    fn describe<'a>(
        &'a self,
        request: DescribeKindRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<KindDescription, KindError<KindFault>>>;
}
impl<K> DynDescriptor for K
where
    K: KindDescriptor,
    K::Error: NativeError,
{
    fn list<'a>(
        &'a self,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<Vec<KindSummary>, KindError<KindFault>>> {
        Box::pin(async move {
            self.list_kinds_with_limits(control, limits)
                .await
                .map_err(|error| error.map_error(KindFault::new))
        })
    }
    fn describe<'a>(
        &'a self,
        request: DescribeKindRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<KindDescription, KindError<KindFault>>> {
        Box::pin(async move {
            self.describe_kind_with_limits(request, control, limits)
                .await
                .map_err(|error| error.map_error(KindFault::new))
        })
    }
}

/// `SourceChecker`, checked, with the native error erased.
pub trait DynChecker {
    fn check<'a>(
        &'a self,
        request: CheckSourceRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<CheckedSource, KindError<CheckFailure<KindFault>>>>;
}
impl<K> DynChecker for K
where
    K: SourceChecker,
    K::Error: NativeError,
{
    fn check<'a>(
        &'a self,
        request: CheckSourceRequest,
        control: CancellationToken,
        limits: DescriptionLimits,
    ) -> Work<'a, Result<CheckedSource, KindError<CheckFailure<KindFault>>>> {
        Box::pin(async move {
            self.check_source_with_limits(request, control, limits)
                .await
                .map_err(|error| error.map_error(|failure| failure.map_error(KindFault::new)))
        })
    }
}

/// `SourcePuller`, checked, with the native error erased.
pub trait DynPuller {
    fn pull<'a>(
        &'a self,
        request: PullRequest,
        control: CancellationToken,
        limits: PullLimits,
    ) -> Work<'a, Result<(PullResult, ConformantDocument), PullError<KindFault>>>;
}
impl<K> DynPuller for K
where
    K: SourcePuller,
    K::Error: NativeError,
{
    fn pull<'a>(
        &'a self,
        request: PullRequest,
        control: CancellationToken,
        limits: PullLimits,
    ) -> Work<'a, Result<(PullResult, ConformantDocument), PullError<KindFault>>> {
        Box::pin(async move {
            self.pull_source_with_limits(request, control, limits)
                .await
                .map_err(|error| error.map_error(KindFault::new))
        })
    }
}

/// `BindingInvoker` and `InvocationSupport`, as hooks. The router forwards an already
/// admitted invocation to these with the same `ProviderIo`, so the SDK's checks and
/// channel exist once per call. The methods are not named `invoke` and `preflight`:
/// a blanket-implemented trait shares every adapter's method namespace with the SDK's
/// `BindingInvokerExt`, and the same names there make each call ambiguous.
pub trait DynInvoker {
    fn can_invoke(&self, kind: &str) -> bool;
    fn forward_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Work<'a, Result<(), InvocationError>>;
    fn forward_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Work<'a, Result<Option<ContextChallenge>, InvocationError>>;
}
impl<K: BindingInvoker + InvocationSupport> DynInvoker for K {
    fn can_invoke(&self, kind: &str) -> bool {
        InvocationSupport::can_invoke(self, kind)
    }
    fn forward_invoke<'a>(
        &'a self,
        request: BindingRequest,
        io: ProviderIo,
        control: CancellationToken,
    ) -> Work<'a, Result<(), InvocationError>> {
        Box::pin(self.handle_invoke(request, io, control))
    }
    fn forward_preflight<'a>(
        &'a self,
        request: BindingRequest,
        control: CancellationToken,
    ) -> Work<'a, Result<Option<ContextChallenge>, InvocationError>> {
        Box::pin(self.handle_preflight(request, control))
    }
}

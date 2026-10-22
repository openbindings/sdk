use crate::CancellationToken;
use crate::{
    InspectionError, InspectionErrorKind, KindSupportVerdict, SourceInspection, SupportedKind,
};
use openbindings::Source;
use std::{
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};

/// Implementer seam for the Source Inspector interface. Consumers use [`Inspector`].
///
/// Each implementation owns kind semantics, source acquisition and any host
/// handles. Checks must be pure and deterministic for its version/configuration.
/// Inspection must not intentionally mutate the source or external state, and
/// must preserve ordering for the same observed source state/configuration.
///
/// A checked call is `Send` when the provider is `Sync` and its associated future
/// is `Send`; neither bound is required of browser-local providers.
/// Use `Ready` for synchronous work, or
/// `Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>`
/// for ordinary native async implementations; omit `Send` for local host handles.
/// No allocation or executor is imposed by this trait.
pub trait SourceInspector {
    /// Future returned by this implementation. It may borrow the provider,
    /// and source for the duration of inspection. The cancellation control is
    /// owned and shares cancellation state with the caller.
    type Inspection<'a>: Future<Output = Result<SourceInspection, InspectionError>>
    where
        Self: 'a;

    /// Authoritative support for this exact kind and this inspection job only.
    /// No normalization, prefix inference, acquisition or other effects.
    fn supports_kind(&self, kind: &str) -> bool;

    /// Advisory list. Every listed kind must be supported; omission says nothing.
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        Vec::new()
    }

    /// Provider hook called after the checked facade admits the source and kind.
    ///
    /// Refuse unreadable sources. Omit unhandled admitted targets only with an
    /// explicit partial result. Observe `control` between bounded synchronous
    /// units of work. The owned control shares state with the caller; it can be
    /// retained across suspension without borrowing a facade-local token.
    /// The facade drops a pending future on cancellation, but
    /// cannot preempt a blocking poll or undo detached work started by a provider.
    fn inspect<'a>(
        &'a self,
        source: &'a Source,
        control: CancellationToken,
    ) -> Self::Inspection<'a>;
}

// Preserve the underlying future rather than boxing or changing its Send bounds.
macro_rules! forward_inspector {
    ($($wrapper:ty),+ $(,)?) => {$ (
        impl<P: SourceInspector + ?Sized> SourceInspector for $wrapper {
            type Inspection<'a> = P::Inspection<'a> where Self: 'a;

            fn supports_kind(&self, kind: &str) -> bool {
                (**self).supports_kind(kind)
            }

            fn supported_kinds(&self) -> Vec<SupportedKind> {
                (**self).supported_kinds()
            }

            fn inspect<'a>(&'a self, source: &'a Source, control: CancellationToken) -> Self::Inspection<'a> {
                (**self).inspect(source, control)
            }
        }
    )+};
}
forward_inspector!(&P, std::sync::Arc<P>, std::rc::Rc<P>);

/// Checked consumer entry point around a source-inspection provider.
///
/// Own one per provider configuration and reuse it. Wrap `&P`, `Arc<P>` or `Rc<P>`
/// to reuse an existing provider and its state across independent interfaces.
/// This is not a registry or a
/// transport. The source is borrowed during work; returned inventories own their
/// values and remain valid after the source/provider are dropped.
#[derive(Clone)]
pub struct Inspector<P> {
    provider: P,
}

impl<P: SourceInspector> Inspector<P> {
    /// Wrap a provider with interface boundary checks.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Return one verdict per unique kind, preserving first-occurrence order.
    /// Empty kind strings are invalid input. Checks never inspect a source.
    ///
    /// # Errors
    /// Returns an error if any requested kind is empty.
    pub fn check_kind_support<K: AsRef<str>>(
        &self,
        kinds: impl IntoIterator<Item = K>,
    ) -> Result<Vec<KindSupportVerdict>, InspectionError> {
        let kinds: Vec<_> = kinds.into_iter().collect();
        if kinds.iter().any(|kind| kind.as_ref().is_empty()) {
            // Each facade spells this refusal in its own code vocabulary: synthesis
            // as `openbindings.synthesis.empty-kind`, binding invocation as the
            // boundary code `empty-kind` at `/kinds`.
            return Err(InspectionError::new(
                InspectionErrorKind::InvalidInput,
                "openbindings.inspection.empty-kind",
                "kind must be nonempty",
            ));
        }
        Ok(openbindings_internal_interfaces::kind_support_verdicts(
            kinds.iter().map(AsRef::as_ref),
            |kind| self.provider.supports_kind(kind),
        ))
    }

    /// Validate and return the provider's advisory list. An empty list is valid
    /// and must never be used to infer that every kind is unsupported.
    ///
    /// # Errors
    /// Returns an error if the provider lists an empty kind or a kind it does not support.
    /// Duplicate supported kinds are preserved in provider order.
    pub fn list_supported_kinds(&self) -> Result<Vec<SupportedKind>, InspectionError> {
        let kinds = self.provider.supported_kinds();
        if kinds
            .iter()
            .any(|kind| kind.kind.is_empty() || !self.provider.supports_kind(&kind.kind))
        {
            return Err(InspectionError::result(
                "advisory list contains an empty or unsupported kind",
            ));
        }
        Ok(kinds)
    }

    /// Inspect a source with a fresh, uncancelled control. Work begins when the
    /// returned future is polled.
    pub fn inspect<'a>(
        &'a self,
        source: &'a Source,
    ) -> impl Future<Output = Result<SourceInspection, InspectionError>> + 'a {
        self.inspect_owned(source, CancellationToken::new())
    }
    /// Inspect with caller-owned cancellation. The control is cloned, sharing
    /// its cancellation state; the future does not borrow the caller's handle.
    /// Carrier admission and support checks finish before the provider hook is
    /// called. All work begins when the future is polled. Results are structurally
    /// checked before delivery. Kind-specific semantic claims remain the provider's.
    pub fn inspect_with_control<'a>(
        &'a self,
        source: &'a Source,
        control: &CancellationToken,
    ) -> impl Future<Output = Result<SourceInspection, InspectionError>> + 'a {
        self.inspect_owned(source, control.clone())
    }
    fn inspect_owned<'a>(
        &'a self,
        source: &'a Source,
        control: CancellationToken,
    ) -> impl Future<Output = Result<SourceInspection, InspectionError>> + 'a {
        let provider_control = control.clone();
        run_checked(
            move || {
                if provider_control.is_cancelled() {
                    return Err(InspectionError::cancelled());
                }
                if source.kind.is_empty() {
                    return Err(InspectionError::input("source kind must be nonempty"));
                }
                if let Some(key) = source
                    .additional_fields
                    .keys()
                    .find(|key| !key.starts_with("x-"))
                {
                    return Err(InspectionError::input(format!(
                        "source member {key:?} is not an x- extension"
                    )));
                }
                if !self.provider.supports_kind(&source.kind) {
                    return Err(InspectionError::new(
                        InspectionErrorKind::UnsupportedKind,
                        "openbindings.inspection.unsupported-kind",
                        "provider does not support this exact kind for inspection",
                    ));
                }
                Ok(self.provider.inspect(source, provider_control))
            },
            control,
        )
    }
}
// Keep the provider future generic here: a GAT projection inside the facade
// coroutine can otherwise impose an unintended static lifetime when callers
// require a borrowed provider's checked future to be Send. The start closure
// also keeps admission and provider invocation lazy.
async fn run_checked<F>(
    start: impl FnOnce() -> Result<F, InspectionError>,
    control: CancellationToken,
) -> Result<SourceInspection, InspectionError>
where
    F: Future<Output = Result<SourceInspection, InspectionError>>,
{
    let mut future = pin!(start()?);
    let mut cancelled = pin!(control.cancelled());
    let result = poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(InspectionError::cancelled()));
        }
        future.as_mut().poll(cx)
    })
    .await?;
    if control.is_cancelled() {
        return Err(InspectionError::cancelled());
    }
    result.validate()?;
    Ok(result)
}

impl<P> std::fmt::Debug for Inspector<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inspector").finish_non_exhaustive()
    }
}

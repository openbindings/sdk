use crate::{
    InspectionError, InspectionErrorKind, KindSupportVerdict, SourceInspection, SupportedKind,
};
use openbindings::{Source, WorkControl};
use std::{
    collections::HashSet,
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};

/// Implementer seam for the Source Inspector role. Consumers use [`Inspector`].
///
/// Each implementation owns kind semantics, source acquisition and any host
/// handles. Checks must be pure and deterministic for its version/configuration.
/// Inspection must not intentionally mutate the source or external state, and
/// must preserve ordering for the same observed source state/configuration.
///
/// The associated future preserves a native provider's `Send` capability without
/// requiring it of browser-local providers. Use `Ready` for synchronous work, or
/// `Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>`
/// for ordinary native async implementations; omit `Send` for local host handles.
/// No allocation or executor is imposed by this trait.
pub trait SourceInspector {
    /// Future returned by this implementation. It may borrow the provider,
    /// source and cancellation control for the duration of inspection.
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
    /// units of work. The facade drops a pending future on cancellation, but
    /// cannot preempt a blocking poll or undo detached work started by a provider.
    fn inspect<'a>(&'a self, source: &'a Source, control: &'a WorkControl) -> Self::Inspection<'a>;
}

/// Checked consumer entry point around a source-inspection provider.
///
/// Own one per provider configuration and reuse it. This is not a registry or a
/// transport. The source is borrowed during work; returned inventories own their
/// values and remain valid after the source/provider are dropped.
#[derive(Clone, Debug)]
pub struct Inspector<P> {
    provider: P,
}

impl<P: SourceInspector> Inspector<P> {
    /// Wrap a provider with role boundary checks.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Return one verdict per unique kind, preserving first-occurrence order.
    /// Empty kind strings are invalid input. Checks never inspect a source.
    pub fn check_kind_support<K: AsRef<str>>(
        &self,
        kinds: impl IntoIterator<Item = K>,
    ) -> Result<Vec<KindSupportVerdict>, InspectionError> {
        let kinds: Vec<_> = kinds.into_iter().collect();
        if kinds.iter().any(|kind| kind.as_ref().is_empty()) {
            return Err(InspectionError::input("kind must be nonempty"));
        }
        let mut seen = HashSet::new();
        Ok(kinds
            .iter()
            .map(AsRef::as_ref)
            .filter(|kind| seen.insert(*kind))
            .map(|kind| KindSupportVerdict {
                kind: kind.into(),
                supported: self.provider.supports_kind(kind),
            })
            .collect())
    }

    /// Validate and return the provider's advisory list. An empty list is valid
    /// and must never be used to infer that every kind is unsupported.
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

    /// Inspect a source with a fresh, uncancelled control.
    pub async fn inspect(&self, source: &Source) -> Result<SourceInspection, InspectionError> {
        self.inspect_with_control(source, &WorkControl::new()).await
    }

    /// Inspect with caller-owned cancellation. Carrier admission and support
    /// checks finish before the provider hook is called. Results are structurally
    /// checked before delivery. Kind-specific semantic claims remain the provider's.
    pub async fn inspect_with_control(
        &self,
        source: &Source,
        control: &WorkControl,
    ) -> Result<SourceInspection, InspectionError> {
        if control.is_cancelled() {
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
        let mut future = pin!(self.provider.inspect(source, control));
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
}

use crate::CancellationToken;
use crate::*;
use openbindings::ValidatedDocument;
use std::{
    collections::HashSet,
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};
/// Implementer seam for independent synthesis. Consumers use [`Synthesizer`].
///
/// Support checks are pure. Derivation must preserve sources, truthfully account
/// for units and emit only kind-sound bindings. A checked call is Send when the
/// provider is Sync and its associated future is Send. Neither is mandatory for
/// local providers. Cancellation cannot preempt blocking polls or undo detached work.
pub trait InterfaceSynthesizer {
    /// Provider future, optionally borrowing provider/request data; no boxing required.
    type Synthesis<'a>: Future<Output = Result<SynthesisCandidate, SynthesisError>>
    where
        Self: 'a;
    /// Authoritative support for this exact kind and this synthesis job.
    fn supports_kind(&self, kind: &str) -> bool;
    /// Advisory kinds; every listed kind must be supported.
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        Vec::new()
    }
    /// Derive once, after facade admission. Refuse unsupported composition before
    /// acquiring sources. The owned control shares state with the caller.
    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest<'a>,
        control: CancellationToken,
    ) -> Self::Synthesis<'a>;
}
macro_rules! forward {($($wrapper:ty),+)=>{$(
    impl<P:InterfaceSynthesizer+?Sized> InterfaceSynthesizer for $wrapper {
        type Synthesis<'a>=P::Synthesis<'a> where Self:'a;
        fn supports_kind(&self,k:&str)->bool{(**self).supports_kind(k)}
        fn supported_kinds(&self)->Vec<SupportedKind>{(**self).supported_kinds()}
        fn synthesize<'a>(&'a self,r:&'a SynthesisRequest<'a>,c:CancellationToken)->Self::Synthesis<'a>{(**self).synthesize(r,c)}
    }
)+};}
forward!(&P, std::sync::Arc<P>, std::rc::Rc<P>);
/// Checked consumer entry point; reuse with owned, borrowed or shared providers.
#[derive(Clone)]
pub struct Synthesizer<P> {
    provider: P,
}
impl<P: InterfaceSynthesizer> Synthesizer<P> {
    /// Wrap a provider with request/result checks.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
    /// One pure verdict per unique kind in first-occurrence order.
    ///
    /// # Errors
    /// Returns an error if any requested kind is empty.
    pub fn check_kind_support<K: AsRef<str>>(
        &self,
        kinds: impl IntoIterator<Item = K>,
    ) -> Result<Vec<KindSupportVerdict>, SynthesisError> {
        let kinds: Vec<_> = kinds.into_iter().collect();
        if kinds.iter().any(|k| k.as_ref().is_empty()) {
            return Err(SynthesisError::simple(
                SynthesisErrorKind::InvalidInput,
                "empty-kind",
                "kind must be nonempty",
            ));
        }
        let mut seen = HashSet::new();
        Ok(kinds
            .iter()
            .map(AsRef::as_ref)
            .filter(|k| seen.insert(*k))
            .map(|k| KindSupportVerdict {
                kind: k.into(),
                supported: self.provider.supports_kind(k),
            })
            .collect())
    }
    /// Validate the advisory list; absence is not a negative support verdict.
    ///
    /// # Errors
    /// Returns an error if the provider lists empty or duplicate kinds, or lists a kind it does not support.
    pub fn list_supported_kinds(&self) -> Result<Vec<SupportedKind>, SynthesisError> {
        let kinds = self.provider.supported_kinds();
        if kinds
            .iter()
            .any(|k| k.kind.is_empty() || !self.provider.supports_kind(&k.kind))
        {
            return Err(SynthesisError::simple(
                SynthesisErrorKind::InvalidResult,
                "advisory-list",
                "advisory list has an empty or unsupported kind",
            ));
        }
        Ok(kinds)
    }
    /// Complete admitted coverage or a refusal retaining the checked partial result.
    /// Work begins only when the returned future is polled.
    pub fn synthesize<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
    ) -> impl Future<Output = Result<ValidatedDocument, SynthesisError>> + 'a {
        strict(self.with_control(r, CancellationToken::new()))
    }
    /// Strict synthesis with shared caller cancellation.
    pub fn synthesize_with_control<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        c: &CancellationToken,
    ) -> impl Future<Output = Result<ValidatedDocument, SynthesisError>> + 'a {
        strict(self.with_control(r, c.clone()))
    }
    /// Sound document and durable checked coverage, potentially incomplete.
    pub fn synthesize_with_coverage<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
    ) -> impl Future<Output = Result<Synthesis, SynthesisError>> + 'a {
        self.with_control(r, CancellationToken::new())
    }
    /// Coverage synthesis with shared cancellation; does not borrow the control handle.
    pub fn synthesize_with_coverage_and_control<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        c: &CancellationToken,
    ) -> impl Future<Output = Result<Synthesis, SynthesisError>> + 'a {
        self.with_control(r, c.clone())
    }
    fn with_control<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        control: CancellationToken,
    ) -> impl Future<Output = Result<Synthesis, SynthesisError>> + 'a {
        let provider_control = control.clone();
        checked(
            move || {
                if provider_control.is_cancelled() {
                    return Err(SynthesisError::cancelled());
                }
                crate::validate::admit(r)?;
                if r.sources
                    .iter()
                    .any(|s| !self.provider.supports_kind(&s.source.kind))
                {
                    return Err(SynthesisError::simple(
                        SynthesisErrorKind::UnsupportedKind,
                        "unsupported-kind",
                        "a supplied exact kind is unsupported for synthesis",
                    ));
                }
                Ok(self.provider.synthesize(r, provider_control))
            },
            r,
            control,
        )
    }
}
// Keep the provider GAT out of the coroutine's own type projections; also keeps
// admission and provider invocation lazy for borrowed, conditionally-Send calls.
async fn checked<'a, F: Future<Output = Result<SynthesisCandidate, SynthesisError>>>(
    start: impl FnOnce() -> Result<F, SynthesisError>,
    request: &'a SynthesisRequest<'a>,
    control: CancellationToken,
) -> Result<Synthesis, SynthesisError> {
    let mut work = pin!(start()?);
    let mut cancelled = pin!(control.cancelled());
    let candidate = poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(SynthesisError::cancelled()));
        }
        work.as_mut().poll(cx)
    })
    .await
    .map_err(SynthesisError::checked_provider_error)?;
    if control.is_cancelled() {
        return Err(SynthesisError::cancelled());
    }
    let result = crate::validate::validate(request, candidate);
    if control.is_cancelled() {
        return Err(SynthesisError::cancelled());
    }
    result
}
async fn strict<F: Future<Output = Result<Synthesis, SynthesisError>>>(
    work: F,
) -> Result<ValidatedDocument, SynthesisError> {
    let s = work.await?;
    if s.coverage().strict_eligible() {
        Ok(s.document)
    } else {
        Err(SynthesisError::incomplete(s))
    }
}

impl<P> std::fmt::Debug for Synthesizer<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Synthesizer").finish_non_exhaustive()
    }
}

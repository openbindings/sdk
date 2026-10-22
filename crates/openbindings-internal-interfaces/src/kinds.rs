//! Kind support records shared by every interface that lists or checks kinds.
use serde::Serialize;
use std::collections::HashSet;

/// A kind a provider supports for its job, in an advisory list: an unlisted
/// kind may still be supported, so omission conveys no negative verdict.
///
/// The source inspector, interface synthesizer and binding invoker interfaces
/// define this record identically, and every companion hands out this one type,
/// so a provider serving several jobs can build its list once. Providers build
/// one with [`Self::new`] and [`Self::with_description`]. Later versions may
/// add fields: read them by name and match with `..`.
///
/// ```
/// use openbindings_internal_interfaces::SupportedKind;
/// let kind = SupportedKind::new("urn:example:kind:v1").with_description("Example kind");
/// assert_eq!(kind.kind, "urn:example:kind:v1");
/// ```
#[derive(Clone, Serialize)]
#[non_exhaustive]
pub struct SupportedKind {
    /// Exact, opaque kind as carried by a source.
    pub kind: String,
    /// Optional human-facing description of the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
impl SupportedKind {
    /// An exact kind without a description.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            description: None,
        }
    }
    /// Attach a human-facing description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}
impl std::fmt::Debug for SupportedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupportedKind").finish_non_exhaustive()
    }
}

/// Authoritative support verdict for one exact kind. The job it answers for is
/// the one the checked caller that returned it performs: inspection,
/// synthesis or binding invocation.
/// Later versions may add fields: read them by name and match with `..`.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct KindSupportVerdict {
    /// Exact requested kind, unchanged.
    pub kind: String,
    /// Whether the provider knows how to interpret this exact kind for that job;
    /// not a promise that every case succeeds.
    pub supported: bool,
}
impl std::fmt::Debug for KindSupportVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KindSupportVerdict")
            .field("supported", &self.supported)
            .finish_non_exhaustive()
    }
}

/// One verdict per unique kind, in first-occurrence order. The companions'
/// checked callers call this after refusing an empty kind; it is not part of
/// any companion's API.
#[doc(hidden)]
pub fn kind_support_verdicts<'a>(
    kinds: impl IntoIterator<Item = &'a str>,
    supports: impl Fn(&str) -> bool,
) -> Vec<KindSupportVerdict> {
    let mut seen = HashSet::new();
    kinds
        .into_iter()
        .filter(|kind| seen.insert(*kind))
        .map(|kind| KindSupportVerdict {
            kind: kind.into(),
            supported: supports(kind),
        })
        .collect()
}

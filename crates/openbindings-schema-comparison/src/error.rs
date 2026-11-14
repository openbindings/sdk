use openbindings::SchemaOrigin;
use std::{fmt, sync::Arc};

/// Category of a failed comparison. No comparison verdict is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComparisonFailureKind {
    /// A schema or a supported keyword has an inadmissible shape.
    InvalidSchema,
    /// Repeated decoded object members make an exact value ambiguous.
    DuplicateMember,
    /// Reference syntax or the supplied reference context is invalid.
    InvalidReference,
    /// Explicitly supplied content does not provide the requested target.
    MissingReference,
    /// The reference form is valid but this profile resolver does not implement it.
    UnsupportedReference,
    /// The pinned profile does not specify this normalization edge sufficiently.
    UnsupportedNormalization,
    /// An active original reference target is reached again.
    ReferenceCycle,
    /// A specified type/enum intersection is empty, or merged const values conflict.
    /// This is not a general schema satisfiability classification.
    UnsatisfiableMerge,
    /// An explicit per-attempt resource allowance was exhausted.
    LimitExceeded(&'static str),
    /// A context root is not the exact root of its paired conformance proof.
    ContextMismatch,
    /// The caller cancelled the comparison.
    Cancelled,
}
/// The original operand responsible for a comparison failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComparisonSide {
    /// Left operand of an identity comparison.
    Left,
    /// Right operand of an identity comparison.
    Right,
    /// Target operand of a directional comparison.
    Target,
    /// Candidate operand of a directional comparison.
    Candidate,
    /// Required document of an interface check, regardless of selected role.
    Required,
    /// Provided document of an interface check, regardless of selected role.
    Provided,
}

/// Failure to normalize or compare schemas. No comparison verdict is implied.
/// Classification is separate from optional original-source provenance. Formatting
/// omits source values. Equality includes provenance; use `kind()` for categories.
#[derive(Clone)]
pub struct ComparisonFailure {
    kind: ComparisonFailureKind,
    side: Option<ComparisonSide>,
    location: Option<Arc<Location>>,
    origin_attempted: bool,
}
struct Location {
    origin: SchemaOrigin,
    bytes: usize,
}
impl ComparisonFailure {
    /// Copyable category, independent of the original source location.
    pub fn kind(&self) -> ComparisonFailureKind {
        self.kind
    }
    /// Established location in the original input or supplied resource.
    /// Limits, cancellation and failures without known ancestry have no location.
    pub fn origin(&self) -> Option<&SchemaOrigin> {
        self.location.as_ref().map(|location| &location.origin)
    }
    /// Original operand when established. Global limits and cancellation have none.
    pub fn side(&self) -> Option<ComparisonSide> {
        self.side
    }
    pub(crate) fn with_side(mut self, side: ComparisonSide) -> Self {
        if !self.is_global() {
            self.side = Some(side);
        }
        self
    }
    pub(crate) fn is_global(&self) -> bool {
        matches!(
            self.kind,
            ComparisonFailureKind::LimitExceeded(_) | ComparisonFailureKind::Cancelled
        )
    }
    pub(crate) fn needs_origin(&self) -> bool {
        !self.origin_attempted && !self.is_global()
    }
    pub(crate) fn mark_origin_attempted(&mut self) {
        self.origin_attempted = true;
    }
    pub(crate) fn located(mut self, origin: SchemaOrigin, bytes: usize) -> Self {
        self.location = Some(Arc::new(Location { origin, bytes }));
        self
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.location.as_ref().map_or(0, |location| location.bytes)
    }
}
impl PartialEq for ComparisonFailure {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.side == other.side && self.origin() == other.origin()
    }
}
impl Eq for ComparisonFailure {}
impl From<ComparisonFailureKind> for ComparisonFailure {
    fn from(kind: ComparisonFailureKind) -> Self {
        Self {
            kind,
            side: None,
            location: None,
            origin_attempted: false,
        }
    }
}
impl fmt::Debug for ComparisonFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComparisonFailure")
            .field("kind", &self.kind)
            .field("side", &self.side)
            .field("origin", &self.origin())
            .finish()
    }
}
impl fmt::Display for ComparisonFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            ComparisonFailureKind::InvalidSchema => "a schema or keyword has an invalid shape",
            ComparisonFailureKind::DuplicateMember => "an exact value repeats an object member",
            ComparisonFailureKind::InvalidReference => {
                "a reference or its supplied context is invalid"
            }
            ComparisonFailureKind::MissingReference => "a reference target was not supplied",
            ComparisonFailureKind::UnsupportedReference => "this reference form is not implemented",
            ComparisonFailureKind::UnsupportedNormalization => {
                "this normalization edge is not implemented"
            }
            ComparisonFailureKind::ReferenceCycle => {
                "the comparison profile cannot inline a reference cycle"
            }
            ComparisonFailureKind::UnsatisfiableMerge => "the profile merge is unsatisfiable",
            ComparisonFailureKind::LimitExceeded(_) => {
                "the comparison exhausted a configured resource allowance"
            }
            ComparisonFailureKind::Cancelled => "the caller cancelled the comparison",
            ComparisonFailureKind::ContextMismatch => {
                "a comparison context root does not match its conformance proof"
            }
        })
    }
}
impl std::error::Error for ComparisonFailure {}

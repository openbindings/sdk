use std::fmt;

/// Failure to establish normalized identity. No equality verdict is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComparisonFailure {
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
    /// The caller cancelled the comparison.
    Cancelled,
}
impl fmt::Display for ComparisonFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSchema => "a schema or keyword has an invalid shape",
            Self::DuplicateMember => "an exact value repeats an object member",
            Self::InvalidReference => "a reference or its supplied context is invalid",
            Self::MissingReference => "a reference target was not supplied",
            Self::UnsupportedReference => "this reference form is not implemented",
            Self::UnsupportedNormalization => "this normalization edge is not implemented",
            Self::ReferenceCycle => "the comparison profile cannot inline a reference cycle",
            Self::UnsatisfiableMerge => "the profile merge is unsatisfiable",
            Self::LimitExceeded(_) => "the comparison exhausted a configured resource allowance",
            Self::Cancelled => "the caller cancelled the comparison",
        })
    }
}
impl std::error::Error for ComparisonFailure {}

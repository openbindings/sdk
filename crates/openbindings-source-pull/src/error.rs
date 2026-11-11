use openbindings::{DocumentValidation, EditError, VersionRefusal};
use std::fmt;

/// Request shape or original document admission failure, before provider work.
pub enum RequestError {
    /// Invalid fixed request field or closed-carrier rule.
    Shape(&'static str),
    /// The original document did not obtain core conformance.
    Interface(DocumentValidation),
    /// The original document requests an unsupported specification version.
    Version(VersionRefusal),
}
impl fmt::Debug for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(field) => f.debug_tuple("Shape").field(field).finish(),
            Self::Interface(_) => f.write_str("Interface { .. }"),
            Self::Version(_) => f.write_str("Version { .. }"),
        }
    }
}
impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Shape(_) => "source-pull request has an invalid shape",
            Self::Interface(_) => "source-pull request requires a conformant document",
            Self::Version(_) => "source-pull request version is not supported",
        })
    }
}
impl std::error::Error for RequestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Version(error) => Some(error),
            Self::Shape(_) | Self::Interface(_) => None,
        }
    }
}

/// A result contradicts a mechanically checkable relationship in the publication.
/// This does not judge kind-owned fidelity, change classification or completeness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelationshipError {
    /// An identifier cannot name a conformant document entry.
    Identifier,
    /// An addition replaces an original key or an operation key collides with an alias.
    KeyCollision,
    /// An added binding does not name the selected source.
    BindingSource,
    /// Added reports do not cover added bindings exactly once.
    AddedCoverage,
    /// A report's operation differs from its binding's canonical operation key.
    BindingOperation,
    /// Reused does not identify an original operation, or ordinary does not identify a new one.
    Reuse,
    /// Existing reports do not cover the original source's bindings exactly once.
    ExistingCoverage,
    /// An existing target is absent or belongs to a different source.
    ExistingTarget,
    /// A binding report's references label is malformed or names no input schema or operation slot.
    ReferenceTarget,
    /// An omitted existing, alsoIdentifies or through label is malformed or names no input entry.
    OmittedTarget,
    /// An existing report repeats the same entry type and key.
    DuplicateTarget,
    /// A uses label is malformed, repeated, names no addition, or names its own binding.
    Uses,
}
/// Failure of local result validation, distinct from a provider's operational error.
pub enum PullValidationError {
    /// The selected source is absent from the original document.
    MissingSource,
    /// A report/document relationship was contradicted.
    Relationship(RelationshipError),
    /// A finite local handoff or relationship allowance was exhausted.
    LimitExceeded(&'static str),
    /// The caller cancelled local work.
    Cancelled,
    /// Core could not build the prospective document under the original JSON limits.
    Edit(EditError),
    /// Adding all declared additions did not obtain core conformance.
    Prospective(DocumentValidation),
    /// Core refused the prospective document's specification version.
    Version(VersionRefusal),
}
impl From<RelationshipError> for PullValidationError {
    fn from(value: RelationshipError) -> Self {
        Self::Relationship(value)
    }
}
impl fmt::Debug for PullValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSource => f.write_str("MissingSource"),
            Self::Relationship(e) => f.debug_tuple("Relationship").field(e).finish(),
            Self::LimitExceeded(e) => f.debug_tuple("LimitExceeded").field(e).finish(),
            Self::Cancelled => f.write_str("Cancelled"),
            Self::Edit(_) => f.write_str("Edit { .. }"),
            Self::Prospective(_) => f.write_str("Prospective { .. }"),
            Self::Version(_) => f.write_str("Version { .. }"),
        }
    }
}
impl fmt::Display for PullValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingSource => "source-pull validation requires the selected source",
            Self::Relationship(_) => "source-pull result contradicts a document relationship",
            Self::LimitExceeded(_) => "source-pull validation exceeded a local limit",
            Self::Cancelled => "source-pull validation was cancelled",
            Self::Edit(_) => "source-pull validation could not build the prospective document",
            Self::Prospective(_) => {
                "source-pull additions require a conformant prospective document"
            }
            Self::Version(_) => "source-pull prospective document version is not supported",
        })
    }
}
impl std::error::Error for PullValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            Self::Version(error) => Some(error),
            _ => None,
        }
    }
}

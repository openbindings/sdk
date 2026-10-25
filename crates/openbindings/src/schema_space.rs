//! OpenBindings reference inspection and the shared §7 projection adapter.
use crate::*;
use openbindings_internal_schema as embedding;
use std::ops::Deref;
#[cfg(test)]
thread_local! {
    /// Resolution spaces assembled on this thread, for reuse tests.
    pub(crate) static ASSEMBLED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
// Holds snapshots of the document and the set's resources, never the set or
// revision handles, so a set that keeps a space forms no ownership cycle.
pub(crate) struct SchemaSpace {
    inner: embedding::SchemaSpace,
}
impl Deref for SchemaSpace {
    type Target = embedding::SchemaSpace;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl SchemaSpace {
    /// Layer supplied resources over the document. Value-contract callers keep
    /// the result per live revision and set; validation keeps its own index.
    pub fn new(document: &Document, supplied: &ResourceSet) -> Self {
        #[cfg(test)]
        ASSEMBLED.with(|count| count.set(count.get() + 1));
        Self {
            inner: embedding::SchemaSpace::new(
                document.indexed(),
                openbindings_internal_schema::SuppliedSet::supplied(supplied).clone(),
            ),
        }
    }
    pub fn program(
        &self,
        entry: usize,
        control: &dyn WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        self.inner
            .program(entry, &|| control.is_cancelled())
            .map(EvaluationProgram::from_projection)
            .map_err(crate::contracts::projection_failure)
    }
    fn references(&self, control: &dyn WorkControl) -> ReferenceReport {
        let report = self.inner.references(&|| control.is_cancelled());
        ReferenceReport {
            complete: report.complete,
            limitation: report.limitation.map(reference_refusal),
            references: report
                .references
                .into_iter()
                .map(|r| Reference {
                    location: crate::contracts::original_location(r.location),
                    keyword: r.keyword,
                    spelling: r.spelling,
                    resolution: match r.resolution {
                        embedding::ReferenceResolution::Located {
                            target,
                            dynamic_lookup,
                        } => ReferenceResolution::Located {
                            target: crate::contracts::original_location(target),
                            dynamic_lookup,
                        },
                        embedding::ReferenceResolution::Unresolved { detail } => {
                            ReferenceResolution::Unresolved {
                                detail: reference_refusal(detail),
                            }
                        }
                    },
                })
                .collect(),
        }
    }
}
/// Extensible cause of a refusal during static reference inspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReferenceRefusalReason {
    /// A referenced resource was not supplied; inspection performs no acquisition.
    ResourceUnavailable,
    /// The reference is malformed or cannot be interpreted.
    InvalidReference,
    /// The reference does not identify a schema.
    MissingTarget,
    /// More than one declaration claims the target.
    AmbiguousTarget,
    /// The resource conflicts with another admitted identity.
    IdentityConflict,
    /// Inspection does not support a required capability.
    UnsupportedCapability,
    /// Inspection exceeded a work or representation limit.
    LimitExceeded,
    /// The caller cancelled inspection.
    Cancelled,
    /// The resolver could not complete inspection.
    ResolverFailure,
}

/// A reference-inspection refusal, separate from a value outcome.
/// Later versions may add fields: read them by name and match with `..`.
#[derive(Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ReferenceRefusal {
    /// Broad refusal category.
    pub reason: ReferenceRefusalReason,
    /// Stable detail code.
    pub code: String,
    /// Explanatory text without source-controlled details by default.
    pub message: String,
    /// Original schema location, when established.
    pub location: Option<SchemaLocation>,
}
impl std::fmt::Debug for ReferenceRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceRefusal")
            .field("reason", &self.reason)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ReferenceRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ReferenceRefusal {}
fn reference_refusal(f: embedding::Failure) -> ReferenceRefusal {
    use ReferenceRefusalReason as R;
    use embedding::FailureReason as F;
    let reason = match f.reason {
        F::Cancelled => R::Cancelled,
        F::LimitExceeded => R::LimitExceeded,
        F::ResourceUnavailable => R::ResourceUnavailable,
        F::UnsupportedCapability => R::UnsupportedCapability,
        _ => match f.code.as_str() {
            "resource-identity-conflict" | "ambiguous-resource" => R::IdentityConflict,
            "ambiguous-anchor" | "ambiguous-dynamic-anchor" => R::AmbiguousTarget,
            "non-schema-target" | "anchor-missing" | "resource-root" => R::MissingTarget,
            "invalid-reference"
            | "anonymous-relative-reference"
            | "fragment-encoding"
            | "missing-base"
            | "reference-type" => R::InvalidReference,
            _ => R::ResolverFailure,
        },
    };
    ReferenceRefusal {
        reason,
        code: f.code,
        message: f.message,
        location: f.location.map(crate::contracts::original_location),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
/// Static reference inspection outcome. Located dynamic references still require evaluation-time scope resolution.
/// Serializes with its variant, `located` or `unresolved`, in `status`.
///
/// This closed partition may be exhaustively matched:
/// ```
/// use openbindings::ReferenceResolution;
/// fn located(resolution: &ReferenceResolution) -> bool {
///     match resolution {
///         ReferenceResolution::Located { .. } => true,
///         ReferenceResolution::Unresolved { .. } => false,
///     }
/// }
/// ```
pub enum ReferenceResolution {
    /// A static target was located in the supplied original context.
    Located {
        /// Original target resource and pointer, not a fetched or generated address.
        target: SchemaLocation,
        /// True when evaluation must additionally apply dynamic-anchor scope; the static target alone is not the final runtime target.
        dynamic_lookup: bool,
    },
    /// No sound target could be established within supported capabilities and supplied resources.
    Unresolved {
        /// Structured unresolved-reference cause at the original reference location when available.
        detail: ReferenceRefusal,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize)]
/// One `$ref` or `$dynamicRef` encountered in supported schema positions; opaque JSON content is not traversed as schema.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Reference {
    /// Original location of the reference keyword.
    pub location: SchemaLocation,
    /// Reference keyword spelling, such as `$ref` or `$dynamicRef`.
    pub keyword: String,
    /// Original scalar reference string, or `None` when its value is not representable as one.
    pub spelling: Option<String>,
    /// Static location result or a structured refusal; never triggers I/O.
    pub resolution: ReferenceResolution,
}
impl std::fmt::Debug for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reference")
            .field("location", &self.location)
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Bounded original-context reference inspection, not conformance proof or successful schema preparation.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct ReferenceReport {
    /// Retained reference observations in traversal order.
    pub references: Vec<Reference>,
    /// Whether traversal completed; individual unresolved references may still occur in a complete report.
    pub complete: bool,
    /// Traversal-level cause when inspection is incomplete, distinct from per-reference refusals.
    /// Inspection skips the references of a schema nested more than 256 schemas below the
    /// schema position that holds it, with a `LimitExceeded` limitation (`schema-depth`).
    pub limitation: Option<ReferenceRefusal>,
}
use serde::Serialize;

impl Document {
    /// Inspect references with no caller resources and a fresh uncancelled control. No network or filesystem access; success is not conformance proof.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance. Unresolved references and cancellation are recorded in the returned report.
    pub fn references(&self) -> Result<ReferenceReport, InterpretationError> {
        self.references_with_resources(ResourceSet::default(), &crate::work_control::NeverCancelled)
    }
    /// Inspect schema references in this exact snapshot plus supplied resources, with cooperative cancellation. Interpretation errors refuse the document; traversal limits are represented in [`ReferenceReport`].
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance. Unresolved references and cancellation are recorded in the returned report.
    pub fn references_with_resources(
        &self,
        resources: ResourceSet,
        control: &dyn WorkControl,
    ) -> Result<ReferenceReport, InterpretationError> {
        self.interpretable()?;
        Ok(SchemaSpace::new(self, &resources).references(control))
    }
}

impl Conformant<Document> {
    /// Inspect references in the conformant document revision without acquiring resources.
    pub fn references(&self) -> ReferenceReport {
        self.references_with_resources(ResourceSet::default(), &crate::work_control::NeverCancelled)
    }
    /// Inspect references using an immutable supplied resource set and control.
    pub fn references_with_resources(
        &self,
        resources: ResourceSet,
        control: &dyn WorkControl,
    ) -> ReferenceReport {
        SchemaSpace::new(self.as_document(), &resources).references(control)
    }
}

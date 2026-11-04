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
    /// Layer supplied resources over the document. Schema callers keep
    /// the result per live revision and set; validation keeps its own index.
    pub(crate) fn new_bounded(
        document: &Document,
        supplied: &ExternalSchemas,
        maximum: usize,
        maximum_identity: usize,
        control: &dyn embedding::Control,
    ) -> Result<Self, embedding::Failure> {
        let inner = embedding::SchemaSpace::new_bounded(
            document.indexed(),
            embedding::SuppliedSet::supplied(supplied).clone(),
            maximum,
            maximum_identity,
            control,
        )?;
        #[cfg(test)]
        ASSEMBLED.with(|count| count.set(count.get() + 1));
        Ok(Self { inner })
    }
    pub(crate) fn check_inventory_limits(
        &self,
        maximum: usize,
        maximum_identity: usize,
        control: &dyn embedding::Control,
    ) -> Result<(), embedding::Failure> {
        control.check()?;
        if self.identity_bytes() > maximum_identity {
            return Err(embedding::Failure::new(
                embedding::FailureReason::LimitExceeded,
                "schema-identity-bytes",
                "expanded schema identities exceed their configured byte budget",
            ));
        }
        if self.node_count() > maximum {
            return Err(embedding::Failure::new(
                embedding::FailureReason::LimitExceeded,
                "schema-nodes",
                "schema inventory exceeds its configured node limit",
            ));
        }
        Ok(())
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
                    location: crate::schema::evaluation::original_location(r.location),
                    keyword: r.keyword,
                    spelling: r.spelling,
                    resolution: match r.resolution {
                        embedding::ReferenceResolution::Located {
                            target,
                            dynamic_lookup,
                        } => ReferenceResolution::Located {
                            target: crate::schema::evaluation::original_location(target),
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
    /// Document preconditions or the resolver prevented inspection; `code` identifies the cause.
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
    pub location: Option<SchemaOrigin>,
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
        _ if f.query_failure.is_some() => {
            use embedding::QueryFailure as Q;
            match f.query_failure.as_deref().expect("query cause") {
                Q::MalformedReference | Q::InvalidIdentifier => R::InvalidReference,
                Q::MissingTarget | Q::NonSchemaTarget | Q::MissingAnchor => R::MissingTarget,
                Q::AmbiguousAnchor => R::AmbiguousTarget,
                Q::IdentityConflict => R::IdentityConflict,
                Q::InvalidExternalSchema => R::ResolverFailure,
                Q::ResourceUnavailable(_) => R::ResourceUnavailable,
                Q::UnsupportedDialect(_) | Q::UnsupportedCapability(_) => R::UnsupportedCapability,
            }
        }
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
        location: f.location.map(crate::schema::evaluation::original_location),
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
        target: SchemaOrigin,
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
    pub location: SchemaOrigin,
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
    /// Inspect schema references in an admitted draft without claiming conformance.
    /// Opaque data is not traversed. No resource is fetched. An unsupported or
    /// malformed version, duplicate member names, cancellation and traversal
    /// limits produce an incomplete report with a structured limitation.
    pub fn references(&self) -> ReferenceReport {
        self.references_with_resources(ExternalSchemas::default(), &crate::control::NeverCancelled)
    }
    /// Inspect the draft plus an immutable supplied resource set. The report is
    /// diagnostic evidence, not a canonical domain view or a prepared contract.
    pub fn references_with_resources(
        &self,
        resources: ExternalSchemas,
        control: &dyn WorkControl,
    ) -> ReferenceReport {
        use ReferenceRefusalReason::{Cancelled, ResolverFailure, UnsupportedCapability};
        let refusal = if control.is_cancelled() {
            Some((Cancelled, "cancelled", "reference inspection was cancelled"))
        } else if crate::conformance::version_refusal(self.value()).is_some() {
            Some((
                UnsupportedCapability,
                "unsupported-version",
                "reference inspection requires a supported specification version",
            ))
        } else if self.value().has_duplicate_names() {
            Some((
                ResolverFailure,
                "duplicate-members",
                "duplicate member names prevent unambiguous reference inspection",
            ))
        } else if self
            .value()
            .get("openbindings")
            .and_then(JsonRef::as_str)
            .is_none_or(|version| check_version(version) != VersionDecision::Supported)
        {
            Some((
                ResolverFailure,
                "malformed-version",
                "reference inspection requires a valid specification version declaration",
            ))
        } else {
            None
        };
        if let Some((reason, code, message)) = refusal {
            return ReferenceReport {
                complete: false,
                references: Vec::new(),
                limitation: Some(ReferenceRefusal {
                    reason,
                    code: code.into(),
                    message: message.into(),
                    location: None,
                }),
            };
        }
        inspect(self, &resources, control)
    }
}

impl ConformantDocument {
    /// Inspect references in the conformant document revision without acquiring resources.
    pub fn references(&self) -> ReferenceReport {
        self.references_with_resources(ExternalSchemas::default(), &crate::control::NeverCancelled)
    }
    /// Inspect references using an immutable supplied resource set and control.
    pub fn references_with_resources(
        &self,
        resources: ExternalSchemas,
        control: &dyn WorkControl,
    ) -> ReferenceReport {
        inspect(self.document(), &resources, control)
    }
}

/// A statically established reference defect. This is semantic evidence at a
/// reference, not an evaluation outcome for a surrounding schema or instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReferenceDefect {
    /// Invalid URI, percent encoding, or JSON Pointer syntax.
    MalformedReference,
    /// No JSON value exists at the requested pointer.
    MissingTarget,
    /// The selected value is not a schema position.
    NonSchemaTarget,
    /// No plain-name declaration exists in the selected resource.
    MissingAnchor,
    /// More than one declaration has the requested plain name.
    AmbiguousAnchor,
    /// The referring scope does not establish the required URI base.
    InvalidIdentifier,
}
/// Structured failure of an original-context schema lookup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "details", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ResolutionFailureKind {
    /// A reference has an established semantic defect.
    Undefined(ReferenceDefect),
    /// Required content was not supplied. No fetch was attempted.
    ResourceUnavailable {
        /// Resolved identity of the absent resource.
        uri: String,
    },
    /// The inventory cannot interpret this resource's dialect.
    UnsupportedDialect {
        /// The declared or inherited dialect URI.
        uri: String,
    },
    /// Original inventory interpretation requires an unsupported capability.
    UnsupportedCapability {
        /// Stable identifier for the unsupported interpretation.
        capability: String,
    },
    /// A reached resource has conflicting identity or invalid dialect metadata.
    InvalidExternalSchema,
    /// A work or representation bound was reached.
    Limit {
        /// Resource whose bound was reached.
        resource: String,
    },
    /// The caller cancelled this query.
    Cancelled,
    /// The node or resource belongs to another environment.
    WrongEnvironment,
    /// The caller requested a value outside the schema inventory.
    NotSchemaLocation,
    /// The caller's dynamic-anchor name is not a valid plain name.
    InvalidAnchorName,
}
/// A query failure, with real original provenance when one is available.
/// Wrong-environment misuse has no origin in the selected environment.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ResolutionFailure {
    /// Structured cause; no diagnostic-message parsing is required.
    #[serde(flatten)]
    pub kind: ResolutionFailureKind,
    /// Location in an original source, never a generated evaluator address.
    pub origin: Option<SchemaOrigin>,
}
impl std::fmt::Debug for ResolutionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolutionFailure").finish_non_exhaustive()
    }
}
impl std::fmt::Display for ResolutionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("schema reference resolution did not complete")
    }
}
impl std::error::Error for ResolutionFailure {}
/// The static target and optional dynamic-anchor name. Dynamic scope selection
/// remains an evaluator responsibility; a pointer never opts into that lookup.
#[derive(Clone, Debug)]
pub struct ReferenceTarget {
    /// Original schema target.
    pub schema: SchemaNode,
    /// Plain dynamic-anchor name, when that declaration was selected by name.
    pub dynamic_anchor: Option<String>,
}
impl SchemaEnvironment {
    /// Resolve a URI in a schema's original scope. The URI may name a
    /// meta-schema or other explicitly supplied resource; it need not appear
    /// verbatim in `$ref` or `$dynamicRef`. No I/O occurs.
    ///
    /// # Errors
    /// Distinguishes established reference defects from missing resources,
    /// unsupported dialects, invalid external schemas, and caller misuse.
    pub fn resolve(
        &self,
        from: &SchemaNode,
        reference: &str,
    ) -> Result<ReferenceTarget, ResolutionFailure> {
        self.resolve_with_control(from, reference, &crate::control::NeverCancelled)
    }
    /// Resolve with cooperative cancellation before and after the query.
    ///
    /// # Errors
    /// Same failures as [`Self::resolve`], with cancellation reported separately.
    pub fn resolve_with_control(
        &self,
        from: &SchemaNode,
        reference: &str,
        control: &dyn WorkControl,
    ) -> Result<ReferenceTarget, ResolutionFailure> {
        if !self.owns(&from.environment) {
            return Err(ResolutionFailure {
                kind: ResolutionFailureKind::WrongEnvironment,
                origin: None,
            });
        }
        let cancelled = || ResolutionFailure {
            kind: ResolutionFailureKind::Cancelled,
            origin: Some(from.origin().clone()),
        };
        control.check().map_err(|_| cancelled())?;
        let result = self
            .inner
            .space
            .query(from.id, reference)
            .map_err(|error| self.query_failure(error, from))?;
        control.check().map_err(|_| cancelled())?;
        Ok(ReferenceTarget {
            schema: self.node(result.node),
            dynamic_anchor: result.dynamic_name,
        })
    }
    /// Find a dynamic-anchor declaration in exactly one resource. A static
    /// anchor of the same name is not a dynamic anchor.
    ///
    /// # Errors
    /// Refuses foreign handles, invalid names, ambiguous declarations,
    /// conflicting resources, and unsupported resource inventories.
    pub fn dynamic_anchor(
        &self,
        resource: &ResourceIdentity,
        name: &str,
    ) -> Result<Option<SchemaNode>, ResolutionFailure> {
        self.dynamic_anchor_with_control(resource, name, &crate::control::NeverCancelled)
    }
    /// Dynamic-anchor lookup with cooperative cancellation.
    ///
    /// # Errors
    /// Same failures as [`Self::dynamic_anchor`], plus cancellation.
    pub fn dynamic_anchor_with_control(
        &self,
        identity: &ResourceIdentity,
        name: &str,
        control: &dyn WorkControl,
    ) -> Result<Option<SchemaNode>, ResolutionFailure> {
        if !self.owns(&identity.environment) {
            return Err(ResolutionFailure {
                kind: ResolutionFailureKind::WrongEnvironment,
                origin: None,
            });
        }
        let resource = self.inner.space.resource(identity.id);
        let origin = resource.root.map(|id| self.node(id).origin().clone());
        let failure = |kind| ResolutionFailure {
            kind,
            origin: origin.clone(),
        };
        control
            .check()
            .map_err(|_| failure(ResolutionFailureKind::Cancelled))?;
        if !embedding::schema_index::plain_name(name) {
            return Err(failure(ResolutionFailureKind::InvalidAnchorName));
        }
        if !self.inner.space.eligible(identity.id) {
            return Err(failure(ResolutionFailureKind::InvalidExternalSchema));
        }
        self.inner
            .space
            .query_inventory(identity.id)
            .map_err(|error| failure(query_kind(error)))?;
        let result = match resource.anchors.get(name).map(Vec::as_slice) {
            Some([(node, true)]) => Ok(Some(self.node(*node))),
            Some([(_, false)]) | None => Ok(None),
            Some(declarations) => Err(ResolutionFailure {
                kind: ResolutionFailureKind::Undefined(ReferenceDefect::AmbiguousAnchor),
                origin: declarations
                    .first()
                    .map(|(id, _)| self.node(*id).origin().clone()),
            }),
        };
        control
            .check()
            .map_err(|_| failure(ResolutionFailureKind::Cancelled))?;
        result
    }
    pub(crate) fn query_failure(
        &self,
        error: embedding::QueryFailure,
        from: &SchemaNode,
    ) -> ResolutionFailure {
        ResolutionFailure {
            kind: query_kind(error),
            origin: Some(from.origin().clone()),
        }
    }
}
pub(crate) fn query_kind(error: embedding::QueryFailure) -> ResolutionFailureKind {
    use ReferenceDefect as D;
    use ResolutionFailureKind as R;
    use embedding::QueryFailure as Q;
    match error {
        Q::MalformedReference => R::Undefined(D::MalformedReference),
        Q::MissingTarget => R::Undefined(D::MissingTarget),
        Q::NonSchemaTarget => R::Undefined(D::NonSchemaTarget),
        Q::MissingAnchor => R::Undefined(D::MissingAnchor),
        Q::AmbiguousAnchor => R::Undefined(D::AmbiguousAnchor),
        Q::InvalidIdentifier => R::Undefined(D::InvalidIdentifier),
        Q::IdentityConflict | Q::InvalidExternalSchema | Q::UnsupportedDialect(None) => {
            R::InvalidExternalSchema
        }
        Q::ResourceUnavailable(uri) => R::ResourceUnavailable { uri },
        Q::UnsupportedDialect(Some(uri)) => R::UnsupportedDialect { uri },
        Q::UnsupportedCapability(capability) => R::UnsupportedCapability {
            capability: capability.into(),
        },
    }
}

fn inspect(
    document: &Document,
    resources: &ExternalSchemas,
    control: &dyn WorkControl,
) -> ReferenceReport {
    let limits = PreparationLimits::default();
    match SchemaSpace::new_bounded(
        document,
        resources,
        limits.max_schema_nodes,
        limits.max_identity_bytes,
        &|| control.is_cancelled(),
    ) {
        Ok(space) => space.references(control),
        Err(failure) => ReferenceReport {
            complete: false,
            references: Vec::new(),
            limitation: Some(reference_refusal(failure)),
        },
    }
}

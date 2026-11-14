//! OpenBindings reference inspection and the shared §7 projection adapter.
use crate::*;
use openbindings_internal_schema as embedding;
use std::ops::Deref;
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
    pub(crate) fn supplied_bounded(
        supplied: &ExternalSchemas,
        maximum: usize,
        maximum_identity: usize,
        control: &dyn embedding::Control,
    ) -> Result<Self, embedding::Failure> {
        // The shared resolver reserves source zero for an OBI inventory. An
        // empty inventory contributes no schemas or identifiers. The supplied
        // resource remains a separate source and is entered at its own root.
        static EMPTY: std::sync::OnceLock<embedding::RawDocument> = std::sync::OnceLock::new();
        let empty =
            EMPTY.get_or_init(|| embedding::RawDocument::parse("{}").expect("empty inventory"));
        Ok(Self {
            inner: embedding::SchemaSpace::new_bounded(
                empty.clone(),
                embedding::SuppliedSet::supplied(supplied).clone(),
                maximum,
                maximum_identity,
                control,
            )?,
        })
    }
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
    fn references(&self, limits: ReferenceLimits, control: &dyn WorkControl) -> ReferenceReport {
        let report = self
            .inner
            .references_with_limit(limits.max_report_bytes, &|| control.is_cancelled());
        let references = report
            .references
            .into_iter()
            .map(|r| Reference {
                location: crate::schema::evaluation::original_location(r.location),
                resource_root: r
                    .resource_root
                    .map(crate::schema::evaluation::original_location),
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
            .collect();
        match report.limitation {
            Some(limitation) => ReferenceReport::Incomplete {
                retained: references,
                limitation: reference_refusal(limitation),
            },
            None => ReferenceReport::Complete { references },
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
    /// Resolved identity of an unavailable resource, when that is the cause.
    /// This is source-controlled URI text; inspection never fetches it.
    pub resource_uri: Option<String>,
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
        f.write_str(match self.reason {
            ReferenceRefusalReason::ResourceUnavailable => {
                "reference resource unavailable; supply the required external schema"
            }
            ReferenceRefusalReason::InvalidReference => "reference is invalid",
            ReferenceRefusalReason::MissingTarget => "reference target is missing",
            ReferenceRefusalReason::AmbiguousTarget => "reference target is ambiguous",
            ReferenceRefusalReason::IdentityConflict => "reference resource identities conflict",
            ReferenceRefusalReason::UnsupportedCapability => {
                "reference inspection requires an unsupported capability"
            }
            ReferenceRefusalReason::LimitExceeded => {
                "reference inspection reached a limit; inspect PreparationLimits"
            }
            ReferenceRefusalReason::Cancelled => "reference inspection was cancelled",
            ReferenceRefusalReason::ResolverFailure => "reference inspection could not complete",
        })
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
        _ => match f.code.as_ref() {
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
        code: f.code.into_owned(),
        message: f.message.into_owned(),
        location: f.location.map(crate::schema::evaluation::original_location),
        resource_uri: match f.query_failure.map(|cause| *cause) {
            Some(embedding::QueryFailure::ResourceUnavailable(uri)) => Some(uri),
            _ => None,
        },
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
/// Drafts with a missing or malformed `openbindings` declaration use the 0.2
/// schema positions and resolution rules, just as document validation does.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Reference {
    /// Original location of the reference keyword.
    pub location: SchemaOrigin,
    /// Original location of the schema beginning the resource containing this site.
    /// `None` exactly for the document resource. A supplied source's top resource
    /// uses that source's URI and an empty pointer, even without an `$id`.
    /// This is a location, not the resolved base. A conformant caller can use
    /// [`SchemaNode::resource_base`] for the base.
    pub resource_root: Option<SchemaOrigin>,
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
            .field("resource_root", &self.resource_root)
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Bounded original-context reference inspection, not conformance proof or successful schema preparation.
/// A missing or malformed `openbindings` declaration is inspected under 0.2's
/// rules, as specified for texts declaring no version. This assumption adds no
/// limitation: a complete traversal of such a draft is complete. Well-formed
/// unsupported declarations and duplicate member names still prevent inspection.
/// Inspection has no separate schema nesting bound beyond document admission.
/// By default retains at most 8 MiB of generated and source text in reference entries,
/// adjustable with [`ReferenceLimits`]:
/// all location, resource-root, target and refusal pointers, every origin's
/// source URI, keywords, spellings, refusal codes/messages and unavailable-resource
/// URIs. Text is counted in UTF-8 bytes; pointers through an unpaired name use
/// two bytes per UTF-16 unit. This is a text allowance, not total heap usage.
/// The whole entry is charged before its text is materialized or cloned. The
/// first entry that does not fit stops traversal and returns [`Self::Incomplete`]
/// with `LimitExceeded` and code `reference-report-bytes`, without a
/// location for the omitted entry.
/// The inventory is available only after choosing a completion case:
/// ```compile_fail,E0609
/// use openbindings::Document;
/// fn unchecked_rename(document: &Document) {
///     for reference in document.references().references { let _ = reference; }
/// }
/// ```
pub enum ReferenceReport {
    /// Traversal completed. Individual references can still be unresolved.
    Complete {
        /// All reference observations in traversal order.
        references: Vec<Reference>,
    },
    /// Traversal stopped before the full inventory was established.
    Incomplete {
        /// Observations retained before traversal stopped, in traversal order.
        retained: Vec<Reference>,
        /// Traversal-level cause, separate from per-reference refusals.
        limitation: ReferenceRefusal,
    },
}
use serde::{Serialize, ser::SerializeStruct};
impl Serialize for ReferenceReport {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Complete { references } => {
                let mut record = serializer.serialize_struct("ReferenceReport", 2)?;
                record.serialize_field("complete", &true)?;
                record.serialize_field("references", references)?;
                record.end()
            }
            Self::Incomplete {
                retained,
                limitation,
            } => {
                let mut record = serializer.serialize_struct("ReferenceReport", 3)?;
                record.serialize_field("complete", &false)?;
                record.serialize_field("retained", retained)?;
                record.serialize_field("limitation", limitation)?;
                record.end()
            }
        }
    }
}

/// Memory allowance for retained reference-report text, not total heap usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceLimits {
    /// Maximum generated and source text bytes in complete entries; default 8 MiB.
    pub max_report_bytes: usize,
}
impl Default for ReferenceLimits {
    fn default() -> Self {
        Self {
            max_report_bytes: 8 * 1024 * 1024,
        }
    }
}

impl Document {
    /// Inspect schema references in an admitted draft without claiming conformance.
    /// Opaque data is not traversed. No resource is fetched. A missing or malformed
    /// version uses 0.2's rules without adding a limitation. A well-formed
    /// unsupported version, duplicate member names, cancellation and traversal
    /// limits produce an incomplete report with a structured limitation.
    pub fn references(&self) -> ReferenceReport {
        self.references_with_limits(ReferenceLimits::default())
    }
    /// Inspect references with a caller-selected retained-text allowance.
    pub fn references_with_limits(&self, limits: ReferenceLimits) -> ReferenceReport {
        self.references_with_resources_and_limits(
            ExternalSchemas::default(),
            limits,
            &crate::control::NeverCancelled,
        )
    }
    /// Inspect the draft and supplied resources reached by its references,
    /// transitively, within the report's traversal limits. Unreached supplied
    /// resources are omitted. This is diagnostic evidence, not preparation.
    pub fn references_with_resources(
        &self,
        resources: ExternalSchemas,
        control: &dyn WorkControl,
    ) -> ReferenceReport {
        self.references_with_resources_and_limits(resources, ReferenceLimits::default(), control)
    }
    /// Inspect reached supplied resources with a caller-selected text allowance.
    /// Inventory and identity protection retain their existing defaults.
    pub fn references_with_resources_and_limits(
        &self,
        resources: ExternalSchemas,
        limits: ReferenceLimits,
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
        } else {
            None
        };
        if let Some((reason, code, message)) = refusal {
            return ReferenceReport::Incomplete {
                retained: Vec::new(),
                limitation: ReferenceRefusal {
                    reason,
                    code: code.into(),
                    message: message.into(),
                    location: None,
                    resource_uri: None,
                },
            };
        }
        inspect(self, &resources, limits, control)
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
#[derive(Clone, PartialEq, Eq, Serialize)]
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
impl std::fmt::Debug for ResolutionFailureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Undefined(defect) => return f.debug_tuple("Undefined").field(defect).finish(),
            Self::ResourceUnavailable { .. } => "ResourceUnavailable { .. }",
            Self::UnsupportedDialect { .. } => "UnsupportedDialect { .. }",
            Self::UnsupportedCapability { .. } => "UnsupportedCapability { .. }",
            Self::InvalidExternalSchema => "InvalidExternalSchema",
            Self::Limit { .. } => "Limit { .. }",
            Self::Cancelled => "Cancelled",
            Self::WrongEnvironment => "WrongEnvironment",
            Self::NotSchemaLocation => "NotSchemaLocation",
            Self::InvalidAnchorName => "InvalidAnchorName",
        };
        f.write_str(name)
    }
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
        f.debug_struct("ResolutionFailure")
            .field("kind", &self.kind)
            .field("has_origin", &self.origin.is_some())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ResolutionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match &self.kind {
            ResolutionFailureKind::Undefined(defect) => match defect {
                ReferenceDefect::MalformedReference => "schema reference is malformed",
                ReferenceDefect::MissingTarget => "schema reference target is missing",
                ReferenceDefect::NonSchemaTarget => "schema reference target is not a schema",
                ReferenceDefect::MissingAnchor => "schema reference anchor is missing",
                ReferenceDefect::AmbiguousAnchor => "schema reference anchor is ambiguous",
                ReferenceDefect::InvalidIdentifier => {
                    "schema reference has no usable base identifier"
                }
            },
            ResolutionFailureKind::ResourceUnavailable { .. } => {
                "schema reference resource is unavailable; supply the external schema"
            }
            ResolutionFailureKind::UnsupportedDialect { .. } => {
                "schema reference resolution requires an unsupported dialect"
            }
            ResolutionFailureKind::UnsupportedCapability { .. } => {
                "schema reference resolution requires an unsupported capability"
            }
            ResolutionFailureKind::InvalidExternalSchema => {
                "schema reference resource has invalid identity or dialect metadata"
            }
            ResolutionFailureKind::Limit { .. } => "schema reference resolution reached a limit",
            ResolutionFailureKind::Cancelled => "schema reference resolution was cancelled",
            ResolutionFailureKind::WrongEnvironment => {
                "schema reference node belongs to a different environment"
            }
            ResolutionFailureKind::NotSchemaLocation => {
                "schema reference position is outside the schema inventory"
            }
            ResolutionFailureKind::InvalidAnchorName => {
                "schema reference dynamic-anchor name is invalid"
            }
        })
    }
}
impl std::error::Error for ResolutionFailure {}
/// The static target and optional dynamic-anchor name. Dynamic scope selection
/// remains an evaluator responsibility; a pointer never opts into that lookup.
#[derive(Clone)]
pub struct ReferenceTarget {
    /// Original schema target.
    pub schema: SchemaNode,
    /// Plain dynamic-anchor name, when that declaration was selected by name.
    pub dynamic_anchor: Option<String>,
}
impl std::fmt::Debug for ReferenceTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceTarget")
            .field("schema", &self.schema)
            .field("has_dynamic_anchor", &self.dynamic_anchor.is_some())
            .finish_non_exhaustive()
    }
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
    report_limits: ReferenceLimits,
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
        Ok(space) => space.references(report_limits, control),
        Err(failure) => ReferenceReport::Incomplete {
            retained: Vec::new(),
            limitation: reference_refusal(failure),
        },
    }
}

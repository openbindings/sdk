//! Schema evaluation outcomes and optional evaluator extensions.
use crate::*;
use serde::Serialize;
use std::{fmt, sync::Arc};

/// Structural bounds and cooperative SDK budgets for schema preparation.
/// These are implementation limits, not OpenBindings conformance rules. They
/// do not count all dependency operations, allocator requests or callback work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparationLimits {
    /// Maximum inventoried schema positions, including supplied and packaged schemas.
    pub max_schema_nodes: usize,
    /// Cumulative conservative expanded-URI spelling bound. Relative identifiers
    /// are charged before dot removal, without materializing expanded strings.
    /// Several indexes retain spellings, so this is not a resident-memory limit.
    pub max_identity_bytes: usize,
    /// Shared budget at cooperative SDK work sites, including environment
    /// inventory, reference queries, semantic proofs and nested meta-validation.
    /// Default 100 million. A cache hit charges its checks rather than replaying completed work.
    /// This is neither an instruction count nor a deadline for dependency calls.
    pub max_sdk_work_units: usize,
    /// Monotone reservations at explicit SDK storage sites, including temporary
    /// and failed construction. Reservations are estimates, not allocator-wide
    /// accounting or a process memory ceiling. Dependency constructors require
    /// their evaluator's separate structural and representation limits.
    pub max_sdk_reservation_bytes: usize,
}
impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            max_schema_nodes: 100_000,
            max_identity_bytes: openbindings_internal_schema::DEFAULT_IDENTITY_BYTES,
            max_sdk_work_units: openbindings_internal_schema::preparation::DEFAULT_WORK_UNITS,
            max_sdk_reservation_bytes:
                openbindings_internal_schema::preparation::DEFAULT_ALLOCATION_BYTES,
        }
    }
}

/// Why preparation could not construct a validator. This judges no value.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "details", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PreparationFailureKind {
    /// This evaluator cannot prepare after an original-context resolution failure.
    /// An `Undefined` cause is local evidence, never a per-value judgment here.
    Resolution {
        /// Structured original lookup cause, with provenance on the outer failure.
        cause: ResolutionFailureKind,
    },
    /// The selected evaluator does not implement a required capability.
    UnsupportedCapability {
        /// Capability identifier.
        capability: String,
    },
    /// A configured work or representation bound was reached.
    Limit {
        /// Resource whose bound was reached.
        resource: String,
    },
    /// The caller cancelled the attempt.
    Cancelled,
    /// A reached external schema cannot be interpreted as a valid resource.
    InvalidExternalSchema,
    /// The implementation could not complete its work.
    Implementation {
        /// Diagnostic description, never a semantic verdict.
        message: String,
    },
}
/// A preparation refusal with original provenance when established.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PreparationFailure {
    /// Structured cause; callers need not parse diagnostic prose.
    #[serde(flatten)]
    pub kind: PreparationFailureKind,
    /// Original source location; never a private evaluator projection.
    pub origin: Option<SchemaOrigin>,
}
impl PreparationFailure {
    /// Construct a preparation failure without manufacturing source provenance.
    pub fn new(kind: PreparationFailureKind) -> Self {
        Self { kind, origin: None }
    }
    /// Attach an established original source coordinate.
    #[must_use]
    pub fn with_origin(mut self, origin: SchemaOrigin) -> Self {
        self.origin = Some(origin);
        self
    }
}
impl From<ResolutionFailure> for PreparationFailure {
    fn from(failure: ResolutionFailure) -> Self {
        let kind = match failure.kind {
            ResolutionFailureKind::Cancelled => PreparationFailureKind::Cancelled,
            ResolutionFailureKind::Limit { resource } => PreparationFailureKind::Limit { resource },
            ResolutionFailureKind::InvalidExternalSchema => {
                PreparationFailureKind::InvalidExternalSchema
            }
            cause => PreparationFailureKind::Resolution { cause },
        };
        Self {
            kind,
            origin: failure.origin,
        }
    }
}
impl fmt::Debug for PreparationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            PreparationFailureKind::Resolution { .. } => "Resolution",
            PreparationFailureKind::UnsupportedCapability { .. } => "UnsupportedCapability",
            PreparationFailureKind::Limit { .. } => "Limit",
            PreparationFailureKind::Cancelled => "Cancelled",
            PreparationFailureKind::InvalidExternalSchema => "InvalidExternalSchema",
            PreparationFailureKind::Implementation { .. } => "Implementation",
        };
        f.debug_struct("PreparationFailure")
            .field("kind", &kind)
            .field("has_origin", &self.origin.is_some())
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for PreparationFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Resolution { .. } => "Resolution { .. }",
            Self::UnsupportedCapability { .. } => "UnsupportedCapability { .. }",
            Self::Limit { .. } => "Limit { .. }",
            Self::Cancelled => "Cancelled",
            Self::InvalidExternalSchema => "InvalidExternalSchema",
            Self::Implementation { .. } => "Implementation { .. }",
        })
    }
}
// Only SDK-owned limit names select text; evaluator-defined names remain data.
pub(super) fn preparation_limit_text(resource: &str) -> &'static str {
    match resource {
        "schema-nodes" => "limit reached: raise PreparationLimits::max_schema_nodes",
        "schema-identity-bytes" => "limit reached: raise PreparationLimits::max_identity_bytes",
        "preparation-sdk-work-units" => {
            "limit reached: raise PreparationLimits::max_sdk_work_units"
        }
        "preparation-sdk-reservation-bytes" => {
            "limit reached: raise PreparationLimits::max_sdk_reservation_bytes"
        }
        _ => "limit reached; inspect the resource field and the evaluator's limits",
    }
}
impl fmt::Display for PreparationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("schema preparation failed: ")?;
        f.write_str(match &self.kind {
            PreparationFailureKind::Resolution { cause } => match cause {
                ResolutionFailureKind::Undefined(_) => "reference has a semantic defect",
                ResolutionFailureKind::ResourceUnavailable { .. } => {
                    "resource unavailable; supply the required external schema"
                }
                ResolutionFailureKind::UnsupportedDialect { .. } => "unsupported dialect",
                ResolutionFailureKind::UnsupportedCapability { .. } => {
                    "unsupported resolution capability"
                }
                ResolutionFailureKind::InvalidExternalSchema => "invalid external schema",
                ResolutionFailureKind::Limit { resource } => preparation_limit_text(resource),
                ResolutionFailureKind::Cancelled => "resolution cancelled",
                ResolutionFailureKind::WrongEnvironment => "schema belongs to another environment",
                ResolutionFailureKind::NotSchemaLocation => "requested location is not a schema",
                ResolutionFailureKind::InvalidAnchorName => "invalid dynamic-anchor name",
            },
            PreparationFailureKind::UnsupportedCapability { .. } => "unsupported capability",
            PreparationFailureKind::Limit { resource } => preparation_limit_text(resource),
            PreparationFailureKind::Cancelled => "cancelled",
            PreparationFailureKind::InvalidExternalSchema => "invalid external schema",
            PreparationFailureKind::Implementation { .. } => "implementation failure",
        })
    }
}
impl std::error::Error for PreparationFailure {}
impl From<Cancelled> for PreparationFailure {
    fn from(_: Cancelled) -> Self {
        Self {
            kind: PreparationFailureKind::Cancelled,
            origin: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Extensible cause of an unavailable schema verdict. Refusal is neither satisfaction nor failure; consumers should retain an unknown-cause fallback.
#[non_exhaustive]
pub enum NoVerdictReason {
    /// The evaluator does not implement a required capability.
    UnsupportedCapability,
    /// Sound preparation or evaluation could not be established, for example a potential non-progressing cycle. This does not prove semantic undefinedness.
    ConservativePreparation,
    /// A resource required by the evaluator's preparation or evaluation is absent;
    /// no network retrieval is attempted. The default evaluator retains typed
    /// original-environment lookup results. Missing content for an ordinary
    /// value-evaluation reference does not by itself refuse preparation or
    /// determine a value outcome: independent assertions may establish
    /// satisfaction or failure. Missing a required meta-schema can instead prevent
    /// schema admission and produce a preparation failure. This cause reports an
    /// inability to establish the outcome with the resources available.
    /// Use [`Document::references`] to inspect document-wide reference spellings
    /// and original keyword locations, then prepare again with supplied resources.
    /// That inventory is not an exact per-schema missing-resource list.
    /// Apply your disclosure policy before logging it.
    ResourceUnavailable,
    /// A work or representation limit prevented a verdict: one the application
    /// configured, or one fixed by the evaluator, such as the default
    /// evaluator's exact `multipleOf` arithmetic bound.
    LimitExceeded,
    /// The caller's cooperative cancellation was observed.
    Cancelled,
    /// The evaluator encountered an internal or operational failure; retry policy belongs to the caller.
    EvaluatorFailure,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// A keyword or schema in an original schema source, by resource and exact
/// pointer; generated evaluation-program identities are mapped back before
/// exposure. Line and column come from the source's holder:
/// [`ConformantDocument::locate`] for the document and [`ExternalSchemas::locate`] for a
/// supplied resource. This record contains coordinates, not source text; a
/// prepared schema may retain immutable original sources needed by its evaluator.
/// Serializes pointer fields as ordinary JSON strings, replacing unpaired UTF-16 units with U+FFFD; the stored [`Pointer`] stays exact for locating and editing.
///
/// Custom evaluators build one with [`Self::new`] or [`SchemaNode::origin`].
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct SchemaOrigin {
    /// Original source's supplied retrieval URI, or the published fixed-schema URI
    /// for an OBI-02 [`Finding`]. `None` denotes the OpenBindings
    /// document. This is not the resource base established by a nested `$id`.
    pub source_uri: Option<String>,
    /// Exact RFC 6901 pointer from the source's root; an empty pointer denotes the root. Escape for presentation without changing `~0`/`~1` semantics.
    pub pointer: Pointer,
}
impl SchemaOrigin {
    /// A location in the document (`source_uri` `None`) or in the supplied resource
    /// with this URI. [`SchemaNode::origin`] builds one for a schema node an
    /// evaluator was given.
    pub fn new(source_uri: Option<String>, pointer: Pointer) -> Self {
        Self {
            source_uri,
            pointer,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize)]
/// Structured explanation for why no schema verdict was established. Custom
/// evaluators build one with [`Self::new`] and [`Self::with_location`]. Later
/// versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct NoVerdict {
    /// Extensible broad refusal category.
    pub reason: NoVerdictReason,
    /// Evaluator-specific stable detail code; use with the broad reason for branching.
    /// Static text is borrowed; dynamic text is owned by this record.
    pub code: std::borrow::Cow<'static, str>,
    /// Static or owned explanatory text. Custom evaluators should avoid echoing source or instance data.
    pub message: std::borrow::Cow<'static, str>,
    /// Original schema location when known; no generated or guessed location is substituted.
    pub location: Option<SchemaOrigin>,
}
impl std::fmt::Debug for NoVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Any evaluator chooses the code, so only the SDK's broad reason shows.
        f.debug_struct("NoVerdict")
            .field("reason", &self.reason)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}
impl NoVerdict {
    /// Construct an unlocated refusal; add an original schema location with
    /// [`Self::with_location`] when original-source evidence is available.
    /// String literals require no allocation; owned strings are moved. Make a
    /// nonstatic borrowed string owned before passing it to this constructor.
    pub fn new(
        reason: NoVerdictReason,
        code: impl Into<std::borrow::Cow<'static, str>>,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self {
            reason,
            code: code.into(),
            message: message.into(),
            location: None,
        }
    }
    /// Attach an established original schema location.
    #[must_use]
    pub fn with_location(mut self, location: SchemaOrigin) -> Self {
        self.location = Some(location);
        self
    }
}
impl fmt::Display for NoVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.reason {
            NoVerdictReason::UnsupportedCapability => {
                "schema evaluation requires an unsupported capability"
            }
            NoVerdictReason::ConservativePreparation => {
                "schema evaluation could not establish sound preparation"
            }
            NoVerdictReason::ResourceUnavailable => {
                "schema evaluation requires an unavailable resource"
            }
            NoVerdictReason::LimitExceeded => {
                "schema evaluation reached a limit; inspect the evaluator's limits"
            }
            NoVerdictReason::Cancelled => "schema evaluation was cancelled",
            NoVerdictReason::EvaluatorFailure => "schema evaluation could not complete",
        })
    }
}
impl std::error::Error for NoVerdict {}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// An established undefined result under the schema evaluation rules. Custom
/// evaluators build one with [`Self::new`] and [`Self::with_location`] when
/// semantic provenance and disagreeing completions are proved. Later versions
/// may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct UndefinedResult {
    /// Stable evaluator-defined detail code.
    pub code: String,
    /// Explanatory text; custom evaluators should avoid echoing source or instance data.
    pub message: String,
    /// Original schema location when known; no generated or guessed location is substituted.
    pub location: Option<SchemaOrigin>,
}
impl UndefinedResult {
    /// Construct an undefined result without an original schema location.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            location: None,
        }
    }
    /// Attach an established original schema location.
    #[must_use]
    pub fn with_location(mut self, location: SchemaOrigin) -> Self {
        self.location = Some(location);
        self
    }
}
impl fmt::Debug for UndefinedResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self.code.as_str() {
            "static-reference-defect" => self.code.as_str(),
            _ => "<redacted>",
        };
        f.debug_struct("UndefinedResult")
            .field("code", &code)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
/// Optional schema facts, also used by OBI-02 [`Finding`] details from the
/// published fixed schema. Treat application-schema source operands as sensitive; none contain a
/// rejected instance value. Numeric bounds and enum choices are exact JSON token
/// strings. Every variant contains complete facts. The default evaluator includes
/// these in explanations when representable within its diagnostic allowance.
/// `Debug` redacts facts. Serializing whole problems sends schema facts; apps
/// with a private contract should forward only `instance` and `code` publicly.
pub enum ValueProblemDetails {
    /// Complete expected types from the fixed JSON type vocabulary.
    Type {
        /// Fixed JSON type names, never arbitrary schema strings.
        expected: Vec<String>,
    },
    /// One established missing member, verified against the original required array.
    Required {
        /// Source-controlled member name; the instance pointer stays at the existing parent.
        member: String,
    },
    /// Exact numeric bound; the problem code supplies comparator and inclusivity.
    NumericBound {
        /// Original JSON numeric token, without floating-point conversion.
        bound: String,
    },
    /// Exact length/count bound; the problem code supplies unit and direction.
    SizeBound {
        /// Original JSON numeric token.
        bound: String,
    },
    /// Complete enum choices; never a partial prefix.
    Enum {
        /// Exact original JSON token text for every choice, including source-controlled data.
        choices: Vec<String>,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// One established instance failure, located in the value and in the schema.
/// Custom evaluators build one with [`Self::new`] and [`Self::with_details`].
/// Later versions may add fields: read them by name and match with `..`.
/// Serializes pointer fields as ordinary JSON strings, replacing unpaired UTF-16 units with U+FFFD; the stored [`Pointer`] stays exact for locating and editing.
/// ```compile_fail,E0639
/// use openbindings::*;
/// fn build(instance: Pointer, schema: SchemaOrigin) -> ValueProblem {
///     ValueProblem { instance, schema, code: "type".into(), message: "wrong type".into(),
///         details: None }
/// }
/// ```
/// ```
/// use openbindings::*;
/// fn build(instance: Pointer, schema: SchemaOrigin) -> ValueProblem {
///     ValueProblem::new(instance, schema, "type", "wrong type")
///         .with_details(ValueProblemDetails::Type { expected: vec!["integer".into()] })
/// }
/// fn read(problem: &ValueProblem) {
///     let ValueProblem { instance, schema, code, message, details, .. } = problem;
/// }
/// ```
#[non_exhaustive]
pub struct ValueProblem {
    /// Exact RFC 6901 pointer to the failing node, from the value passed to
    /// validation (the instance's root, also when that value is a subtree of
    /// a larger text). The caller holds the value, so it finds the byte offset,
    /// line and column when asked: [`JsonValue::locate`].
    pub instance: Pointer,
    /// The failing keyword in its original schema source, from that
    /// document's or resource's root; [`ConformantDocument::locate`] and
    /// [`ExternalSchemas::locate`] find its line and column. When an evaluator
    /// cannot map a keyword to the original source, this names the selected
    /// schema rather than a guessed keyword.
    pub schema: SchemaOrigin,
    /// Stable evaluator-defined problem code, commonly the failed JSON Schema keyword.
    pub code: String,
    /// Explanatory text; custom evaluators should avoid exposing instance values or source-controlled strings by default.
    pub message: String,
    /// Schema facts established by the failed keyword: missing member, expected
    /// types, exact numeric/size bound, or complete enum choices. `None` means no
    /// matching variant or unavailable facts. The default evaluator marks
    /// diagnostics incomplete if a fact cannot fit or be represented exactly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ValueProblemDetails>,
}
impl std::fmt::Debug for ValueProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Fixed arms of the pinned evaluator's ValidationErrorKind::keyword;
        // recognize exact spellings only, keeping arbitrary custom codes private.
        let code = match self.code.as_str() {
            "additionalItems"
            | "additionalProperties"
            | "anyOf"
            | "pattern"
            | "const"
            | "contains"
            | "contentEncoding"
            | "contentMediaType"
            | "enum"
            | "exclusiveMaximum"
            | "exclusiveMinimum"
            | "falseSchema"
            | "format"
            | "maxItems"
            | "maximum"
            | "maxLength"
            | "maxProperties"
            | "minItems"
            | "minimum"
            | "minLength"
            | "minProperties"
            | "multipleOf"
            | "not"
            | "oneOf"
            | "propertyNames"
            | "required"
            | "type"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "uniqueItems"
            | "$ref" => self.code.as_str(),
            _ => "<redacted>",
        };
        f.debug_struct("ValueProblem")
            .field("code", &code)
            .field("instance", &self.instance)
            .field("schema", &self.schema)
            .field("details", &self.details)
            .finish_non_exhaustive()
    }
}
impl ValueProblem {
    /// A problem without schema facts; add them with [`Self::with_details`].
    /// `instance` is the failing node's pointer from the validated value
    /// (`Pointer::from_str` parses one, [`JsonRef::at_pointer`] checks it);
    /// [`SchemaNode::origin`] at preparation names `schema`.
    pub fn new(
        instance: Pointer,
        schema: SchemaOrigin,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            instance,
            schema,
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
    /// Attach established schema facts.
    #[must_use]
    pub fn with_details(mut self, details: ValueProblemDetails) -> Self {
        self.details = Some(details);
        self
    }
}
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
/// Closed result partition for the selected schema and instance: satisfies, fails, undefined, or no verdict. No value outcome proves that the surrounding OpenBindings document conforms.
///
/// Serializes its variant in `status`. Pointer fields are ordinary JSON strings, replacing unpaired UTF-16 units with U+FFFD; stored pointers stay exact.
#[must_use = "inspect the outcome before continuing"]
pub enum ValueOutcome {
    /// The instance was established to satisfy the selected schema.
    Satisfies,
    /// The instance was established to fail the selected schema; diagnostic completeness is separate from the verdict.
    Fails {
        /// Value-free explanation of the established failure.
        message: String,
        /// Retained actual failing instance locations; diagnostics may be bounded after failure is established.
        problems: Vec<ValueProblem>,
        /// Whether selected diagnostics, including schema facts, completed. False does not weaken the established failure verdict.
        problems_complete: bool,
    },
    /// Whether the value satisfies the selected schema is established to be undefined.
    Undefined {
        /// Explanation of the undefined result, including an original schema location when known.
        detail: UndefinedResult,
    },
    /// The evaluator could not establish a result.
    NoVerdict {
        /// Structured cause of refusal, including an original schema location when known.
        detail: NoVerdict,
    },
}
impl std::fmt::Debug for ValueOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Satisfies => f.write_str("Satisfies"),
            Self::Fails {
                problems,
                problems_complete,
                ..
            } => f
                .debug_struct("Fails")
                .field("problems", problems)
                .field("problems_complete", problems_complete)
                .finish_non_exhaustive(),
            Self::NoVerdict { detail } => {
                f.debug_struct("NoVerdict").field("detail", detail).finish()
            }
            Self::Undefined { detail } => {
                f.debug_struct("Undefined").field("detail", detail).finish()
            }
        }
    }
}

// It also reuses the schema index the revision built when it was validated,
// and what that validation established about its schemas.
impl openbindings_internal_schema::IndexedDocument for ConformantDocument {
    fn indexed(&self) -> openbindings_internal_schema::RawDocument {
        self.document().indexed()
    }
}

impl ConformantDocument {
    /// Locate a schema location in this document: the node its pointer names,
    /// with line and column in the document's text. `None` for a location in a
    /// supplied resource, or a pointer that names no node in this revision.
    /// Line and column are computed on this call from the original text.
    pub fn locate(&self, at: &SchemaOrigin) -> Option<SourceLocation> {
        if at.source_uri.is_some() {
            return None;
        }
        self.document().value().locate(&at.pointer)
    }
}

/// Borrowed inputs for one preparation attempt. The returned validator must
/// own what it needs; a request does not lend it a completed caller's lifetime.
#[derive(Clone, Copy)]
pub struct EvaluationRequest<'a> {
    /// Selected contained schema and its immutable document context.
    pub schema: &'a Schema,
    /// Explicit external schemas; no URI authorizes acquisition.
    pub external_schemas: &'a ExternalSchemas,
}
impl fmt::Debug for EvaluationRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvaluationRequest").finish_non_exhaustive()
    }
}
pub(crate) fn original_location(at: openbindings_internal_schema::Location) -> SchemaOrigin {
    SchemaOrigin {
        source_uri: at.resource,
        pointer: at.pointer,
    }
}
/// Optional evaluator implementation. Its validator type is concrete and may
/// contain local state such as `Rc`; neither trait imposes thread-safety bounds.
/// An evaluator owns compilation strategy, value evaluation and dynamic scope.
/// Core supplies original schemas and reference interpretation through
/// [`EvaluationRequest::environment`], without a vendor projection API.
///
/// ```
/// use openbindings::*;
/// use std::rc::Rc;
/// struct TrueOnly;
/// struct Ready(Rc<()>);
/// impl SchemaEvaluator for TrueOnly {
///     type Validator = Ready;
///     fn prepare(&self, request: EvaluationRequest<'_>, _: &PreparationLimits,
///         control: &dyn WorkControl) -> Result<Ready, PreparationFailure> {
///         control.check()?;
///         if request.schema.json().view().as_bool() == Some(true) {
///             Ok(Ready(Rc::new(())))
///         } else {
///             Err(PreparationFailure::new(PreparationFailureKind::UnsupportedCapability {
///                 capability: "true-only".into(),
///             }).with_origin(request.schema.location()))
///         }
///     }
/// }
/// impl SchemaValidator for Ready {
///     fn evaluate(&self, _: &JsonValue, _: Detail, control: &dyn WorkControl) -> ValueOutcome {
///         match control.check() {
///             Ok(()) => ValueOutcome::Satisfies,
///             Err(error) => ValueOutcome::NoVerdict { detail: error.into() },
///         }
///     }
/// }
/// let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}}}"#)?;
/// let proof = document.validate()?.conformant().unwrap();
/// let schema = proof.resolve_operation("run").unwrap().input().unwrap();
/// let prepared = schema.prepare(&TrueOnly, &ExternalSchemas::default())?;
/// drop((schema, proof, document));
/// assert!(matches!(prepared.evaluate(&JsonValue::null()), ValueOutcome::Satisfies));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait SchemaEvaluator {
    /// Owned compiled state. Thread-safety and cloning follow this type.
    type Validator: SchemaValidator + 'static;
    /// Prepare the selected schema under explicit work bounds. No instance is
    /// judged by this call; a preparation refusal is not a value outcome.
    ///
    /// # Errors
    /// Returns a structured capability, resource, limit, cancellation, or
    /// implementation failure when this evaluator cannot prepare the request.
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure>;
}
/// How much failure detail one validation call asks for. The verdict does not
/// depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detail {
    /// The verdict only. A failure carries no problems and `problems_complete`
    /// is false.
    Outcome,
    /// The verdict with problems and available schema facts that explain a failure.
    Explain,
}
/// Owned validator returned by a [`SchemaEvaluator`]. Repeated calls must preserve caller input and retain no accidental borrow of a completed request. Only claim established semantic verdicts; bound diagnostic work separately and report incompleteness.
pub trait SchemaValidator {
    /// Return only established verdicts. Resource/capability/work failures are NoVerdict.
    ///
    /// The requested [`Detail`] changes only what a failure carries. With
    /// [`Detail::Outcome`], problems are empty and `problems_complete` is false.
    /// With [`Detail::Explain`], a failure carries at least one problem unless
    /// detail work was cut short or found no problem it can report in the
    /// original sources, in which case `problems_complete` is false. Cutting
    /// detail work short, by cancellation or an exhausted budget, never turns an
    /// established failure into another outcome.
    ///
    /// `Detail` may gain variants. Answer one you do not recognize as for
    /// [`Detail::Outcome`], since a failure without problems and with
    /// `problems_complete` false is a truthful answer to every request.
    fn evaluate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome;
}
/// An owned prepared schema. `Clone`, `Send` and `Sync` are available exactly
/// when the concrete validator supports them. No trait object is imposed.
#[derive(Clone)]
pub struct PreparedSchema<V: SchemaValidator> {
    pub(crate) validator: V,
}
impl<V: SchemaValidator> fmt::Debug for PreparedSchema<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedSchema { .. }")
    }
}
impl<V: SchemaValidator> PreparedSchema<V> {
    /// Validate an admitted exact value, distinguishing satisfies, fails, undefined
    /// (the specification gives the value no defined validity) and no-verdict (the
    /// evaluator could not answer). For ordinary Rust data first use [`JsonValue::from_serializable`];
    /// for exact JSON text/bytes use [`JsonValue::parse`]. Admission failure is
    /// separate from validation.
    ///
    /// This is the cheap check: a failure carries no problems and
    /// `problems_complete` is false. Call [`Self::explain`] with the same value
    /// to learn why it fails.
    pub fn evaluate(&self, value: &JsonValue) -> ValueOutcome {
        self.evaluate_with_control(value, &crate::control::NeverCancelled)
    }
    /// Validate with cooperative cancellation. Cancellation observed before a
    /// verdict is established yields no-verdict and leaves this owner usable with
    /// a fresh healthy control. A verdict the default evaluator establishes
    /// before it observes cancellation stands, whether the value satisfies or
    /// fails.
    /// This is not a preemptive wall-clock deadline; applications schedule work.
    pub fn evaluate_with_control(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        self.check(value, Detail::Outcome, control)
    }
    /// Validate and explain a failure with problems that name where and why the
    /// value fails. The verdict is the one [`Self::evaluate`] returns.
    ///
    /// Explanation costs more than the cheap check, and its cost is bounded
    /// by the evaluator, not the SDK. The default evaluator retains a bounded
    /// collection of failures and materializes their original schema and instance
    /// locations after establishing failure, ordered by instance then schema.
    /// Tied locations retain required-array order. Messages use fixed SDK text;
    /// `details` contains missing members, expected types, exact numeric/size
    /// bounds and complete enum choices from the original schema. Other keywords
    /// have no details; pattern text stays out. Debug output redacts all facts.
    /// Serializing whole problems sends schema facts. An app with a private
    /// contract should forward only `instance` and `code` in public responses.
    /// Exhausting the remaining work or storage allowance keeps the failure but
    /// may omit problems or facts. Facts that cannot be represented exactly are
    /// also omitted. Use [`Self::evaluate`] when no explanation is needed.
    /// Failure diagnostics can be incomplete; inspect `problems_complete` rather
    /// than assuming every failed keyword is reported.
    pub fn explain(&self, value: &JsonValue) -> ValueOutcome {
        self.explain_with_control(value, &crate::control::NeverCancelled)
    }
    /// Explain with cooperative cancellation. Cancellation before a verdict yields
    /// no-verdict. Cancellation of the explanation after a verdict does not weaken
    /// an established failure: it keeps `Fails` with `problems_complete` false.
    pub fn explain_with_control(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        self.check(value, Detail::Explain, control)
    }
    // Inlined into each public call, so the cheap check costs no extra frame.
    #[inline]
    fn check(
        &self,
        value: &JsonValue,
        requested: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        if value.has_duplicate_names() {
            return ValueOutcome::NoVerdict {
                detail: NoVerdict::new(
                    NoVerdictReason::UnsupportedCapability,
                    "duplicate-instance-members",
                    "a value with repeated JSON member names has no unambiguous interpretation",
                ),
            };
        }
        self.validator.evaluate(value, requested, control)
    }
}

impl std::fmt::Debug for ValueProblemDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueProblemDetails")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SchemaOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaOrigin").finish_non_exhaustive()
    }
}

macro_rules! forward_validator {
    ($owner:ident) => {
        impl<V: SchemaValidator + ?Sized> SchemaValidator for $owner<V> {
            fn evaluate(
                &self,
                value: &JsonValue,
                detail: Detail,
                control: &dyn WorkControl,
            ) -> ValueOutcome {
                (**self).evaluate(value, detail, control)
            }
        }
        impl<E: SchemaEvaluator + ?Sized> SchemaEvaluator for $owner<E> {
            type Validator = E::Validator;
            fn prepare(
                &self,
                request: EvaluationRequest<'_>,
                limits: &PreparationLimits,
                control: &dyn WorkControl,
            ) -> Result<Self::Validator, PreparationFailure> {
                (**self).prepare(request, limits, control)
            }
        }
    };
}
use std::rc::Rc;
forward_validator!(Arc);
forward_validator!(Rc);
forward_validator!(Box);

// An internal space check can refuse an invalid external schema as
// `ConservativePreparation` with code `invalid-schema`; only internal entry
// points reach it, and public preparation refuses that schema as
// `InvalidExternalSchema` first. Map it here if a public path ever reaches it.
#[doc(hidden)]
impl From<openbindings_internal_schema::Failure> for PreparationFailure {
    fn from(failure: openbindings_internal_schema::Failure) -> Self {
        use openbindings_internal_schema::FailureReason as F;
        let origin = failure.location.map(original_location);
        let kind = match failure.reason {
            F::Cancelled => PreparationFailureKind::Cancelled,
            F::LimitExceeded => PreparationFailureKind::Limit {
                resource: failure.code.into_owned(),
            },
            _ if failure.query_failure.is_some() => {
                let cause = crate::schema::references::query_kind(
                    *failure.query_failure.expect("checked resolution cause"),
                );
                return ResolutionFailure {
                    kind: cause,
                    origin,
                }
                .into();
            }
            F::UnsupportedCapability | F::ConservativePreparation => {
                PreparationFailureKind::UnsupportedCapability {
                    capability: failure.code.into_owned(),
                }
            }
            F::ResourceUnavailable => PreparationFailureKind::Implementation {
                message: "the resolver omitted the unavailable resource identity".into(),
            },
            F::EvaluatorFailure => PreparationFailureKind::Implementation {
                message: failure.message.into_owned(),
            },
        };
        Self { kind, origin }
    }
}

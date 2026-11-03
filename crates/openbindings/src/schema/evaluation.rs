//! Schema evaluation outcomes and optional evaluator extensions.
use crate::*;
use serde::Serialize;
use std::{fmt, sync::Arc};

/// Bounds for constructing an original schema environment. These are SDK work
/// limits, not OpenBindings conformance rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparationLimits {
    /// Maximum inventoried schema positions, including supplied and packaged schemas.
    pub max_schema_nodes: usize,
    /// Cumulative conservative expanded-URI spelling bound. Relative identifiers
    /// are charged before dot removal, without materializing expanded strings.
    /// Several indexes retain spellings, so this is not a resident-memory limit.
    pub max_identity_bytes: usize,
}
impl Default for PreparationLimits {
    fn default() -> Self {
        Self {
            max_schema_nodes: 100_000,
            max_identity_bytes: openbindings_internal_schema::DEFAULT_IDENTITY_BYTES,
        }
    }
}

/// Why preparation could not construct a validator. This judges no value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreparationFailureKind {
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
#[derive(Clone, PartialEq, Eq)]
pub struct PreparationFailure {
    /// Structured cause; callers need not parse diagnostic prose.
    pub kind: PreparationFailureKind,
    /// Original source location; never a private evaluator projection.
    pub origin: Option<SchemaOrigin>,
}
impl fmt::Debug for PreparationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparationFailure").finish_non_exhaustive()
    }
}
impl fmt::Display for PreparationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("schema preparation did not complete")
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
    /// no network retrieval is attempted. The default evaluator prepares an empty
    /// stand-in for a missing resource and returns this cause during validation
    /// when an instance's evaluation reaches it; a reference to a fragment of a
    /// missing resource refuses at preparation. This does not prove that missing
    /// content is necessary to the instance's semantic verdict.
    /// Use [`Document::references`] to inspect document-wide reference spellings
    /// and original keyword locations, then prepare again with supplied resources.
    /// That inventory is not an exact per-value-contract missing-resource list.
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
/// supplied resource. A prepared value contract keeps no source text. Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
///
/// Custom evaluators build one with [`Self::new`] or [`SchemaRequest::location`].
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct SchemaOrigin {
    /// Original source's supplied retrieval URI, or `None` for the OpenBindings
    /// document. This is not the resource base established by a nested `$id`.
    pub source_uri: Option<String>,
    /// Exact RFC 6901 pointer from the source's root; an empty pointer denotes the root. Escape for presentation without changing `~0`/`~1` semantics.
    pub pointer: Pointer,
}
impl SchemaOrigin {
    /// A location in the document (`resource` `None`) or in the supplied resource
    /// with this URI. [`SchemaRequest::location`] builds one for a schema node an
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
    pub code: String,
    /// Explanatory text. Custom evaluators own its content and should avoid echoing source or instance data.
    pub message: String,
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
    pub fn new(
        reason: NoVerdictReason,
        code: impl Into<String>,
        message: impl Into<String>,
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
        f.write_str(&self.message)
    }
}
impl std::error::Error for NoVerdict {}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// An established undefined result under the value-contract rules. Custom
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
/// Optional schema facts. Treat source operands as sensitive; none contain a
/// rejected instance value. Numeric bounds and enum choices are exact JSON token
/// strings. Every variant contains complete facts.
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
/// Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
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
    /// Optional schema facts. None means disabled or unavailable.
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
    /// [`SchemaRequest::location`] at preparation names `schema`.
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
/// Serializes as JSON text naming the variant in `status`, the field every TypeScript result discriminates on; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
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
        /// Whether selected diagnostics, including requested details, completed. False does not weaken the established failure verdict.
        problems_complete: bool,
    },
    /// Whether the value satisfies its value contract is established to be undefined.
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
    /// Positions are computed on this call; a prepared value contract keeps none.
    pub fn locate(&self, at: &SchemaOrigin) -> Option<SourceLocation> {
        if at.source_uri.is_some() {
            return None;
        }
        self.document().value().locate(&at.pointer)
    }
}

/// Original snapshots and an entry point. Evaluators may use the checked projection
/// helper or evaluate this request directly; generated identifiers are never API identity.
#[derive(Clone)]
pub struct SchemaRequest {
    pub(crate) document: ConformantDocument,
    pub(crate) resources: ExternalSchemas,
    pub(crate) entry: JsonValue,
}
impl std::fmt::Debug for SchemaRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRequest").finish_non_exhaustive()
    }
}

impl SchemaRequest {
    /// Borrow the conformant document revision owning this normative value contract.
    pub fn document(&self) -> &ConformantDocument {
        &self.document
    }
    /// Borrow the immutable explicit resources for this preparation; evaluators must not substitute global URI caches.
    pub fn supplied_resources(&self) -> &ExternalSchemas {
        &self.resources
    }
    /// Borrow the selected exact input/output schema in its original context.
    pub fn entry(&self) -> &JsonValue {
        &self.entry
    }
    /// Allocate the selected schema's original document location.
    pub fn entry_location(&self) -> SchemaOrigin {
        SchemaOrigin {
            source_uri: None,
            pointer: openbindings_internal_json::backend::exact_pointer(self.entry.view()),
        }
    }
    /// Allocate the original location of a schema node from this request's
    /// document (such as one inside [`Self::entry`]) or from one of its
    /// supplied resources, or `None` for a node from another source, including
    /// a projected [`EvaluationProgram`] resource. Builds only the pointer.
    pub fn location(&self, node: JsonRef<'_>) -> Option<SchemaOrigin> {
        use openbindings_internal_json::backend::{exact_pointer, same_source};
        let resource = if same_source(node, self.document.document().value()) {
            None
        } else {
            Some(
                self.resources
                    .iter()
                    .find(|resource| same_source(node, &resource.document))?
                    .uri
                    .clone(),
            )
        };
        Some(SchemaOrigin {
            source_uri: resource,
            pointer: exact_pointer(node),
        })
    }
    /// Build a finite evaluator projection with private resource identities, original-location mapping and cooperative cancellation. Refuses unsupported/ambiguous/unavailable references instead of retrieving data.
    ///
    /// The first projection for a document revision and resource set assembles
    /// their resolution space using the set's identity index; later preparations
    /// with the same live pair reuse it, and dropping the set or every owner of
    /// the revision releases it.
    ///
    /// # Errors
    /// Returns a refusal if references cannot be resolved soundly, a required capability or resource is unavailable, work limits are exceeded, or cancellation is observed. A refusal is not an instance failure.
    pub fn evaluation_program(
        &self,
        control: &dyn WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        control.check()?;
        let limits = PreparationLimits::default();
        let space = self
            .document
            .resolution_bounded(
                &self.resources,
                limits.max_schema_nodes,
                limits.max_identity_bytes,
                control,
            )
            .map_err(projection_failure)?;
        let entry = space
            .document_node(&self.entry)
            .expect("preparation admits only indexed document schema positions");
        space.program(entry, control)
    }
}
/// A closed §7 schema projection with original provenance. Generated identifiers
/// are private to this program and are not original source identities.
#[derive(Clone)]
pub struct EvaluationProgram {
    entry_uri: String,
    resources: Vec<ExternalSchema>,
    projection: openbindings_internal_schema::Program,
}
impl EvaluationProgram {
    /// Borrow the private absolute URI selecting the projected entry.
    pub fn entry_uri(&self) -> &str {
        &self.entry_uri
    }
    /// Borrow the immutable projected resources with their original provenance.
    pub fn resources(&self) -> &[ExternalSchema] {
        &self.resources
    }
    /// Map a generated keyword URI to an established original source location.
    pub fn original_location(&self, uri: &str) -> Option<SchemaOrigin> {
        self.projection
            .original_location(uri)
            .map(original_location)
    }
}
impl EvaluationProgram {
    pub(crate) fn from_projection(mut projection: openbindings_internal_schema::Program) -> Self {
        let resources = std::mem::take(&mut projection.resources)
            .into_iter()
            .map(|r| ExternalSchema {
                uri: r.uri,
                document: r.document,
            })
            .collect();
        Self {
            entry_uri: projection.entry_uri.clone(),
            resources,
            projection,
        }
    }
}
pub(crate) fn original_location(at: openbindings_internal_schema::Location) -> SchemaOrigin {
    SchemaOrigin {
        source_uri: at.resource,
        pointer: at.pointer,
    }
}
pub(crate) fn projection_failure(f: openbindings_internal_schema::Failure) -> NoVerdict {
    use openbindings_internal_schema::FailureReason as F;
    let reason = match f.reason {
        F::UnsupportedCapability => NoVerdictReason::UnsupportedCapability,
        F::ConservativePreparation => NoVerdictReason::ConservativePreparation,
        F::ResourceUnavailable => NoVerdictReason::ResourceUnavailable,
        F::LimitExceeded => NoVerdictReason::LimitExceeded,
        F::Cancelled => NoVerdictReason::Cancelled,
        F::EvaluatorFailure => NoVerdictReason::EvaluatorFailure,
    };
    NoVerdict {
        reason,
        code: f.code,
        message: f.message,
        location: f.location.map(original_location),
    }
}
/// Thread-safe preparation extension point. Respect immutable original context, explicit resources and [`WorkControl`]; perform no implicit I/O. Return a refusal when required capabilities, soundness or limits prevent preparation. Prepared objects must own everything they need after the request is dropped.
///
/// A deliberately small evaluator can truthfully support just the boolean `true`
/// schema. Unsupported requests remain refusals; no source is fetched.
///
/// ```
/// use openbindings::*;
/// use std::sync::Arc;
/// struct TrueOnly;
/// impl SchemaEvaluator for TrueOnly {
///     fn prepare(&self, request: &SchemaRequest, control: &dyn WorkControl)
///         -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
///         control.check()?;
///         if request.entry().view().as_bool() == Some(true) {
///             Ok(Arc::new(TrueOnly))
///         } else {
///             Err(NoVerdict::new(
///                 NoVerdictReason::UnsupportedCapability,
///                 "true-only",
///                 "this evaluator supports only the true schema",
///             )
///             .with_location(request.entry_location()))
///         }
///     }
/// }
/// impl PreparedSchema for TrueOnly {
///     fn validate(&self, _: &JsonValue, _: Detail, control: &dyn WorkControl) -> ValueOutcome {
///         match control.check() {
///             Ok(()) => ValueOutcome::Satisfies,
///             Err(detail) => ValueOutcome::NoVerdict { detail: detail.into() },
///         }
///     }
/// }
/// let document = Document::parse(
///     r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}}}"#)?;
/// let document = document.validate()?.conformant().expect("conformant document");
/// let operation = document.resolve_operation("run").expect("declared operation");
/// let contract = operation.input().expect("declared input").prepare(&TrueOnly)?;
/// assert!(matches!(contract.validate(&JsonValue::null()), ValueOutcome::Satisfies));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait SchemaEvaluator: Send + Sync {
    /// Prepare this selected schema only, borrowing the request and control for the call. Return a shareable immutable [`PreparedSchema`] or truthful [`NoVerdict`]; never turn preparation refusal into instance failure.
    ///
    /// # Errors
    /// Returns a refusal if references cannot be resolved soundly, a required capability or resource is unavailable, work limits are exceeded, or cancellation is observed. A refusal is not an instance failure.
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict>;
}
/// How much failure detail one validation call asks for. The verdict does not
/// depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detail {
    /// The verdict only. A failure carries no problems and `problems_complete`
    /// is false.
    Outcome,
    /// The verdict with problems that explain a failure.
    Explain,
}
/// Thread-safe retained validator returned by a [`SchemaEvaluator`]. Repeated calls must preserve caller input and retain no accidental borrow of a completed request. Only claim established semantic verdicts; bound diagnostic work separately and report incompleteness.
pub trait PreparedSchema: Send + Sync {
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
    fn validate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome;
}
/// A prepared value contract that retains its required compiled state
/// independently of the document, resources and evaluator. It is
/// `Clone + Send + Sync + 'static`; cloning shares immutable state, and dropping
/// the final owner releases it, without promising lower RSS.
#[derive(Clone)]
pub struct PreparedValueContract {
    pub(crate) schema: Arc<dyn PreparedSchema>,
}
impl fmt::Debug for PreparedValueContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedValueContract { .. }")
    }
}
impl PreparedValueContract {
    /// Admit exact JSON text or bytes under [`JsonLimits::default`], then validate.
    /// Every [`ValueOutcome`] is returned as `Ok`, including undefined and
    /// no-verdict. Duplicate members are retained by admission and yield
    /// no-verdict during validation. Number tokens are never rounded.
    ///
    /// Use [`JsonValue::parse_with_limits`] and [`Self::validate`] for custom
    /// admission limits or to retain exact values for repeated checks. Use
    /// [`Self::validate_with_control`] for cooperative validation cancellation.
    ///
    /// # Errors
    /// Returns [`InputError`] only when JSON admission fails, including invalid
    /// UTF-8, a byte order mark, malformed JSON or an exceeded input limit.
    pub fn validate_json(&self, input: impl AsRef<[u8]>) -> Result<ValueOutcome, InputError> {
        // A limit refusal names parsing with limits, which this call does not take.
        let value = JsonValue::parse(input)
            .map_err(openbindings_internal_json::backend::value_check_refusal)?;
        Ok(self.validate(&value))
    }

    /// Validate an admitted exact value, distinguishing satisfies, fails, undefined
    /// (the specification gives the value no defined validity) and no-verdict (the
    /// evaluator could not answer). For ordinary Rust data first use [`JsonValue::from_serializable`];
    /// for exact JSON text/bytes use [`JsonValue::parse`]. Admission failure is
    /// separate from validation.
    ///
    /// This is the cheap check: a failure carries no problems and
    /// `problems_complete` is false. Call [`Self::explain`] with the same value
    /// to learn why it fails.
    pub fn validate(&self, value: &JsonValue) -> ValueOutcome {
        self.validate_with_control(value, &crate::control::NeverCancelled)
    }
    /// Validate with cooperative cancellation. Cancellation observed before a
    /// verdict is established yields no-verdict and leaves this owner usable with
    /// a fresh healthy control. A verdict the default evaluator establishes
    /// before it observes cancellation stands, whether the value satisfies or
    /// fails.
    /// This is not a preemptive wall-clock deadline; applications schedule work.
    pub fn validate_with_control(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        self.check(value, Detail::Outcome, control)
    }
    /// Validate and explain a failure with problems that name where and why the
    /// value fails. The verdict is the one [`Self::validate`] returns.
    ///
    /// Explanation costs more than the cheap check, and its cost is bounded
    /// by the evaluator, not the SDK. The default evaluator reports the JSON
    /// Schema library's first error, with redacted messages and original schema
    /// facts: building it allocates in proportion to the schema and value, so
    /// keep [`Self::validate`] on paths that check untrusted input. The default
    /// evaluator builds an explanation only for values within half the nesting
    /// its `stack_bytes` limit admits and reports a deeper failure without its
    /// problem, so release builds explain on a thread with that much stack;
    /// debug builds need more.
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
        self.schema.validate(value, requested, control)
    }
}

impl std::fmt::Debug for ValueProblemDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueProblemDetails")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for EvaluationProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvaluationProgram").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SchemaOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaOrigin").finish_non_exhaustive()
    }
}

//! Retained value contracts and the optional evaluator interface.
use crate::{document::DocumentInner, schema_space::SchemaSpace, *};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex, PoisonError, Weak},
};

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
    /// no network retrieval is attempted. The default evaluator can prepare paired
    /// bounds for qualified missing static references and return this cause during
    /// validation when those bounds do not decide the instance. Other partial
    /// schemas can refuse at preparation. This does not prove that missing content
    /// is necessary to the instance's semantic verdict.
    /// Use [`Document::references`] to inspect document-wide reference spellings
    /// and original keyword locations, then prepare again with supplied resources.
    /// That inventory is not an exact per-value-contract missing-resource list.
    /// Apply your disclosure policy before logging it.
    ResourceUnavailable,
    /// A configured work or representation limit prevented a verdict.
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
/// [`Conformant::locate`] for the document and [`ResourceSet::locate`] for a
/// supplied resource. A prepared value contract keeps no source text. Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
///
/// Custom evaluators build one with [`Self::new`] or [`SchemaRequest::location`].
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct SchemaLocation {
    /// Original supplied-resource URI, or `None` for the OpenBindings document. A URI is an identity, never an instruction to fetch.
    pub resource: Option<String>,
    /// Exact RFC 6901 pointer from the source's root; an empty pointer denotes the root. Escape for presentation without changing `~0`/`~1` semantics.
    pub pointer: Pointer,
}
impl SchemaLocation {
    /// A location in the document (`resource` `None`) or in the supplied resource
    /// with this URI. [`SchemaRequest::location`] builds one for a schema node an
    /// evaluator was given.
    pub fn new(resource: Option<String>, pointer: Pointer) -> Self {
        Self { resource, pointer }
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
    pub location: Option<SchemaLocation>,
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
    pub fn with_location(mut self, location: SchemaLocation) -> Self {
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
    pub location: Option<SchemaLocation>,
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
    pub fn with_location(mut self, location: SchemaLocation) -> Self {
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
/// fn build(instance: Pointer, schema: SchemaLocation) -> ValueProblem {
///     ValueProblem { instance, schema, code: "type".into(), message: "wrong type".into(),
///         details: None }
/// }
/// ```
/// ```
/// use openbindings::*;
/// fn build(instance: Pointer, schema: SchemaLocation) -> ValueProblem {
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
    /// document's or resource's root; [`Conformant::locate`] and
    /// [`ResourceSet::locate`] find its line and column. When an evaluator
    /// cannot map a keyword to the original source, this names the selected
    /// schema rather than a guessed keyword.
    pub schema: SchemaLocation,
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
        schema: SchemaLocation,
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
#[serde(tag = "outcome", rename_all = "kebab-case")]
/// Closed result partition for the selected schema and instance: satisfies, fails, undefined, or no verdict. No value outcome proves that the surrounding OpenBindings document conforms.
///
/// Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
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

#[derive(Clone)]
/// An explicit immutable schema resource associated with its retrieval identity. Supplying it performs no I/O.
pub struct SchemaResource {
    /// Absolute retrieval URI without a nonempty fragment; [`ResourceSet::new`] validates and normalizes identity.
    pub uri: String,
    /// Exact resource JSON retained by its set; duplicate-member resources are refused.
    pub document: JsonValue,
}
/// Immutable caller-supplied resources. Separate sets can use the same URI
/// for different snapshots; this is not a process-wide registry or an acquirer.
/// Clones share storage, including the identity index built by [`Self::new`].
/// [`SchemaRequest::evaluation_program`] reads that index, and the resolution
/// space it assembles for a nonempty set and a document revision is kept with
/// the set while both are alive, and released when either is dropped. The
/// default evaluator reuses the index instead of indexing the set again, but
/// assembles its own resolution space for each preparation, so its preparation
/// time still grows with the set's resources, including those the contract
/// never reaches.
#[derive(Clone, Default)]
pub struct ResourceSet {
    inner: Arc<ResourceSetInner>,
}
#[derive(Default)]
pub(crate) struct ResourceSetInner {
    resources: Vec<SchemaResource>,
    input_policy: openbindings_internal_schema::Supplied,
    // One space per live revision this set was projected with. A revision's
    // entry leaves when the revision is dropped; all leave with the set.
    spaces: Mutex<Vec<(Weak<DocumentInner>, Arc<SchemaSpace>)>>,
}
impl Drop for ResourceSetInner {
    fn drop(&mut self) {
        let set: *const Self = self;
        let spaces = std::mem::take(
            self.spaces
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        );
        // Live revisions stop tracking this set; the spaces go with it.
        for (revision, _) in &spaces {
            if let Some(revision) = revision.upgrade() {
                revision.forget_set(set);
            }
        }
    }
}
impl ResourceSetInner {
    /// Release the space kept for a revision that is being dropped.
    pub(crate) fn release(&self, document: *const DocumentInner) {
        // Drop the released space after unlocking: it can own large snapshots.
        let released: Vec<_> = {
            let mut spaces = self.spaces.lock().unwrap_or_else(PoisonError::into_inner);
            let (released, kept) = std::mem::take(&mut *spaces)
                .into_iter()
                .partition(|(revision, _)| std::ptr::eq(revision.as_ptr(), document));
            *spaces = kept;
            released
        };
        drop(released);
    }
}
#[derive(Clone, PartialEq, Eq)]
/// Invalid supplied-resource configuration, separate from schema preparation and value outcomes.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct ResourceError {
    /// Caller-supplied resource identifier associated with the error; treat it as untrusted display data.
    pub uri: String,
    /// Explanation of the configuration failure; not a schema verdict.
    pub message: String,
}
impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ResourceError {}
impl ResourceSet {
    /// Admit immutable resources without acquisition. Duplicate retrieval spellings
    /// after RFC resolution and empty-fragment removal are configuration errors.
    /// Canonical associations use the library's ordinary URI normalization;
    /// collisions and protected standard identities are retained as input policy
    /// and refuse when reached, without poisoning unrelated contracts.
    ///
    /// # Errors
    /// Returns a resource error if a URI is not absolute or has a nonempty fragment, two resources have the same normalized URI, or a resource contains duplicate members.
    pub fn new(resources: impl IntoIterator<Item = SchemaResource>) -> Result<Self, ResourceError> {
        let mut by_uri = BTreeMap::new();
        for mut resource in resources {
            let invalid = |message: &str| ResourceError {
                uri: resource.uri.clone(),
                message: message.into(),
            };
            if !crate::uri::absolute(&resource.uri)
                || crate::uri::fragment(&resource.uri).is_some_and(|s| !s.is_empty())
            {
                return Err(invalid(
                    "retrieval URI must be absolute with no nonempty fragment",
                ));
            }
            if resource.document.has_duplicate_names() {
                return Err(invalid("resource repeats JSON member names"));
            }
            resource.uri = crate::uri::compared_id(None, &resource.uri)
                .ok_or_else(|| invalid("invalid retrieval URI"))?;
            resource.document = openbindings_internal_json::backend::standalone(resource.document);
            let uri = resource.uri.clone();
            if by_uri.insert(uri.clone(), resource).is_some() {
                return Err(ResourceError {
                    uri,
                    message: "duplicate retrieval URI".into(),
                });
            }
        }
        let resources: Vec<_> = by_uri.into_values().collect();
        let input_policy = openbindings_internal_schema::Supplied::new(resources.iter().map(|r| {
            openbindings_internal_schema::ProjectedResource {
                uri: r.uri.clone(),
                document: r.document.clone(),
            }
        }))
        .expect("admitted resources");
        Ok(Self {
            inner: Arc::new(ResourceSetInner {
                resources,
                input_policy,
                spaces: Mutex::default(),
            }),
        })
    }
    /// Borrow supplied resources in deterministic normalized-URI order; no allocation or acquisition.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &SchemaResource> {
        self.inner.resources.iter()
    }
    /// Locate a schema location in one of these resources: the node its
    /// pointer names in the resource with its URI, with line and column in
    /// that resource's own text. A resource built from a subtree of a larger
    /// value is admitted as its own text, so its coordinates count from the
    /// subtree's start, where [`JsonValue::locate`] on that subtree counts in
    /// the whole text it was parsed from. `None` for a document location, a
    /// URI this set does not hold (such as a packaged meta-schema's), or a
    /// pointer that names no node. Positions are computed on this call.
    pub fn locate(&self, at: &SchemaLocation) -> Option<SourceLocation> {
        let uri = at.resource.as_deref()?;
        let resources = &self.inner.resources;
        let found = resources
            .binary_search_by(|resource| resource.uri.as_str().cmp(uri))
            .ok()?;
        resources[found].document.locate(&at.pointer)
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.resources.is_empty()
    }
    /// The space kept for this set and a live revision, if one was assembled.
    pub(crate) fn kept_space(&self, document: &Arc<DocumentInner>) -> Option<Arc<SchemaSpace>> {
        let spaces = self
            .inner
            .spaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        spaces
            .iter()
            .find(|(revision, _)| std::ptr::eq(revision.as_ptr(), Arc::as_ptr(document)))
            .map(|(_, space)| space.clone())
    }
    /// Keep `space` for this set and a live revision; when a concurrent
    /// preparation kept one first, that space wins and is returned.
    pub(crate) fn keep_space(
        &self,
        document: &Arc<DocumentInner>,
        space: Arc<SchemaSpace>,
    ) -> Arc<SchemaSpace> {
        let mut spaces = self
            .inner
            .spaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((_, kept)) = spaces
            .iter()
            .find(|(revision, _)| std::ptr::eq(revision.as_ptr(), Arc::as_ptr(document)))
        {
            return kept.clone();
        }
        spaces.push((Arc::downgrade(document), space.clone()));
        space
    }
    pub(crate) fn downgrade(&self) -> Weak<ResourceSetInner> {
        Arc::downgrade(&self.inner)
    }
    #[cfg(test)]
    pub(crate) fn kept_spaces(&self) -> usize {
        self.inner.spaces.lock().unwrap().len()
    }
    /// One shared empty set, so preparation without resources allocates nothing.
    pub(crate) fn empty() -> &'static Self {
        static EMPTY: std::sync::OnceLock<ResourceSet> = std::sync::OnceLock::new();
        EMPTY.get_or_init(Self::default)
    }
}

// The default evaluator reuses the identity index built in `ResourceSet::new`
// rather than admitting the resources again on every preparation.
impl openbindings_internal_schema::SuppliedSet for ResourceSet {
    fn supplied(&self) -> &openbindings_internal_schema::Supplied {
        &self.inner.input_policy
    }
}

impl Conformant<Document> {
    /// Locate a schema location in this document: the node its pointer names,
    /// with line and column in the document's text. `None` for a location in a
    /// supplied resource, or a pointer that names no node in this revision.
    /// Positions are computed on this call; a prepared value contract keeps none.
    pub fn locate(&self, at: &SchemaLocation) -> Option<SourceLocation> {
        if at.resource.is_some() {
            return None;
        }
        self.as_document().value().locate(&at.pointer)
    }
}

/// Original snapshots and an entry point. Evaluators may use the checked projection
/// helper or evaluate this request directly; generated identifiers are never API identity.
#[derive(Clone)]
pub struct SchemaRequest {
    document: Conformant<Document>,
    resources: ResourceSet,
    entry: JsonValue,
}
impl std::fmt::Debug for SchemaRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRequest").finish_non_exhaustive()
    }
}

impl SchemaRequest {
    /// Borrow the conformant document revision owning this normative value contract.
    pub fn document(&self) -> &Conformant<Document> {
        &self.document
    }
    /// Borrow the immutable explicit resources for this preparation; evaluators must not substitute global URI caches.
    pub fn supplied_resources(&self) -> &ResourceSet {
        &self.resources
    }
    /// Borrow the selected exact input/output schema in its original context.
    pub fn entry(&self) -> &JsonValue {
        &self.entry
    }
    /// Allocate the selected schema's original document location.
    pub fn entry_location(&self) -> SchemaLocation {
        SchemaLocation {
            resource: None,
            pointer: openbindings_internal_json::backend::exact_pointer(self.entry.view()),
        }
    }
    /// Allocate the original location of a schema node from this request's
    /// document (such as one inside [`Self::entry`]) or from one of its
    /// supplied resources, or `None` for a node from another source, including
    /// a projected [`EvaluationProgram`] resource. Builds only the pointer.
    pub fn location(&self, node: JsonRef<'_>) -> Option<SchemaLocation> {
        use openbindings_internal_json::backend::{exact_pointer, same_source};
        let resource = if same_source(node, self.document.as_document().value()) {
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
        Some(SchemaLocation {
            resource,
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
        let space = self.document.resolution(&self.resources);
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
    resources: Vec<SchemaResource>,
    projection: openbindings_internal_schema::Program,
}
impl EvaluationProgram {
    /// Borrow the private absolute URI selecting the projected entry.
    pub fn entry_uri(&self) -> &str {
        &self.entry_uri
    }
    /// Borrow the immutable projected resources with their original provenance.
    pub fn resources(&self) -> &[SchemaResource] {
        &self.resources
    }
    /// Map a generated keyword URI to an established original source location.
    pub fn original_location(&self, uri: &str) -> Option<SchemaLocation> {
        self.projection
            .original_location(uri)
            .map(original_location)
    }
}
impl EvaluationProgram {
    pub(crate) fn from_projection(mut projection: openbindings_internal_schema::Program) -> Self {
        let resources = std::mem::take(&mut projection.resources)
            .into_iter()
            .map(|r| SchemaResource {
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
pub(crate) fn original_location(at: openbindings_internal_schema::Location) -> SchemaLocation {
    SchemaLocation {
        resource: at.resource,
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
/// An operation's input or output schema as a normative value contract, not yet
/// prepared. Only a conformant operation hands one out, through
/// [`ConformantOperationView::input`] and [`ConformantOperationView::output`]; a
/// plain [`OperationView`] exposes the exact schema value without preparation:
/// ```compile_fail,E0599
/// use openbindings::*;
/// fn draft(operation: OperationView, evaluator: &dyn SchemaEvaluator) {
///     let _ = operation.input().unwrap().prepare(evaluator);
/// }
/// ```
/// Neither a value contract nor a prepared value contract can be assembled by hand:
/// ```compile_fail,E0451
/// use openbindings::*;
/// fn forge(operation: &ConformantOperationView) {
///     let _ = ValueContract { operation, side: todo!() };
/// }
/// ```
/// ```compile_fail,E0451
/// use openbindings::*;
/// fn raw(schema: std::sync::Arc<dyn PreparedSchema>) { let _ = PreparedValueContract { schema }; }
/// ```
/// Preparation borrows the evaluator and any supplied resources. Each call
/// prepares anew; retain the [`PreparedValueContract`] to validate repeatedly. The
/// document's resolution space is assembled only when an evaluator requests the
/// core projection, once per live revision and resource set. See the
/// [Rust lifecycle guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md#resources-diagnostics-and-lifetime).
#[derive(Clone, Copy)]
pub struct ValueContract<'a> {
    // Two words, like the exact schema reference it replaces in conformant reads.
    operation: &'a ConformantOperationView,
    side: Side,
}
/// Which declared field of its operation a contract is.
#[derive(Clone, Copy)]
pub(crate) enum Side {
    Input,
    Output,
}
impl fmt::Debug for ValueContract<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValueContract { .. }")
    }
}
impl<'a> ValueContract<'a> {
    /// The contract of a field the operation declares, or `None` when it is absent.
    pub(crate) fn declared(operation: &'a ConformantOperationView, side: Side) -> Option<Self> {
        operation.field(side).map(|_| Self { operation, side })
    }
    /// Borrow the exact schema without preparing it. Present `true`, `false` and
    /// objects remain distinct original values.
    pub fn schema(&self) -> JsonRef<'a> {
        self.operation
            .field(self.side)
            .expect("a contract exists only for a declared field")
    }
    /// Prepare with no supplied resources. Ready means prepared, not decidable:
    /// a prepared value contract can still return any of the four [`ValueOutcome`]
    /// variants, including no-verdict for every value.
    ///
    /// # Errors
    /// Returns a [`NoVerdict`] refusal when preparation is not established. A
    /// refusal judges no instance and does not prove semantic undefinedness.
    pub fn prepare(
        &self,
        evaluator: &dyn SchemaEvaluator,
    ) -> Result<PreparedValueContract, NoVerdict> {
        self.prepare_with(evaluator, ResourceSet::empty())
    }
    /// Prepare with immutable supplied resources; nothing is fetched. An
    /// evaluator that calls [`SchemaRequest::evaluation_program`] reuses the
    /// set's index, and its resolution space with this document while both are
    /// alive. The default evaluator reuses the set's index and assembles its
    /// resolution space again on every preparation.
    ///
    /// # Errors
    /// Returns a [`NoVerdict`] refusal when preparation is not established. A
    /// refusal judges no instance and does not prove semantic undefinedness.
    pub fn prepare_with(
        &self,
        evaluator: &dyn SchemaEvaluator,
        resources: &ResourceSet,
    ) -> Result<PreparedValueContract, NoVerdict> {
        self.prepare_with_control(evaluator, resources, &crate::work_control::NeverCancelled)
    }
    /// Prepare with cooperative cancellation. A cancelled attempt returns no
    /// prepared owner. Cancellation and transient evaluator failures do not
    /// poison a retry; use a fresh [`WorkControl`] after cancellation.
    ///
    /// # Errors
    /// Returns a [`NoVerdict`] refusal when preparation is not established,
    /// including `Cancelled` when cancellation is observed before a prepared
    /// owner is returned. A refusal judges no instance.
    pub fn prepare_with_control(
        &self,
        evaluator: &dyn SchemaEvaluator,
        resources: &ResourceSet,
        control: &dyn WorkControl,
    ) -> Result<PreparedValueContract, NoVerdict> {
        control.check()?;
        let schema = self.schema();
        let entry = schema.to_owned();
        let document = self.operation.proof();
        if !document
            .as_document()
            .schema_index()
            .positions
            .contains_key(&openbindings_internal_json::backend::node_id(&entry))
        {
            return Err(NoVerdict::new(
                NoVerdictReason::ConservativePreparation,
                "non-schema-entry",
                "the operation direction is not a schema position",
            )
            .with_location(SchemaLocation {
                resource: None,
                pointer: openbindings_internal_json::backend::exact_pointer(schema),
            }));
        }
        let request = SchemaRequest {
            document: document.clone(),
            resources: resources.clone(),
            entry,
        };
        let prepared = evaluator.prepare(&request, control);
        // An evaluator may finish while the caller cancels; no ready owner is returned.
        control.check()?;
        prepared.map(|schema| PreparedValueContract { schema })
    }
}

/// A prepared value contract that retains its required compiled state
/// independently of the document, resources and evaluator. It is
/// `Clone + Send + Sync + 'static`; cloning shares immutable state, and dropping
/// the final owner releases it, without promising lower RSS.
#[derive(Clone)]
pub struct PreparedValueContract {
    schema: Arc<dyn PreparedSchema>,
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
        let value = JsonValue::parse(input)?;
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
        self.validate_with_control(value, &crate::work_control::NeverCancelled)
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
    /// facts: building it can allocate in proportion to the schema and value and
    /// can overflow the stack on deeply nested values. Explain values you trust,
    /// or values that already passed size and depth limits, and keep
    /// [`Self::validate`] on paths that check untrusted input. Failure
    /// diagnostics can be incomplete; inspect `problems_complete` rather than
    /// assuming every failed keyword is reported.
    pub fn explain(&self, value: &JsonValue) -> ValueOutcome {
        self.explain_with_control(value, &crate::work_control::NeverCancelled)
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

impl std::fmt::Debug for SchemaResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaResource").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ResourceSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceSet")
            .field("resources", &self.inner.resources.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SchemaLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaLocation").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self.message.as_str() {
            "retrieval URI must be absolute with no nonempty fragment" => {
                "retrieval URI must be absolute with no nonempty fragment"
            }
            "resource repeats JSON member names" => "resource repeats JSON member names",
            "invalid retrieval URI" => "invalid retrieval URI",
            "duplicate retrieval URI" => "duplicate retrieval URI",
            _ => "<redacted>",
        };
        f.debug_struct("ResourceError")
            .field("reason", &reason)
            .finish_non_exhaustive()
    }
}

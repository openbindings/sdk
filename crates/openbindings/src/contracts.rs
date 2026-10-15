//! Retained value contracts and the optional evaluator interface.
use crate::{schema_space::SchemaSpace, *};
use serde::Serialize;
use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Closed choice of the operation's input or output contract.
pub enum Direction {
    /// Select the operation's `input` schema.
    Input,
    /// Select the operation's `output` schema.
    Output,
}
impl Direction {
    /// Return the normative field spelling, `input` or `output`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
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
    /// no network retrieval is attempted. The default evaluator can prepare paired
    /// bounds for qualified missing static references and return this cause during
    /// validation when those bounds do not decide the instance. Other partial
    /// schemas can refuse at preparation. This does not prove that missing content
    /// is necessary to the instance's semantic verdict.
    /// Use [`Document::references`] to inspect document-wide reference spellings
    /// and original keyword locations, then create a new context with supplied resources.
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
/// Location in an original schema source; generated evaluation-program identities are mapped back before exposure.
pub struct SchemaLocation {
    /// Original supplied-resource URI, or `None` for the OpenBindings document. A URI is an identity, never an instruction to fetch.
    pub resource: Option<String>,
    /// RFC 6901 JSON Pointer within the original source; the empty string denotes its root. Escape for presentation without changing `~0`/`~1` semantics.
    pub pointer: String,
}
#[derive(Clone, Serialize)]
/// Structured explanation for why no schema verdict was established. This public record may be constructed by custom evaluators.
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
        f.debug_struct("NoVerdict")
            .field("reason", &self.reason)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}
impl NoVerdict {
    /// Construct an unlocated refusal; callers may set its public `location` when original-source evidence is available.
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
    pub(crate) fn located(mut self, location: SchemaLocation) -> Self {
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

#[derive(Clone, Serialize)]
/// An established undefined result under the value-contract rules. Custom evaluators can construct this public record when semantic provenance and disagreeing completions are proved.
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
#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
/// One established instance failure with an optional original schema coordinate. Custom evaluators can construct this public record.
pub struct ValueProblem {
    /// RFC 6901 pointer to an existing location in the input instance; empty means the root.
    pub instance_pointer: String,
    /// Original schema keyword location when mapping is available.
    pub schema_location: Option<SchemaLocation>,
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
            .field("schema_location", &self.schema_location)
            .field("details", &self.details)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
/// Closed result partition for the selected schema and instance: satisfies, fails, undefined, or no verdict. No value outcome proves that the surrounding OpenBindings document conforms.
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
    /// Exact resource JSON retained by the context; duplicate-member resources are refused.
    pub document: JsonValue,
}
/// Immutable caller-supplied resources. Separate contexts can use the same URI
/// for different snapshots; this is not a process-wide registry or an acquirer.
#[derive(Clone, Default)]
pub struct ResourceSet {
    resources: Arc<Vec<SchemaResource>>,
    pub(crate) input_policy: openbindings_internal_schema::Supplied,
}
#[derive(Clone, PartialEq, Eq)]
/// Invalid supplied-resource configuration, separate from schema preparation and value outcomes.
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
            resources: Arc::new(resources),
            input_policy,
        })
    }
    /// Borrow supplied resources in deterministic normalized-URI order; no allocation or acquisition.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &SchemaResource> {
        self.resources.iter()
    }
}

/// Original snapshots and an entry point. Evaluators may use the checked projection
/// helper or evaluate this context directly; generated identifiers are never API identity.
#[derive(Clone)]
pub struct SchemaRequest {
    pub(crate) space: Arc<SchemaSpace>,
    pub(crate) entry: usize,
    document: Conformant<Document>,
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
    /// Borrow the immutable explicit resources for this context; evaluators must not substitute global URI caches.
    pub fn supplied_resources(&self) -> &ResourceSet {
        &self.space.supplied
    }
    /// Borrow the selected exact input/output schema in its original context.
    pub fn entry(&self) -> &JsonValue {
        &self.space.nodes[self.entry].value
    }
    /// Allocate the selected schema's original document location.
    pub fn entry_location(&self) -> SchemaLocation {
        self.space.location(self.entry)
    }
    /// Build a finite evaluator projection with private resource identities, original-location mapping and cooperative cancellation. Refuses unsupported/ambiguous/unavailable references instead of retrieving data.
    ///
    /// # Errors
    /// Returns a refusal if references cannot be resolved soundly, a required capability or resource is unavailable, work limits are exceeded, or cancellation is observed. A refusal is not an instance failure.
    pub fn evaluation_program(
        &self,
        control: &dyn WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        self.space.program(self.entry, control)
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
///             Err(NoVerdict {
///                 reason: NoVerdictReason::UnsupportedCapability,
///                 code: "true-only".into(),
///                 message: "this evaluator supports only the true schema".into(),
///                 location: Some(request.entry_location()),
///             })
///         }
///     }
/// }
/// impl PreparedSchema for TrueOnly {
///     fn validate(&self, _: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
///         match control.check() {
///             Ok(()) => ValueOutcome::Satisfies,
///             Err(detail) => ValueOutcome::NoVerdict { detail: detail.into() },
///         }
///     }
/// }
/// let document = Document::parse(
///     r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}}}"#)?;
/// let context = document.validate().unwrap().conformant().expect("conformant document").value_contracts(Arc::new(TrueOnly), ResourceSet::default());
/// let ValueContractPreparation::Ready(contract) = context.prepare("run", Direction::Input) else {
///     panic!("the true schema must be supported");
/// };
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
/// Thread-safe retained validator returned by a [`SchemaEvaluator`]. Repeated calls must preserve caller input and retain no accidental borrow of a completed request. Only claim established semantic verdicts; bound diagnostic work separately and report incompleteness.
pub trait PreparedSchema: Send + Sync {
    /// Return only established verdicts. Resource/capability/work failures are NoVerdict.
    fn validate(&self, value: &JsonValue, control: &dyn WorkControl) -> ValueOutcome;
}
/// An immutable document/resource/evaluator context with caller-owned preparation.
/// Select a value contract using [`Self::prepare`], then retain its ready owner for
/// repeated validation. Replacing an application's active context does not
/// mutate already prepared work. See the
/// [Rust lifecycle guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md#resources-diagnostics-and-lifetime).
#[derive(Clone)]
pub struct ValueContracts {
    inner: Arc<ContractsInner>,
}
impl std::fmt::Debug for ValueContracts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueContracts").finish_non_exhaustive()
    }
}

struct ContractsInner {
    document: Conformant<Document>,
    space: Arc<SchemaSpace>,
    evaluator: Arc<dyn SchemaEvaluator>,
}
/// Setup result. Ready means successfully prepared, not decidable. A ready
/// contract may return no verdict for every instance (including a bare missing reference).
#[derive(Clone)]
#[must_use = "inspect the outcome before continuing"]
pub enum ValueContractPreparation {
    /// A retained prepared value contract, independently usable after context drop.
    Ready(ValueContract),
    /// The selected operation exists but has no field for this direction; this is distinct from the present boolean schema `false`.
    NoValueContract,
    /// No primary key or alias matches the request.
    OperationMissing,
    /// Retained selection branch; conformant documents cannot produce it because OBI-05 proves unique names. Document selection reports ambiguity through OperationSelection.
    OperationAmbiguous {
        #[doc = "Distinct matching primary keys in lexical order; repeated aliases can yield one candidate key."]
        candidates: Vec<String>,
    },
    /// Selection found a value contract, but preparation did not establish readiness.
    NoVerdict {
        #[doc = "Truthful preparation refusal; no instance was judged."]
        detail: NoVerdict,
    },
}
impl ValueContractPreparation {
    /// Consume preparation into a ready value contract or its non-ready reason.
    /// A ready contract can still return any of the four [`ValueOutcome`] variants.
    /// This conversion does not retry preparation or change its evidence.
    ///
    /// # Errors
    /// Returns the missing-operation, absent-contract, ambiguous-operation or
    /// no-verdict reason with its original payload.
    pub fn into_result(self) -> Result<ValueContract, ValueContractPreparationError> {
        match self {
            Self::Ready(contract) => Ok(contract),
            Self::NoValueContract => Err(ValueContractPreparationError::NoValueContract),
            Self::OperationMissing => Err(ValueContractPreparationError::OperationMissing),
            Self::OperationAmbiguous { candidates } => {
                Err(ValueContractPreparationError::OperationAmbiguous { candidates })
            }
            Self::NoVerdict { detail } => Err(ValueContractPreparationError::NoVerdict { detail }),
        }
    }
}

/// A non-ready value-contract preparation result for callers using `?`.
/// No instance was judged. Payloads retain the original selection or preparation
/// evidence. `Display` and [`std::error::Error::source`] can expose evaluator text;
/// `Debug` follows [`NoVerdict`]'s redaction and shows no caller or document text.
#[derive(Clone)]
#[non_exhaustive]
pub enum ValueContractPreparationError {
    /// The selected operation has no schema for the requested direction.
    NoValueContract,
    /// No primary key or alias matches the request.
    OperationMissing,
    /// Retained selection branch, unreachable through a conformant document.
    OperationAmbiguous {
        /// Distinct matching primary keys in lexical order, retained unchanged.
        candidates: Vec<String>,
    },
    /// Preparation did not establish readiness. This does not prove undefinedness.
    NoVerdict {
        /// Original reason, code, message and optional schema location.
        detail: NoVerdict,
    },
}
impl fmt::Debug for ValueContractPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValueContract => f.write_str("NoValueContract"),
            Self::OperationMissing => f.write_str("OperationMissing"),
            Self::OperationAmbiguous { .. } => {
                f.debug_struct("OperationAmbiguous").finish_non_exhaustive()
            }
            // NoVerdict's own Debug already applies the redaction rule.
            Self::NoVerdict { detail } => {
                f.debug_struct("NoVerdict").field("detail", detail).finish()
            }
        }
    }
}
impl fmt::Display for ValueContractPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValueContract => f.write_str("operation has no value contract"),
            Self::OperationMissing => f.write_str("operation is missing"),
            Self::OperationAmbiguous { .. } => f.write_str("operation is ambiguous"),
            Self::NoVerdict { detail } => write!(f, "value contract preparation refused: {detail}"),
        }
    }
}
impl std::error::Error for ValueContractPreparationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoVerdict { detail } => Some(detail),
            _ => None,
        }
    }
}
impl std::fmt::Debug for ValueContractPreparation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(value) => f.debug_tuple("Ready").field(value).finish(),
            Self::NoValueContract => f.write_str("NoValueContract"),
            Self::OperationMissing => f.write_str("OperationMissing"),
            Self::OperationAmbiguous { .. } => {
                f.debug_struct("OperationAmbiguous").finish_non_exhaustive()
            }
            Self::NoVerdict { detail } => {
                f.debug_struct("NoVerdict").field("detail", detail).finish()
            }
        }
    }
}

/// A ready value contract that retains its required compiled state independently of
/// document/context lifetime. Cloning shares immutable state;
/// dropping the final owner releases that owner, without promising lower RSS.
#[derive(Clone)]
pub struct ValueContract {
    schema: Arc<dyn PreparedSchema>,
}
impl fmt::Debug for ValueContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValueContract { .. }")
    }
}
impl ValueContract {
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
    /// separate from validation. Failure diagnostics can be incomplete; inspect
    /// `problems_complete` rather than assuming every failed keyword is reported.
    pub fn validate(&self, value: &JsonValue) -> ValueOutcome {
        self.validate_with_control(value, &crate::work_control::NeverCancelled)
    }
    /// Validate with cooperative cancellation. Cancellation before a verdict yields
    /// no-verdict and leaves this owner usable with a fresh healthy control.
    /// Cancellation of optional diagnostics does not weaken an established failure.
    /// This is not a preemptive wall-clock deadline; applications schedule work.
    pub fn validate_with_control(
        &self,
        value: &JsonValue,
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
        self.schema.validate(value, control)
    }
}
impl Conformant<Document> {
    /// Select an evaluator and immutable supplied resources for normative value contracts.
    /// Documents cannot construct normative contracts:
    /// ```compile_fail
    /// use openbindings::*;
    /// fn draft(doc: Document, evaluator: std::sync::Arc<dyn SchemaEvaluator>) {
    ///     doc.value_contracts(evaluator, ResourceSet::default());
    /// }
    /// ```
    /// Raw prepared schemas cannot manufacture a normative contract:
    /// ```compile_fail
    /// use openbindings::*;
    /// fn raw(schema: std::sync::Arc<dyn PreparedSchema>) { let _ = ValueContract { schema }; }
    /// ```
    /// No preparation or acquisition occurs. Each prepare compiles anew; retain
    /// the ready ValueContract to reuse it. Ready means prepared, not guaranteed decidable.
    pub fn value_contracts(
        &self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
    ) -> ValueContracts {
        ValueContracts {
            inner: Arc::new(ContractsInner {
                document: self.clone(),
                space: Arc::new(SchemaSpace::new(self.as_document().clone(), resources)),
                evaluator,
            }),
        }
    }
}
impl ValueContracts {
    /// Select a primary operation name or alias and prepare one direction's value contract.
    /// Match [`ValueContractPreparation`] or use [`ValueContractPreparation::into_result`]
    /// to propagate a non-ready result with `?`. Missing operation,
    /// absent value contract and preparation refusal are setup states, not instance failures.
    /// The retained ambiguous branch is unreachable through conformant documents.
    /// Ready value contracts can be retained beyond this context and validated repeatedly.
    /// Every call prepares independently, including aliases of the same operation.
    /// Retain and clone a ready [`ValueContract`] to validate without recompiling.
    pub fn prepare(&self, operation: &str, direction: Direction) -> ValueContractPreparation {
        self.prepare_with_control(operation, direction, &crate::work_control::NeverCancelled)
    }
    /// Prepare with cooperative cancellation. A cancelled attempt returns no ready
    /// owner. Cancellation and transient evaluator failures do not poison healthy
    /// retry on this context; use a fresh [`WorkControl`] after cancellation.
    /// A no-verdict refusal does not prove semantic undefinedness. To change supplied
    /// resources, build a new immutable context and decide when to replace active work.
    pub fn prepare_with_control(
        &self,
        operation: &str,
        direction: Direction,
        control: &dyn WorkControl,
    ) -> ValueContractPreparation {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueContractPreparation::NoVerdict { detail };
        }
        let Some(op) = self.inner.document.resolve_operation(operation) else {
            return ValueContractPreparation::OperationMissing;
        };
        let Some(schema) = op.value().get(direction.as_str()) else {
            return ValueContractPreparation::NoValueContract;
        };
        let Some(entry) = self.inner.space.document_node(&schema.to_owned()) else {
            return ValueContractPreparation::NoVerdict {
                detail: NoVerdict::new(
                    NoVerdictReason::ConservativePreparation,
                    "non-schema-entry",
                    "the operation direction is not a schema position",
                )
                .located(SchemaLocation {
                    resource: None,
                    pointer: schema.location().pointer.unwrap_or_default(),
                }),
            };
        };
        let request = SchemaRequest {
            document: self.inner.document.clone(),
            space: self.inner.space.clone(),
            entry,
        };
        let prepared = self.inner.evaluator.prepare(&request, control);
        // An evaluator may finish while the caller cancels; no ready owner is returned.
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueContractPreparation::NoVerdict { detail };
        }
        match prepared {
            Ok(schema) => ValueContractPreparation::Ready(ValueContract { schema }),
            Err(detail) => ValueContractPreparation::NoVerdict { detail },
        }
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
            .field("resources", &self.resources.len())
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

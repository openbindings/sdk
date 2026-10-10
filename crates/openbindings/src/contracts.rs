//! Retained operation contracts and the optional evaluator interface.
use crate::{schema_space::SchemaSpace, *};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Closed choice of the operation's input or output contract.
pub enum Side {
    /// Select the operation's `input` schema.
    Input,
    /// Select the operation's `output` schema.
    Output,
}
impl Side {
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
    /// A required resource is absent from the explicitly supplied context; no network retrieval is attempted.
    /// Use [`ParsedDocument::references`] to inspect explicit reference spellings
    /// and original keyword locations. Apply your disclosure policy before logging them.
    ResourceUnavailable,
    /// A configured work or representation limit prevented a verdict.
    LimitExceeded,
    /// The caller's cooperative cancellation was observed.
    Cancelled,
    /// The evaluator encountered an internal or operational failure; retry policy belongs to the caller.
    EvaluatorFailure,
    /// Semantic undefinedness was established. Potential cycles or incomplete analysis alone must use a conservative refusal instead.
    Undefined,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Location in an original schema source; generated evaluation-program identities are mapped back before exposure.
pub struct SchemaLocation {
    /// Original supplied-resource URI, or `None` for the OpenBindings document. A URI is an identity, never an instruction to fetch.
    pub resource: Option<String>,
    /// RFC 6901 JSON Pointer within the original source; the empty string denotes its root. Escape for presentation without changing `~0`/`~1` semantics.
    pub pointer: String,
}
#[derive(Clone, Debug, Serialize)]
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
    fn cacheable(&self) -> bool {
        !matches!(
            self.reason,
            NoVerdictReason::Cancelled | NoVerdictReason::EvaluatorFailure
        )
    }
}
impl fmt::Display for NoVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for NoVerdict {}
#[derive(Clone, Debug, Serialize)]
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
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
/// Closed semantic partition for the selected schema and instance: satisfies, fails, or no verdict. Neither established verdict proves that the surrounding OpenBindings document conforms.
pub enum ValueOutcome {
    /// The instance was established to satisfy the selected schema.
    Satisfies,
    /// The instance was established to fail the selected schema; diagnostic completeness is separate from the verdict.
    Fails {
        /// Retained actual failing instance locations; diagnostics may be bounded after failure is established.
        problems: Vec<ValueProblem>,
        /// Whether all available failure diagnostics were collected. False does not weaken the established failure verdict.
        problems_complete: bool,
    },
    /// No satisfaction/failure verdict was established.
    NoVerdict {
        /// Structured cause of refusal, including an original schema location when known.
        detail: NoVerdict,
    },
}

#[derive(Clone, Debug)]
/// An explicit immutable schema resource associated with its retrieval identity. Supplying it performs no I/O.
pub struct SchemaResource {
    /// Absolute retrieval URI without a nonempty fragment; [`ResourceSet::new`] validates and normalizes identity.
    pub uri: String,
    /// Exact resource JSON retained by the context; duplicate-member resources are refused.
    pub document: JsonValue,
}
/// Immutable caller-supplied resources. Separate contexts can use the same URI
/// for different snapshots; this is not a process-wide registry or an acquirer.
#[derive(Clone, Debug, Default)]
pub struct ResourceSet {
    resources: Arc<Vec<SchemaResource>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Identifiers are compared exactly after RFC resolution and empty-fragment removal.
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
        Ok(Self {
            resources: Arc::new(by_uri.into_values().collect()),
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
}
impl SchemaRequest {
    /// Borrow the original immutable OpenBindings snapshot, which may be a draft rather than normative proof.
    pub fn document(&self) -> &ParsedDocument {
        &self.space.document
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
    pub fn evaluation_program(
        &self,
        control: &WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        self.space.program(self.entry, control)
    }
}
/// A closed JSON Schema projection with an original-source map. Names are private
/// to this program; consumers must not rely on their spelling or numbering.
#[derive(Clone, Debug)]
pub struct EvaluationProgram {
    /// Private absolute URI selecting the projected entry; never expose it as an original diagnostic location.
    pub entry_uri: String,
    /// Owned projected schema resources sufficient for this program; an evaluator may release them after compilation retains its required state.
    pub resources: Vec<SchemaResource>,
    pub(crate) locations: BTreeMap<String, SchemaLocation>,
}
/// Original schema location or URI-decoding scratch exceeds the caller's UTF-8
/// byte allowance. No partial location is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocationBudgetExceeded;
impl fmt::Display for LocationBudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("original schema location exceeds the diagnostic byte allowance")
    }
}
impl std::error::Error for LocationBudgetExceeded {}
impl EvaluationProgram {
    /// Map a generated absolute keyword URI back to original source, or return `None` if no mapping exists. Never invent coordinates for an unmapped diagnostic.
    pub fn original_location(&self, generated_uri: &str) -> Option<SchemaLocation> {
        self.original_location_bounded(generated_uri, usize::MAX)
            .ok()
            .flatten()
    }
    /// Map an original location when its resource URI plus complete pointer fit
    /// `max_bytes` UTF-8 bytes. Admission precedes copying original strings.
    /// URI fragment percent-decoding uses one normalized-string buffer with a
    /// separate logical UTF-8 allowance of `max_bytes`. Its encoded input length
    /// is a conservative preallocation bound; allocator capacity/reallocation is
    /// not measured. Percent escapes in the URI base are preserved verbatim.
    /// `Ok(None)` means no mapping or an invalid percent-encoded fragment;
    /// [`LocationBudgetExceeded`] means either allowance was insufficient.
    /// This helper establishes neither a verdict nor a total heap bound.
    ///
    /// ```
    /// # use openbindings::{EvaluationProgram, LocationBudgetExceeded};
    /// # fn map(program: &EvaluationProgram, uri: &str) {
    /// match program.original_location_bounded(uri, 1024) {
    ///     Ok(Some(location)) => assert!(location.pointer.len() <= 1024),
    ///     Ok(None) => {} // Preserve an absent location; never guess coordinates.
    ///     Err(LocationBudgetExceeded) => {} // Mark diagnostics incomplete.
    /// }
    /// # }
    /// ```
    pub fn original_location_bounded(
        &self,
        generated_uri: &str,
        max_bytes: usize,
    ) -> Result<Option<SchemaLocation>, LocationBudgetExceeded> {
        let normalized = match generated_uri.split_once('#') {
            Some((base, fragment)) if fragment.contains('%') => {
                if generated_uri.len() > max_bytes {
                    return Err(LocationBudgetExceeded);
                }
                let Some(mut decoded) = crate::uri::decode_fragment(fragment) else {
                    return Ok(None);
                };
                // Grow this admitted buffer instead of retaining both decoded
                // and formatted URI strings. Logical length never exceeds the
                // encoded URI length admitted above.
                decoded.insert(0, '#');
                decoded.insert_str(0, base);
                std::borrow::Cow::Owned(decoded)
            }
            _ => std::borrow::Cow::Borrowed(generated_uri),
        };
        let generated_uri = normalized.as_ref();
        let mut end = generated_uri.len();
        loop {
            if let Some(location) = self.locations.get(&generated_uri[..end]) {
                let bytes = location
                    .resource
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(location.pointer.len())
                    .saturating_add(generated_uri.len() - end);
                if bytes > max_bytes {
                    return Err(LocationBudgetExceeded);
                }
                let mut pointer =
                    String::with_capacity(location.pointer.len() + generated_uri.len() - end);
                pointer.push_str(&location.pointer);
                pointer.push_str(&generated_uri[end..]);
                return Ok(Some(SchemaLocation {
                    resource: location.resource.clone(),
                    pointer,
                }));
            }
            let Some(previous) = generated_uri[..end].rfind('/') else {
                return Ok(None);
            };
            end = previous;
        }
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
///     fn prepare(&self, request: &SchemaRequest, control: &WorkControl)
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
///     fn validate(&self, _: &JsonValue, control: &WorkControl) -> ValueOutcome {
///         match control.check() {
///             Ok(()) => ValueOutcome::Satisfies,
///             Err(detail) => ValueOutcome::NoVerdict { detail },
///         }
///     }
/// }
/// let document = ParsedDocument::parse(
///     r#"{"openbindings":"0.2.0","operations":{"run":{"input":true}}}"#)?;
/// let context = document.value_contracts(Arc::new(TrueOnly), ResourceSet::default())?;
/// let ContractPreparation::Ready(contract) = context.prepare("run", Side::Input) else {
///     panic!("the true schema must be supported");
/// };
/// assert!(matches!(contract.validate(&JsonValue::null()), ValueOutcome::Satisfies));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait SchemaEvaluator: Send + Sync {
    /// Prepare this selected schema only, borrowing the request and control for the call. Return a shareable immutable [`PreparedSchema`] or truthful [`NoVerdict`]; never turn preparation refusal into instance failure.
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict>;
}
/// Thread-safe retained validator returned by a [`SchemaEvaluator`]. Repeated calls must preserve caller input and retain no accidental borrow of a completed request. Only claim established semantic verdicts; bound diagnostic work separately and report incompleteness.
pub trait PreparedSchema: Send + Sync {
    /// Return only established verdicts. Resource/capability/work failures are NoVerdict.
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome;
}
/// An immutable document/resource/evaluator context with bounded preparation reuse.
/// Select a contract using [`Self::prepare`], then retain its ready owner for
/// repeated validation. Replacing an application's active context does not
/// mutate already prepared work. See the
/// [Rust lifecycle guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md#retained-work-and-replacement).
#[derive(Clone)]
pub struct ValueContracts {
    inner: Arc<ContractsInner>,
}
/// Retention policy for preparation reuse. Explicitly retained handles are
/// independent of this cache and remain usable after eviction or context drop.
#[derive(Clone, Copy, Debug)]
pub struct ValueContractOptions {
    /// Most-recently-used preparation entries retained by this context.
    /// Default is four. Zero disables retention. This limits entries, not total allocation bytes.
    pub cache_capacity: usize,
}
impl Default for ValueContractOptions {
    fn default() -> Self {
        Self { cache_capacity: 4 }
    }
}
struct ContractsInner {
    space: Arc<SchemaSpace>,
    evaluator: Arc<dyn SchemaEvaluator>,
    prepared: Mutex<PreparationCache>,
}
struct PreparationCache {
    capacity: usize,
    entries: HashMap<usize, Arc<OnceLock<ContractState>>>,
    recency: VecDeque<usize>,
}
impl PreparationCache {
    fn entry(&mut self, key: usize) -> Arc<OnceLock<ContractState>> {
        if self.capacity == 0 {
            return Arc::default();
        }
        if let Some(found) = self.entries.get(&key) {
            self.recency.retain(|&entry| entry != key);
            self.recency.push_back(key);
            return found.clone();
        }
        if self.entries.len() == self.capacity {
            let oldest = self.recency.pop_front().expect("occupied cache has a key");
            self.entries.remove(&oldest);
        }
        self.recency.push_back(key);
        self.entries.entry(key).or_default().clone()
    }
}
#[derive(Clone)]
enum ContractState {
    Ready(Arc<dyn PreparedSchema>),
    NoVerdict(NoVerdict),
}
/// Setup result. Only Ready owns a successfully prepared contract.
#[derive(Clone, Debug)]
pub enum ContractPreparation {
    /// A retained prepared contract, independently usable after context drop or cache eviction.
    Ready(PreparedContract),
    /// The selected operation exists but has no field for this side; this is distinct from the present boolean schema `false`.
    NoContract,
    /// No primary key or alias matches the request.
    OperationMissing,
    /// Multiple occurrences of the requested primary key or alias prevent selection.
    OperationAmbiguous {
        #[doc = "Distinct matching primary keys in lexical order; repeated aliases can yield one candidate key."]
        candidates: Vec<String>,
    },
    /// Selection found a contract, but preparation did not establish readiness.
    NoVerdict {
        #[doc = "Truthful preparation refusal; no instance was judged."]
        detail: NoVerdict,
    },
}
/// A ready contract that retains its required compiled state independently of
/// document/context lifetime and cache eviction. Cloning shares immutable state;
/// dropping the final owner releases that owner, without promising lower RSS.
#[derive(Clone)]
pub struct PreparedContract {
    schema: Arc<dyn PreparedSchema>,
}
impl fmt::Debug for PreparedContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedContract { .. }")
    }
}
impl PreparedContract {
    /// Validate an admitted exact value, distinguishing satisfies, fails and
    /// no-verdict. For ordinary Rust data first use [`JsonValue::from_serializable`];
    /// for exact JSON text/bytes use [`JsonValue::parse`]. Admission failure is
    /// separate from validation. Failure diagnostics can be incomplete; inspect
    /// `problems_complete` rather than assuming every failed keyword is reported.
    pub fn validate(&self, value: &JsonValue) -> ValueOutcome {
        self.validate_with_control(value, &WorkControl::new())
    }
    /// Validate with cooperative cancellation. Cancellation yields no-verdict,
    /// not failure, and leaves this owner usable with a fresh healthy control.
    /// This is not a preemptive wall-clock deadline; applications schedule work.
    pub fn validate_with_control(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
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
impl ParsedDocument {
    /// Create a context with an explicitly selected evaluator and immutable
    /// resources. Malformed operation namespaces (including unrelated aliases)
    /// return an [`InterpretationError`] here, before contract selection.
    /// No resource acquisition occurs; URIs in other contexts cannot
    /// change this one. This does not establish whole-document conformance: use
    /// [`Self::assess`] if your application requires that before accepting a document.
    ///
    /// Context construction validates the complete operation namespace, including
    /// unrelated aliases, and returns a located [`InterpretationError`] for malformed
    /// entries. Unrelated metadata may still be a draft: an established value verdict
    /// concerns only the selected schema, not normative OBI conformance.
    ///
    /// The default cache retains four most-recent preparation entries. Match
    /// [`ContractPreparation`] after [`ValueContracts::prepare`]; only its ready
    /// branch exposes validation. The optional `openbindings-json-schema-evaluator`
    /// companion supplies `DefaultEvaluator` and runnable first-use examples.
    pub fn value_contracts(
        &self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
    ) -> Result<ValueContracts, InterpretationError> {
        self.value_contracts_with_options(evaluator, resources, ValueContractOptions::default())
    }
    /// Create the same explicit context as [`Self::value_contracts`] with a chosen
    /// cache entry capacity. Zero disables implicit retention. Eviction and context
    /// drop release cache owners; caller-retained [`PreparedContract`] values stay
    /// usable. The entry count is not a byte or process-memory limit.
    pub fn value_contracts_with_options(
        &self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
        options: ValueContractOptions,
    ) -> Result<ValueContracts, InterpretationError> {
        self.interpretable()?;
        self.names()?;
        Ok(ValueContracts {
            inner: Arc::new(ContractsInner {
                space: Arc::new(SchemaSpace::new(self.clone(), resources)),
                evaluator,
                prepared: Mutex::new(PreparationCache {
                    capacity: options.cache_capacity,
                    entries: HashMap::new(),
                    recency: VecDeque::new(),
                }),
            }),
        })
    }
}
impl ValueContracts {
    /// Select a primary operation name or alias and prepare one side's contract.
    /// Match every [`ContractPreparation`] branch: missing/ambiguous operation,
    /// absent contract and preparation refusal are setup states, not instance failures.
    /// Ready contracts can be retained beyond this context and validated repeatedly.
    /// Deterministic preparations may be reused within the bounded context cache;
    /// concurrent first requests may prepare more than once.
    pub fn prepare(&self, operation: &str, side: Side) -> ContractPreparation {
        self.prepare_with_control(operation, side, &WorkControl::new())
    }
    /// Prepare with cooperative cancellation. A cancelled attempt returns no ready
    /// owner. Cancellation and transient evaluator failures do not poison healthy
    /// retry on this context; use a fresh [`WorkControl`] after cancellation.
    /// A no-verdict refusal does not prove semantic undefinedness. To change supplied
    /// resources, build a new immutable context and decide when to replace active work.
    pub fn prepare_with_control(
        &self,
        operation: &str,
        side: Side,
        control: &WorkControl,
    ) -> ContractPreparation {
        if let Err(detail) = control.check() {
            return ContractPreparation::NoVerdict { detail };
        }
        let op = match self.inner.space.document.resolve_operation(operation) {
            Ok(OperationSelection::Found(op)) => op,
            Ok(OperationSelection::Missing) => return ContractPreparation::OperationMissing,
            Ok(OperationSelection::Ambiguous { candidates }) => {
                return ContractPreparation::OperationAmbiguous { candidates };
            }
            Err(error) => {
                let mut detail = NoVerdict::new(
                    NoVerdictReason::EvaluatorFailure,
                    "operation-index-invariant",
                    "the previously checked operation namespace could not be read",
                );
                if let Some(location) = error.source_location() {
                    detail.location = Some(SchemaLocation {
                        resource: None,
                        pointer: location.pointer.clone().unwrap_or_default(),
                    });
                }
                return ContractPreparation::NoVerdict { detail };
            }
        };
        let Some(schema) = op.value().get(side.as_str()) else {
            return ContractPreparation::NoContract;
        };
        let Some(entry) = self.inner.space.document_node(&schema.to_owned()) else {
            return ContractPreparation::NoVerdict {
                detail: NoVerdict::new(
                    NoVerdictReason::ConservativePreparation,
                    "non-schema-entry",
                    "the operation side is not a schema position",
                )
                .located(SchemaLocation {
                    resource: None,
                    pointer: schema.location().pointer.unwrap_or_default(),
                }),
            };
        };
        let cell = {
            let mut prepared = self
                .inner
                .prepared
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            prepared.entry(entry)
        };
        let state = if let Some(state) = cell.get() {
            state.clone()
        } else {
            let request = SchemaRequest {
                space: self.inner.space.clone(),
                entry,
            };
            let state = match self.inner.evaluator.prepare(&request, control) {
                Ok(schema) => ContractState::Ready(schema),
                Err(detail) => ContractState::NoVerdict(detail),
            };
            if !matches!(&state, ContractState::NoVerdict(detail) if !detail.cacheable()) {
                let _ = cell.set(state.clone());
            }
            state
        };
        // An evaluator may finish while the caller cancels; no ready owner is returned.
        if let Err(detail) = control.check() {
            return ContractPreparation::NoVerdict { detail };
        }
        match state {
            ContractState::Ready(schema) => ContractPreparation::Ready(PreparedContract { schema }),
            ContractState::NoVerdict(detail) => ContractPreparation::NoVerdict { detail },
        }
    }
}

#[cfg(test)]
mod location_budget_tests {
    use super::*;
    #[test]
    fn original_mapping_admits_complete_locations_and_decoding_separately() {
        let uri = "https://sdk-program.openbindings.invalid/p#/$defs/n0";
        let location = SchemaLocation {
            resource: Some("https://example.test/".to_owned() + &"u".repeat(32768)),
            pointer: "/é~0~1\n".into(),
        };
        let program = EvaluationProgram {
            entry_uri: uri.into(),
            resources: vec![],
            locations: BTreeMap::from([(uri.into(), location.clone())]),
        };
        let generated = uri.to_owned() + "/type";
        let required = location.resource.as_ref().unwrap().len() + location.pointer.len() + 5;
        for bytes in [0, 1, required - 1] {
            assert_eq!(
                program.original_location_bounded(&generated, bytes),
                Err(LocationBudgetExceeded)
            );
        }
        for bytes in [required, required + 1] {
            assert_eq!(
                program
                    .original_location_bounded(&generated, bytes)
                    .unwrap(),
                Some(SchemaLocation {
                    resource: location.resource.clone(),
                    pointer: location.pointer.clone() + "/type"
                })
            );
        }
        assert_eq!(
            program.original_location(&generated),
            program
                .original_location_bounded(&generated, usize::MAX)
                .unwrap()
        );
        assert_eq!(
            program
                .original_location_bounded(&(uri.to_owned() + "/%74ype"), required)
                .unwrap(),
            program.original_location(&generated)
        );
        assert_eq!(
            program
                .original_location_bounded(&(uri.to_owned() + "/%GG"), required)
                .unwrap(),
            None
        );
        assert_eq!(
            program
                .original_location_bounded("https://unmapped.test/#/x", 0)
                .unwrap(),
            None
        );
    }
    #[test]
    fn percent_bearing_base_lookup_preserves_original_mapping_contract() {
        let at = SchemaLocation {
            resource: None,
            pointer: String::new(),
        };
        for base in [
            "https://example.test/%61",
            "https://example.test/%61#/$defs/n0",
        ] {
            let program = EvaluationProgram {
                entry_uri: base.into(),
                resources: vec![],
                locations: BTreeMap::from([(base.into(), at.clone())]),
            };
            assert_eq!(program.original_location(base), Some(at.clone()));
            assert_eq!(
                program.original_location_bounded(base, 0),
                Ok(Some(at.clone()))
            );
            assert_eq!(
                program.original_location_bounded(&(base.to_owned() + "/type"), 5),
                Ok(Some(SchemaLocation {
                    resource: None,
                    pointer: "/type".into()
                }))
            );
        }
        let uri = "https://example.test/%61#/%74ype";
        let program = EvaluationProgram {
            entry_uri: uri.into(),
            resources: vec![],
            locations: BTreeMap::from([("https://example.test/%61#/type".into(), at.clone())]),
        };
        assert_eq!(
            program.original_location_bounded(uri, uri.len() - 1),
            Err(LocationBudgetExceeded)
        );
        assert_eq!(
            program.original_location_bounded(uri, uri.len()),
            Ok(Some(at))
        );
    }
}

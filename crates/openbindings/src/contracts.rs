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
pub enum Side {
    Input,
    Output,
}
impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoVerdictReason {
    UnsupportedCapability,
    ConservativePreparation,
    ResourceUnavailable,
    LimitExceeded,
    Cancelled,
    EvaluatorFailure,
    Undefined,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SchemaLocation {
    pub resource: Option<String>,
    pub pointer: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct NoVerdict {
    pub reason: NoVerdictReason,
    pub code: String,
    pub message: String,
    pub location: Option<SchemaLocation>,
}
impl NoVerdict {
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
pub struct ValueProblem {
    pub instance_pointer: String,
    pub schema_location: Option<SchemaLocation>,
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ValueOutcome {
    Satisfies,
    Mismatch {
        problems: Vec<ValueProblem>,
        problems_complete: bool,
    },
    NoVerdict {
        detail: NoVerdict,
    },
}

#[derive(Clone, Debug)]
pub struct SchemaResource {
    pub uri: String,
    pub document: JsonValue,
}
#[derive(Clone, Debug, Default)]
pub struct ResourceSet {
    resources: Arc<Vec<SchemaResource>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceError {
    pub uri: String,
    pub message: String,
}
impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.uri, self.message)
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
    pub fn document(&self) -> &ParsedDocument {
        &self.space.document
    }
    pub fn supplied_resources(&self) -> &ResourceSet {
        &self.space.supplied
    }
    pub fn entry(&self) -> &JsonValue {
        &self.space.nodes[self.entry].value
    }
    pub fn entry_location(&self) -> SchemaLocation {
        self.space.location(self.entry)
    }
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
    pub entry_uri: String,
    pub resources: Vec<SchemaResource>,
    pub(crate) locations: BTreeMap<String, SchemaLocation>,
}
impl EvaluationProgram {
    pub fn original_location(&self, generated_uri: &str) -> Option<SchemaLocation> {
        let normalized = if let Some((base, fragment)) = generated_uri.split_once('#') {
            format!("{base}#{}", crate::uri::decode_fragment(fragment)?)
        } else {
            generated_uri.into()
        };
        let generated_uri = normalized.as_str();
        let mut end = generated_uri.len();
        loop {
            if let Some(location) = self.locations.get(&generated_uri[..end]) {
                let mut out = location.clone();
                out.pointer.push_str(&generated_uri[end..]);
                return Some(out);
            }
            let previous = generated_uri[..end].rfind('/')?;
            end = previous;
        }
    }
}
pub trait SchemaEvaluator: Send + Sync {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict>;
}
pub trait PreparedSchema: Send + Sync {
    /// Return only established verdicts. Resource/capability/work failures are NoVerdict.
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome;
}
#[derive(Clone)]
pub struct ValueContracts {
    inner: Arc<ContractsInner>,
}
/// Retention policy for preparation reuse. Explicitly retained handles are
/// independent of this cache and remain usable after eviction or context drop.
#[derive(Clone, Copy, Debug)]
pub struct ValueContractOptions {
    /// Most-recently-used preparation entries retained by this context.
    /// Zero disables retention. This limits entries, not total allocation bytes.
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
    Ready(PreparedContract),
    NoContract,
    OperationMissing,
    OperationAmbiguous { candidates: Vec<String> },
    NoVerdict { detail: NoVerdict },
}
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
    pub fn validate(&self, value: &JsonValue) -> ValueOutcome {
        self.validate_with_control(value, &WorkControl::new())
    }
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
    pub fn value_contracts(
        &self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
    ) -> Result<ValueContracts, InterpretationError> {
        self.value_contracts_with_options(evaluator, resources, ValueContractOptions::default())
    }
    pub fn value_contracts_with_options(
        &self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
        options: ValueContractOptions,
    ) -> Result<ValueContracts, InterpretationError> {
        self.interpretable()?;
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
    pub fn prepare(&self, operation: &str, side: Side) -> ContractPreparation {
        self.prepare_with_control(operation, side, &WorkControl::new())
    }
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
                    NoVerdictReason::Undefined,
                    "invalid-operation-structure",
                    "the operation structure cannot establish a value contract",
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

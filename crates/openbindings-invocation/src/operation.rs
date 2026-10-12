//! Document-based operation invocation over an explicitly supplied binding provider.
//!
//! Selection is caller-owned. Validation uses an explicitly supplied evaluator;
//! without one, a declared contract cannot yield a verdict. Neither construction
//! nor preflight prepares schemas or establishes whole-document conformance.
use crate::{
    BindingRequest, BindingValue, ContextChallenge, Invocation, InvocationError, InvocationLimits,
    LimitError, ProviderIo,
    binding::{self, BindingInvoker},
    lifecycle::Shared,
    value::source_from_json,
};
use openbindings::{
    BindingView, ContractPreparation, InterpretationError, JsonKind, JsonValue, NoVerdict,
    NoVerdictReason, OperationSelection, OperationView, ParsedDocument, ResourceSet,
    SchemaEvaluator, Side, Source, ValueContracts, ValueOutcome, WorkControl,
};
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Copy)]
enum Target<'a> {
    Operation {
        name: &'a str,
        selection: Option<&'a [&'a str]>,
    },
    Binding(&'a str),
}
/// One borrowed selection and optional opaque context. Constructors make an
/// explicit binding mutually exclusive with operation names and ordered choices.
#[derive(Clone, Copy)]
pub struct Request<'a> {
    target: Target<'a>,
    context: Option<&'a JsonValue>,
}
impl<'a> Request<'a> {
    /// Resolve a primary operation name or alias, requiring a sole supported binding.
    pub fn operation(name: &'a str) -> Self {
        Self {
            target: Target::Operation {
                name,
                selection: None,
            },
            context: None,
        }
    }
    /// Use the first supported binding in this caller-ordered list. Every key
    /// must belong to the operation, even keys after the first supported entry.
    /// Empty lists or empty keys are refused as malformed requests.
    pub fn ordered(name: &'a str, selection: &'a [&'a str]) -> Self {
        Self {
            target: Target::Operation {
                name,
                selection: Some(selection),
            },
            context: None,
        }
    }
    /// Invoke this exact binding; unsupported kinds refuse without fallback.
    pub fn binding(key: &'a str) -> Self {
        Self {
            target: Target::Binding(key),
            context: None,
        }
    }
    /// Supply an opaque object unchanged for this attempt; never resolve credentials.
    pub fn with_context(mut self, context: &'a JsonValue) -> Self {
        self.context = Some(context);
        self
    }
}
impl fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("target", &"<redacted>")
            .field("context_present", &self.context.is_some())
            .finish()
    }
}

/// Reusable document/provider pair. Cloned document storage and prepared schema
/// caches are retained, without a registry, implicit evaluator, network access,
/// retry policy, or whole-document conformance gate.
///
/// The caller must supply a provider whose pure exact-kind support represents
/// its routing policy. Multiple providers can be composed by the application;
/// registration order never becomes SDK selection policy.
pub struct Invoker<P> {
    document: ParsedDocument,
    binding: binding::Invoker<P>,
    contracts: Option<ValueContracts>,
    sources: CarrierCache<Source>,
    bindings: CarrierCache<BindingValue>,
}
impl<P: BindingInvoker> Invoker<P> {
    /// Retain a document and an explicit binding provider. Declared schemas
    /// yield `ERR_SCHEMA_UNRESOLVED` on actual values until an evaluator is supplied.
    /// Sides without a schema forward values without claiming validation.
    pub fn new(document: ParsedDocument, provider: P) -> Self {
        Self {
            document,
            binding: binding::Invoker::new(provider),
            contracts: None,
            sources: OnceLock::new(),
            bindings: OnceLock::new(),
        }
    }
    /// Set the explicit evaluator and immutable supplied resources, using core's
    /// bounded preparation cache. No schema is prepared and no URI is fetched.
    /// Malformed operation namespaces produce a core interpretation error here.
    pub fn with_evaluator(
        mut self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
    ) -> Result<Self, InterpretationError> {
        self.contracts = Some(self.document.value_contracts(evaluator, resources)?);
        Ok(self)
    }
    /// Set the same finite handoff policy used by binding invocation.
    pub fn with_limits(mut self, limits: InvocationLimits) -> Result<Self, LimitError> {
        self.binding.limits = limits.validate()?;
        Ok(self)
    }
    /// Resolve names and pure support synchronously, then construct a lazy provider
    /// invocation using the shared binding invocation lifecycle. No schema is
    /// prepared and no provider job is started until the handle is polled.
    pub fn invoke<'a>(
        &'a self,
        request: Request<'a>,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        self.invoke_with_control(request, &WorkControl::new())
    }
    /// Observe caller cancellation without cancelling that token or sibling work.
    /// Schema evaluation is cooperative, not a preemptive wall-clock deadline.
    pub fn invoke_with_control<'a>(
        &'a self,
        request: Request<'a>,
        control: &WorkControl,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        let shared = Shared::new(self.binding.limits);
        let state = shared.clone();
        let local = shared.control();
        let external = control.clone();
        // Pure selection is synchronous; retained carriers allow the provider's
        // borrowed GAT to stay outside any self-referential async state machine.
        let selected = if external.is_cancelled() {
            Err(InvocationError::cancelled())
        } else {
            self.resolve(request)
        };
        let io_external = external.clone();
        let provider_control = local.clone();
        let future = binding::checked(
            move || {
                let selected = selected?;
                let io = ProviderIo::with_gates(
                    state.clone(),
                    self.gate(&selected.operation, Side::Input, &io_external),
                    self.gate(&selected.operation, Side::Output, &io_external),
                );
                let binding_request = BindingRequest {
                    source: selected.source,
                    binding: selected.binding,
                    context: request.context,
                };
                binding_request.admit().map_err(|finding| {
                    state.record_finding(finding);
                    InvocationError::refused()
                })?;
                if !self
                    .binding
                    .provider
                    .supports_kind(&binding_request.source.kind)
                {
                    return Err(InvocationError::refused());
                }
                Ok(self
                    .binding
                    .provider
                    .invoke(binding_request, io, provider_control))
            },
            external,
            local,
        );
        Invocation::new(shared, future)
    }
    /// Resolve by the same rules and delegate advisory preflight. It never prepares
    /// schemas, consumes values, dispatches, or pins a later selection.
    pub async fn preflight(
        &self,
        request: Request<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        self.preflight_with_control(request, &WorkControl::new())
            .await
    }
    /// Advisory preflight with cancellation, isolated from sibling attempts.
    pub async fn preflight_with_control(
        &self,
        request: Request<'_>,
        control: &WorkControl,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        let selected = self.resolve(request)?;
        self.binding
            .preflight_with_control(
                BindingRequest {
                    source: selected.source,
                    binding: selected.binding,
                    context: request.context,
                },
                control,
            )
            .await
    }
    fn gate(&self, operation: &OperationView, side: Side, external: &WorkControl) -> Option<Gate> {
        operation.value().get(side.as_str()).map(|_| Gate {
            contracts: self.contracts.clone(),
            operation: operation.key().into(),
            side,
            prepared: None,
            external: external.clone(),
        })
    }
    fn operation(&self, name: &str) -> Result<OperationView, InvocationError> {
        match self
            .document
            .resolve_operation(name)
            .map_err(|_| InvocationError::refused())?
        {
            OperationSelection::Found(view) => Ok(view),
            OperationSelection::Missing | OperationSelection::Ambiguous { .. } => {
                Err(error("ERR_OPERATION_NOT_FOUND"))
            }
        }
    }
    fn binding(&self, key: &str) -> Result<BindingView, InvocationError> {
        self.document
            .binding(key)
            .map_err(|_| InvocationError::refused())?
            .ok_or_else(|| error("ERR_BINDING_NOT_FOUND"))
    }
    fn supported(&self, binding: &BindingView) -> Result<bool, InvocationError> {
        let source = binding.source().map_err(|_| InvocationError::refused())?;
        let Some(source) = self
            .document
            .source(source)
            .map_err(|_| InvocationError::refused())?
        else {
            return Ok(false);
        };
        let kind = source.kind().map_err(|_| InvocationError::refused())?;
        Ok(!kind.is_empty() && self.binding.provider.supports_kind(kind))
    }
    fn resolve(&self, request: Request<'_>) -> Result<Resolved<'_>, InvocationError> {
        if request
            .context
            .is_some_and(|v| v.kind() != JsonKind::Object)
        {
            return Err(InvocationError::refused());
        }
        let (operation, binding) = match request.target {
            Target::Binding(key) => {
                let binding = self.binding(key)?;
                let name = binding
                    .operation()
                    .map_err(|_| InvocationError::refused())?;
                let operation = self.operation(name)?;
                if operation.key() != name {
                    return Err(error("ERR_OPERATION_NOT_FOUND"));
                }
                (operation, binding)
            }
            Target::Operation { name, selection } => {
                let operation = self.operation(name)?;
                let binding = if let Some(keys) = selection {
                    if keys.is_empty() || keys.iter().any(|key| key.is_empty()) {
                        return Err(InvocationError::refused());
                    }
                    // Validate the complete list before asking support or choosing a prefix.
                    let bindings = keys
                        .iter()
                        .map(|key| {
                            let binding = self.binding(key)?;
                            if binding
                                .operation()
                                .map_err(|_| InvocationError::refused())?
                                != operation.key()
                            {
                                return Err(error("ERR_BINDING_NOT_FOUND"));
                            }
                            Ok(binding)
                        })
                        .collect::<Result<Vec<_>, InvocationError>>()?;
                    let mut selected = None;
                    for binding in bindings {
                        if self.supported(&binding)? {
                            selected = Some(binding);
                            break;
                        }
                    }
                    selected.ok_or_else(|| error("ERR_BINDING_NOT_FOUND"))?
                } else {
                    let mut selected = None;
                    for key in operation
                        .bindings()
                        .map_err(|_| InvocationError::refused())?
                    {
                        let binding = self.binding(&key)?;
                        if self.supported(&binding)? {
                            if selected.is_some() {
                                return Err(error("ERR_BINDING_SELECTION_REQUIRED"));
                            }
                            selected = Some(binding);
                        }
                    }
                    selected.ok_or_else(|| error("ERR_BINDING_NOT_FOUND"))?
                };
                (operation, binding)
            }
        };
        let source_key = binding.source().map_err(|_| InvocationError::refused())?;
        let source = self
            .document
            .source(source_key)
            .map_err(|_| InvocationError::refused())?
            .ok_or_else(|| error("ERR_UNKNOWN_SOURCE"))?;
        Ok(Resolved {
            operation,
            source: self.carrier(&self.sources, "sources", source.key(), || {
                source_from_json(source.value())
            })?,
            binding: self.carrier(&self.bindings, "bindings", binding.key(), || {
                BindingValue::from_json_ref(binding.value())
            })?,
        })
    }
    fn carrier<'a, T>(
        &'a self,
        cache: &'a CarrierCache<T>,
        namespace: &str,
        key: &str,
        decode: impl FnOnce() -> Result<T, crate::BoundaryError>,
    ) -> Result<&'a T, InvocationError> {
        // Only declared keys get slots. Unknown requests cannot grow this cache.
        // Core resolution already checked interpretation before this is reached.
        let slots = cache.get_or_init(|| {
            self.document
                .value()
                .get(namespace)
                .and_then(|v| v.members())
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    m.name
                        .as_str()
                        .map(|name| (name.to_owned(), OnceLock::new()))
                })
                .collect()
        });
        let slot = slots.get(key).ok_or_else(InvocationError::refused)?;
        slot.get_or_init(decode)
            .as_ref()
            .map_err(|_| InvocationError::refused())
    }
}
type CarrierCache<T> = OnceLock<HashMap<String, OnceLock<Result<T, crate::BoundaryError>>>>;
struct Resolved<'a> {
    operation: OperationView,
    source: &'a Source,
    binding: &'a BindingValue,
}
fn error(code: &str) -> InvocationError {
    InvocationError::new(code, None).expect("fixed nonempty code")
}

/// Where operation validation stopped. Preparation is lazy, on the first value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationStage {
    /// A declared schema could not be prepared with the supplied evaluator/resources.
    Preparation,
    /// A prepared schema failed to establish satisfaction for this value.
    Evaluation,
}
/// Local schema evidence for the winning unsuccessful completion. Never placed
/// in portable error data. Custom evaluator text and source locations can be
/// sensitive: inspect deliberately under your application's disclosure policy.
/// Debug intentionally omits that evidence. Retaining this record retains its
/// diagnostic storage, not a value, provider, document, or prepared contract.
pub struct ValidationFailure {
    side: Side,
    stage: ValidationStage,
    outcome: ValueOutcome,
}
impl ValidationFailure {
    /// Input or output side whose value was withheld.
    pub fn side(&self) -> Side {
        self.side
    }
    /// Whether preparation or evaluation could not establish satisfaction.
    pub fn stage(&self) -> ValidationStage {
        self.stage
    }
    /// Borrow the original core verdict and its diagnostic completeness metadata.
    pub fn outcome(&self) -> &ValueOutcome {
        &self.outcome
    }
}
impl fmt::Debug for ValidationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationFailure")
            .field("side", &self.side)
            .field("stage", &self.stage)
            .finish_non_exhaustive()
    }
}
pub(crate) struct Gate {
    contracts: Option<ValueContracts>,
    operation: String,
    side: Side,
    prepared: Option<ContractPreparation>,
    external: WorkControl,
}
impl Gate {
    pub(crate) fn check(
        &mut self,
        value: &JsonValue,
        local: &WorkControl,
    ) -> Result<(), (InvocationError, Option<Arc<ValidationFailure>>)> {
        if local.is_cancelled() || self.external.is_cancelled() {
            return Err((InvocationError::cancelled(), None));
        }
        let prepared = self.prepared.get_or_insert_with(|| match &self.contracts {
            Some(contracts) => contracts.prepare_with_control(&self.operation, self.side, local),
            None => ContractPreparation::NoVerdict {
                detail: NoVerdict::new(
                    NoVerdictReason::UnsupportedCapability,
                    "operation-evaluator-missing",
                    "supply a schema evaluator to validate declared operation contracts",
                ),
            },
        });
        let (stage, outcome) = match prepared {
            ContractPreparation::Ready(contract) => (
                ValidationStage::Evaluation,
                contract.validate_with_control(value, local),
            ),
            ContractPreparation::NoVerdict { detail } => (
                ValidationStage::Preparation,
                ValueOutcome::NoVerdict {
                    detail: detail.clone(),
                },
            ),
            // The immutable, uniquely resolved operation had a declared side.
            _ => (
                ValidationStage::Preparation,
                ValueOutcome::NoVerdict {
                    detail: NoVerdict::new(
                        NoVerdictReason::EvaluatorFailure,
                        "operation-contract-invariant",
                        "the selected declared operation contract could not be prepared",
                    ),
                },
            ),
        };
        if local.is_cancelled() || self.external.is_cancelled() {
            return Err((InvocationError::cancelled(), None));
        }
        let code = match &outcome {
            ValueOutcome::Satisfies => return Ok(()),
            ValueOutcome::Fails { .. } => "ERR_OPERATION_VALIDATION_FAILED",
            ValueOutcome::NoVerdict { .. } => "ERR_SCHEMA_UNRESOLVED",
        };
        Err((
            error(code),
            Some(Arc::new(ValidationFailure {
                side: self.side,
                stage,
                outcome,
            })),
        ))
    }
}

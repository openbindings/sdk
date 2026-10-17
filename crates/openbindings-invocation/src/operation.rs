//! Document-based operation invocation over an explicitly supplied binding provider.
//!
//! Selection is caller-owned. Validation uses an explicitly supplied evaluator;
//! without one, a declared value contract cannot yield a verdict. Neither construction
//! nor preflight prepares schemas; construction requires document conformance proof.
use crate::CancellationToken;
use crate::{
    BindingRequest, BindingValue, ContextChallenge, Invocation, InvocationError, InvocationLimits,
    LimitError, ProviderIo,
    binding::{self, BindingInvoker, BindingPreflight},
    lifecycle::Shared,
    value::source_from_json,
};
use openbindings::{
    Conformant, ConformantBindingView, ConformantOperationView, Document, JsonKind, JsonValue,
    NoVerdict, NoVerdictReason, PreparedContract, ResourceSet, SchemaEvaluator, Source,
    ValueContract, ValueOutcome,
};
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    sync::{Arc, OnceLock},
};

/// The side of an operation a value travels on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// The operation's input, sent to the provider.
    Input,
    /// The operation's output, produced by the provider.
    Output,
}
impl Direction {
    /// Return the operation field spelling, `input` or `output`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
    fn contract(self, operation: &ConformantOperationView) -> Option<ValueContract<'_>> {
        match self {
            Self::Input => operation.input(),
            Self::Output => operation.output(),
        }
    }
}
/// The explicit evaluator and immutable resources every preparation borrows.
#[derive(Clone)]
struct Evaluation {
    evaluator: Arc<dyn SchemaEvaluator>,
    resources: ResourceSet,
}
impl Evaluation {
    fn prepare(
        &self,
        contract: ValueContract<'_>,
        control: &dyn openbindings::WorkControl,
    ) -> Result<PreparedContract, NoVerdict> {
        contract.prepare_with_control(&*self.evaluator, &self.resources, control)
    }
}
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
    /// Invoke this exact binding; unsupported kinds yield `ERR_BINDING_NOT_FOUND`.
    pub fn binding(key: &'a str) -> Self {
        Self {
            target: Target::Binding(key),
            context: None,
        }
    }
    /// Supply an opaque object unchanged for this attempt; never resolve credentials.
    #[must_use = "use the returned value to retain this configuration"]
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

#[derive(Clone, Copy)]
enum BindingChoice<'a> {
    Sole,
    Ordered(&'a [&'a str]),
    Exact(&'a str),
}
/// One prepared operation attempt's binding choice and optional opaque context.
/// The operation is fixed by the [`PreparedOperation`] handle.
#[derive(Clone, Copy)]
pub struct Attempt<'a> {
    selection: BindingChoice<'a>,
    context: Option<&'a JsonValue>,
}
impl<'a> Attempt<'a> {
    /// Require a sole supported binding for the prepared operation.
    pub fn new() -> Self {
        Self {
            selection: BindingChoice::Sole,
            context: None,
        }
    }
    /// Use the first supported binding in this caller-ordered list. Every key
    /// must belong to the prepared operation, including keys after the first
    /// supported entry. Empty lists or empty keys are refused as malformed attempts.
    pub fn ordered(selection: &'a [&'a str]) -> Self {
        Self {
            selection: BindingChoice::Ordered(selection),
            context: None,
        }
    }
    /// Invoke this exact binding of the prepared operation. A binding belonging
    /// to another operation yields `ERR_OPERATION_NOT_FOUND` before provider work;
    /// missing bindings or unsupported kinds yield `ERR_BINDING_NOT_FOUND`.
    pub fn binding(key: &'a str) -> Self {
        Self {
            selection: BindingChoice::Exact(key),
            context: None,
        }
    }
    /// Supply an opaque object unchanged for this attempt; never resolve credentials.
    #[must_use = "use the returned value to retain this configuration"]
    pub fn with_context(mut self, context: &'a JsonValue) -> Self {
        self.context = Some(context);
        self
    }
}
impl Default for Attempt<'_> {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Debug for Attempt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attempt")
            .field("selection", &"<redacted>")
            .field("context_present", &self.context.is_some())
            .finish()
    }
}

/// Reusable document/provider pair. Cloned document storage is retained,
/// without a preparation cache, registry, implicit evaluator, network access,
/// or retry policy. Construction requires a conformant document.
///
/// The caller must supply a provider whose pure exact-kind support represents
/// its routing policy. Multiple providers can be composed by the application;
/// registration order never becomes SDK selection policy.
pub struct Invoker<P> {
    document: Conformant<Document>,
    binding: binding::Invoker<P>,
    evaluation: Option<Evaluation>,
    sources: CarrierCache<Source>,
    bindings: CarrierCache<BindingValue>,
}
impl<P> std::fmt::Debug for Invoker<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invoker").finish_non_exhaustive()
    }
}

impl<P: BindingInvoker> Invoker<P> {
    /// Retain a document and an explicit binding provider.
    /// ```compile_fail
    /// use openbindings::Document;
    /// use openbindings_invocation::{binding::BindingInvoker, operation::Invoker};
    /// fn draft<P: BindingInvoker>(document: Document, provider: P) {
    ///     let _ = Invoker::new(document, provider);
    /// }
    /// ```
    ///
    /// Declared schemas
    /// yield `ERR_SCHEMA_UNRESOLVED` on actual values until an evaluator is supplied.
    /// Sides without a schema forward values without claiming validation.
    pub fn new(document: Conformant<Document>, provider: P) -> Self {
        Self {
            document,
            binding: binding::Invoker::new(provider),
            evaluation: None,
            sources: OnceLock::new(),
            bindings: OnceLock::new(),
        }
    }
    /// Set the explicit evaluator and immutable supplied resources.
    /// No schema is prepared and no URI is fetched.
    /// The document proof already establishes typed namespace shapes.
    #[must_use = "use the returned value to retain this configuration"]
    pub fn with_evaluator(
        mut self,
        evaluator: Arc<dyn SchemaEvaluator>,
        resources: ResourceSet,
    ) -> Self {
        self.evaluation = Some(Evaluation {
            evaluator,
            resources,
        });
        self
    }
    /// Set the same finite handoff policy used by binding invocation.
    ///
    /// # Errors
    /// Returns `LimitError` if a queue capacity or the value byte limit is zero, or the error byte limit is below 256 bytes.
    pub fn with_limits(mut self, limits: InvocationLimits) -> Result<Self, LimitError> {
        self.binding.limits = limits.validate()?;
        Ok(self)
    }
    /// Prepare both declared value contracts for a primary operation name or alias.
    /// Retain the returned handle to reuse compiled state across attempts. This
    /// synchronously prepares schemas, without selecting a binding or calling the
    /// provider. Absent schemas need no evaluator; a declared schema without one
    /// returns `ERR_SCHEMA_UNRESOLVED`. Missing names return
    /// `ERR_OPERATION_NOT_FOUND`. Unrepresentable metadata returns `ERR_REFUSED`.
    /// Preparation refusal returns `ERR_SCHEMA_UNRESOLVED` and retains no handle.
    /// Ready means prepared, not that every value will receive a verdict.
    ///
    /// # Errors
    /// Returns `ERR_OPERATION_NOT_FOUND` for a missing operation, `ERR_REFUSED` for unrepresentable metadata, or `ERR_SCHEMA_UNRESOLVED` when a declared value contract cannot be prepared. Cooperative cancellation returns `ERR_CANCELLED`.
    pub fn prepare(&self, operation: &str) -> Result<PreparedOperation<'_, P>, InvocationError> {
        self.prepare_with_control(operation, &CancellationToken::new())
    }
    /// Prepare with cooperative caller cancellation. Cancellation returns
    /// `ERR_CANCELLED`; any contracts prepared so far are released on error.
    /// A failed call stores nothing in this invoker; a fresh call prepares anew.
    ///
    /// # Errors
    /// Returns `ERR_OPERATION_NOT_FOUND` for a missing operation, `ERR_REFUSED` for unrepresentable metadata, or `ERR_SCHEMA_UNRESOLVED` when a declared value contract cannot be prepared. Cooperative cancellation returns `ERR_CANCELLED`.
    pub fn prepare_with_control(
        &self,
        operation: &str,
        control: &CancellationToken,
    ) -> Result<PreparedOperation<'_, P>, InvocationError> {
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        let operation = self.operation(operation)?;
        let prepare = |direction: Direction| {
            if control.is_cancelled() {
                return Err(InvocationError::cancelled());
            }
            let Some(contract) = direction.contract(&operation) else {
                return Ok(None);
            };
            let evaluation = self
                .evaluation
                .as_ref()
                .ok_or_else(|| error("ERR_SCHEMA_UNRESOLVED"))?;
            match evaluation.prepare(contract, control) {
                Ok(contract) => Ok(Some(contract)),
                Err(detail) if detail.reason == NoVerdictReason::Cancelled => {
                    Err(InvocationError::cancelled())
                }
                Err(_) => Err(error("ERR_SCHEMA_UNRESOLVED")),
            }
        };
        let input = prepare(Direction::Input)?;
        let output = prepare(Direction::Output)?;
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        Ok(PreparedOperation {
            invoker: self,
            operation: operation.key().into(),
            input,
            output,
        })
    }
    /// Resolve names and pure support synchronously, then construct a lazy provider
    /// invocation using the shared binding invocation lifecycle. No schema is
    /// prepared and no provider job is started until the handle is polled.
    pub fn invoke<'a>(
        &'a self,
        request: Request<'a>,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        self.invoke_with_control(request, &CancellationToken::new())
    }
    /// Observe caller cancellation without cancelling that token or sibling work.
    /// Schema evaluation is cooperative, not a preemptive wall-clock deadline.
    pub fn invoke_with_control<'a>(
        &'a self,
        request: Request<'a>,
        control: &CancellationToken,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        self.invoke_prepared_with_control(request, control, None)
    }
    fn invoke_prepared_with_control<'a>(
        &'a self,
        request: Request<'a>,
        control: &CancellationToken,
        prepared: Option<&'a PreparedOperation<'_, P>>,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        let shared = Shared::new(self.binding.limits);
        let state = shared.clone();
        let local = shared.control();
        let external = control.clone();
        // Pure selection is synchronous; retained carriers allow the provider's
        // borrowed GAT to stay outside any self-referential async state machine.
        let mut finding = None;
        let selected = if external.is_cancelled() {
            Err(InvocationError::cancelled())
        } else {
            self.resolve(
                request,
                prepared.map(|p| p.operation.as_str()),
                &mut finding,
            )
        };
        if let Some(finding) = finding {
            state.record_finding(finding);
        }
        let io_external = external.clone();
        let provider_control = local.clone();
        let future = binding::checked(
            move || {
                let selected = selected?;
                let io = ProviderIo::with_gates(
                    state.clone(),
                    self.gate(
                        &selected.operation,
                        Direction::Input,
                        &io_external,
                        prepared.and_then(|p| p.input.as_ref()),
                    ),
                    self.gate(
                        &selected.operation,
                        Direction::Output,
                        &io_external,
                        prepared.and_then(|p| p.output.as_ref()),
                    ),
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
                    return Err(error("ERR_BINDING_NOT_FOUND"));
                }
                Ok(self
                    .binding
                    .provider
                    .invoke(binding_request, io, provider_control))
            },
            external,
            local,
            shared.clone(),
        );
        Invocation::new(shared, future)
    }
    /// Resolve by the same rules and make no prediction. Use
    /// `preflight_with_provider` to opt into the optional provider hook. Neither
    /// path prepares schemas, consumes values, dispatches, or pins selection.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. A missing operation returns `ERR_OPERATION_NOT_FOUND`; unrepresentable metadata returns `ERR_REFUSED`.
    pub async fn preflight(
        &self,
        request: Request<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        self.preflight_with_control(request, &CancellationToken::new())
            .await
    }
    /// Advisory preflight with cancellation, isolated from sibling attempts.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. A missing operation returns `ERR_OPERATION_NOT_FOUND`; unrepresentable metadata returns `ERR_REFUSED`.
    pub async fn preflight_with_control(
        &self,
        request: Request<'_>,
        control: &CancellationToken,
    ) -> Result<Option<ContextChallenge>, InvocationError> {
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        let selected = self.resolve(request, None, &mut None)?;
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
    /// Resolve and call the optional provider preflight hook. Its named future
    /// permits generic callers to require `P::Preflight<'a>: Send`.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. A missing operation returns `ERR_OPERATION_NOT_FOUND`; unrepresentable metadata returns `ERR_REFUSED`. Provider refusals and malformed provider challenges also return an error.
    pub async fn preflight_with_provider(
        &self,
        request: Request<'_>,
    ) -> Result<Option<ContextChallenge>, InvocationError>
    where
        P: BindingPreflight,
    {
        self.preflight_with_provider_and_control(request, &CancellationToken::new())
            .await
    }
    /// Call the optional provider preflight hook with caller cancellation.
    ///
    /// # Errors
    /// Returns an invocation error for malformed request fields, unsupported or missing binding selection, or caller cancellation. A missing operation returns `ERR_OPERATION_NOT_FOUND`; unrepresentable metadata returns `ERR_REFUSED`. Provider refusals and malformed provider challenges also return an error.
    pub async fn preflight_with_provider_and_control(
        &self,
        request: Request<'_>,
        control: &CancellationToken,
    ) -> Result<Option<ContextChallenge>, InvocationError>
    where
        P: BindingPreflight,
    {
        if control.is_cancelled() {
            return Err(InvocationError::cancelled());
        }
        let selected = self.resolve(request, None, &mut None)?;
        self.binding
            .preflight_with_provider_and_control(
                BindingRequest {
                    source: selected.source,
                    binding: selected.binding,
                    context: request.context,
                },
                control,
            )
            .await
    }
    fn gate(
        &self,
        operation: &ConformantOperationView,
        direction: Direction,
        external: &CancellationToken,
        prepared: Option<&PreparedContract>,
    ) -> Option<Gate> {
        direction.contract(operation).map(|_| Gate {
            evaluation: self.evaluation.clone(),
            operation: operation.clone(),
            direction,
            prepared: prepared.cloned().map(Ok),
            external: external.clone(),
        })
    }
    fn operation(&self, name: &str) -> Result<ConformantOperationView, InvocationError> {
        self.document
            .resolve_operation(name)
            .ok_or_else(|| error("ERR_OPERATION_NOT_FOUND"))
    }
    fn binding(&self, key: &str) -> Result<ConformantBindingView, InvocationError> {
        self.document
            .binding(key)
            .ok_or_else(|| error("ERR_BINDING_NOT_FOUND"))
    }
    fn supported(&self, binding: &ConformantBindingView) -> Result<bool, InvocationError> {
        let source = self
            .document
            .source(binding.source())
            .expect("OBI-07 proves source existence");
        Ok(self.binding.provider.supports_kind(source.kind()))
    }
    fn resolve(
        &self,
        request: Request<'_>,
        prepared_operation: Option<&str>,
        finding: &mut Option<crate::BoundaryError>,
    ) -> Result<Resolved<'_>, InvocationError> {
        if request
            .context
            .is_some_and(|v| v.kind() != JsonKind::Object)
        {
            return Err(InvocationError::refused());
        }
        let (operation, binding) = match request.target {
            Target::Binding(key) => {
                let binding = self.binding(key)?;
                let name = binding.operation();
                if prepared_operation.is_some_and(|operation| operation != name) {
                    return Err(error("ERR_OPERATION_NOT_FOUND"));
                }
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
                            if binding.operation() != operation.key() {
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
                    for key in operation.bindings() {
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
        let source_key = binding.source();
        let source = self
            .document
            .source(source_key)
            .ok_or_else(|| error("ERR_UNKNOWN_SOURCE"))?;
        if !self.supported(&binding)? {
            return Err(error("ERR_BINDING_NOT_FOUND"));
        }
        Ok(Resolved {
            operation,
            source: self.carrier(&self.sources, "sources", source.key(), finding, || {
                source_from_json(source.value())
            })?,
            binding: self.carrier(&self.bindings, "bindings", binding.key(), finding, || {
                BindingValue::from_json_ref(binding.value())
            })?,
        })
    }
    fn carrier<'a, T>(
        &'a self,
        cache: &'a CarrierCache<T>,
        namespace: &str,
        key: &str,
        finding: &mut Option<crate::BoundaryError>,
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
        slot.get_or_init(decode).as_ref().map_err(|error| {
            *finding = Some(*error);
            InvocationError::refused()
        })
    }
}
/// Caller-owned prepared input and output contracts for one operation.
///
/// Obtain this handle through [`Invoker::prepare`] or [`Invoker::prepare_with_control`].
/// It borrows its invoker, keeping the document, provider, evaluator and resources
/// fixed for its lifetime. Each attempt supplies its own binding selection and
/// context, with independent cancellation, queues and validation evidence. Sharing
/// a handle does not share attempt state. Dropping it releases its contract owners.
pub struct PreparedOperation<'a, P> {
    invoker: &'a Invoker<P>,
    operation: String,
    input: Option<PreparedContract>,
    output: Option<PreparedContract>,
}
impl<P> std::fmt::Debug for PreparedOperation<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedOperation").finish_non_exhaustive()
    }
}

impl<P: BindingInvoker> PreparedOperation<'_, P> {
    /// Invoke using retained contracts and this attempt's selection and context.
    /// The operation is fixed by this handle. An exact binding belonging to
    /// another operation returns `ERR_OPERATION_NOT_FOUND`. Binding selection
    /// follows [`Invoker::invoke`], and provider work remains lazy until polling.
    pub fn invoke<'a>(
        &'a self,
        attempt: Attempt<'a>,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        self.invoke_with_control(attempt, &CancellationToken::new())
    }
    /// Invoke with caller cancellation isolated from this handle and all sibling
    /// attempts. Cancellation does not discard the retained prepared contracts.
    pub fn invoke_with_control<'a>(
        &'a self,
        attempt: Attempt<'a>,
        control: &CancellationToken,
    ) -> Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P>> {
        let target = match attempt.selection {
            BindingChoice::Sole => Target::Operation {
                name: &self.operation,
                selection: None,
            },
            BindingChoice::Ordered(selection) => Target::Operation {
                name: &self.operation,
                selection: Some(selection),
            },
            BindingChoice::Exact(key) => Target::Binding(key),
        };
        let request = Request {
            target,
            context: attempt.context,
        };
        self.invoker
            .invoke_prepared_with_control(request, control, Some(self))
    }
}
type CarrierCache<T> = OnceLock<HashMap<String, OnceLock<Result<T, crate::BoundaryError>>>>;
struct Resolved<'a> {
    operation: ConformantOperationView,
    source: &'a Source,
    binding: &'a BindingValue,
}
fn error(code: &str) -> InvocationError {
    InvocationError::new(code, None).expect("fixed nonempty code")
}

/// Where operation validation stopped. Plain invocation prepares lazily on the
/// first value; a prepared operation has already completed preparation.
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
/// diagnostic storage, not a value, provider, document, or prepared value contract.
///
/// The invoker withholds values with the cheap check, so an evaluation failure
/// carries the evaluator's verdict without problems. To learn why a value
/// fails, explain it with a contract prepared from the same operation.
pub struct ValidationFailure {
    direction: Direction,
    stage: ValidationStage,
    outcome: ValueOutcome,
}
impl ValidationFailure {
    /// Input or output direction whose value was withheld.
    pub fn direction(&self) -> Direction {
        self.direction
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
            .field("direction", &self.direction)
            .field("stage", &self.stage)
            .finish_non_exhaustive()
    }
}
pub(crate) struct Gate {
    evaluation: Option<Evaluation>,
    operation: ConformantOperationView,
    direction: Direction,
    prepared: Option<Result<PreparedContract, NoVerdict>>,
    external: CancellationToken,
}
impl Gate {
    pub(crate) fn check(
        &mut self,
        value: &JsonValue,
        local: &CancellationToken,
    ) -> Result<(), (InvocationError, Option<Arc<ValidationFailure>>)> {
        if local.is_cancelled() || self.external.is_cancelled() {
            return Err((InvocationError::cancelled(), None));
        }
        let (evaluation, operation, direction) =
            (&self.evaluation, &self.operation, self.direction);
        let prepared = self.prepared.get_or_insert_with(|| {
            // A gate exists only for a declared contract of its immutable operation.
            let Some(contract) = direction.contract(operation) else {
                return Err(NoVerdict::new(
                    NoVerdictReason::EvaluatorFailure,
                    "value-contract-invariant",
                    "the selected declared value contract could not be prepared",
                ));
            };
            match evaluation {
                Some(evaluation) => evaluation.prepare(contract, local),
                None => Err(NoVerdict::new(
                    NoVerdictReason::UnsupportedCapability,
                    "operation-evaluator-missing",
                    "supply a schema evaluator to validate declared value contracts",
                )),
            }
        });
        let (stage, outcome) = match prepared {
            Ok(contract) => (
                ValidationStage::Evaluation,
                contract.validate_with_control(value, local),
            ),
            Err(detail) => (
                ValidationStage::Preparation,
                ValueOutcome::NoVerdict {
                    detail: detail.clone(),
                },
            ),
        };
        if local.is_cancelled() || self.external.is_cancelled() {
            return Err((InvocationError::cancelled(), None));
        }
        let code = match &outcome {
            ValueOutcome::Satisfies => return Ok(()),
            ValueOutcome::Fails { .. } => "ERR_OPERATION_VALIDATION_FAILED",
            ValueOutcome::Undefined { .. } | ValueOutcome::NoVerdict { .. } => {
                "ERR_SCHEMA_UNRESOLVED"
            }
        };
        Err((
            error(code),
            Some(Arc::new(ValidationFailure {
                direction: self.direction,
                stage,
                outcome,
            })),
        ))
    }
}

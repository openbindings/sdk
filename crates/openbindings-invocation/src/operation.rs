//! Operation addressing, caller-owned selection, and typed value validation.
use crate::ContextChallenge;
mod prepared;
mod request;
use crate::{
    BindingRequest, BoundaryFailure, CancellationToken, Invocation, InvocationError,
    InvocationLimits, LimitError, ProviderIo,
    binding::{self, BindingInvoker, InvocationSupport, error},
    channel::{self, Shared},
};
use openbindings::{
    ConformantDocument, Detail, ExternalSchemas, JsonValue, NoVerdictReason, Operation,
    PreparationFailure, PreparationFailureKind, PreparationLimits, PreparedSchema, Schema,
    SchemaEvaluator, SchemaValidator, ValueOutcome,
};
pub use prepared::{BindingChoice, OperationPreparationFailure, PreparedOperation};
pub use request::{OperationAddress, OperationRequest};
use std::{
    fmt,
    future::{Future, poll_fn},
    pin::pin,
    sync::Arc,
    task::Poll,
};

/// The operation schema governing a caller-facing value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Input checked before provider delivery.
    Input,
    /// Output checked before caller-visible commitment.
    Output,
}
impl Direction {
    /// The corresponding operation field spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}
/// Original typed evidence; preparation does not fabricate a per-value outcome.
#[derive(Debug)]
pub enum ValidationEvidence {
    /// The declared schema could not be prepared.
    Preparation(PreparationFailure),
    /// Evaluating this value did not establish satisfaction.
    Evaluation(ValueOutcome),
}
/// Local evidence for the winning validation terminal. Never portable error data.
pub struct ValidationFailure {
    direction: Direction,
    evidence: ValidationEvidence,
}
impl ValidationFailure {
    /// Which value boundary withheld the value.
    pub fn direction(&self) -> Direction {
        self.direction
    }
    /// Inspect original preparation or per-value evidence deliberately.
    pub fn evidence(&self) -> &ValidationEvidence {
        &self.evidence
    }
}
impl fmt::Debug for ValidationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationFailure")
            .field("direction", &self.direction)
            .field(
                "stage",
                &match self.evidence {
                    ValidationEvidence::Preparation(_) => "preparation",
                    ValidationEvidence::Evaluation(_) => "evaluation",
                },
            )
            .finish_non_exhaustive()
    }
}
type CheckResult = Result<(), (InvocationError, Option<Arc<ValidationFailure>>)>;

/// Explicit provider, evaluator and external resources. No global cache or async runtime.
pub struct DefaultOperationInvoker<P, E> {
    provider: P,
    evaluator: E,
    external: ExternalSchemas,
    limits: InvocationLimits,
    preparation: PreparationLimits,
    detail: Detail,
}
impl<P, E> fmt::Debug for DefaultOperationInvoker<P, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DefaultOperationInvoker")
            .finish_non_exhaustive()
    }
}
impl<P: BindingInvoker, E: SchemaEvaluator> DefaultOperationInvoker<P, E> {
    /// Retain explicit dependencies. This does not prepare or judge any schema.
    pub fn new(provider: P, evaluator: E, external: ExternalSchemas) -> Self {
        Self {
            provider,
            evaluator,
            external,
            limits: InvocationLimits::default(),
            preparation: PreparationLimits::default(),
            detail: Detail::Outcome,
        }
    }
    /// Select finite queue and encoding bounds for later attempts.
    /// # Errors
    /// Rejects invalid finite handoff limits.
    pub fn with_limits(mut self, limits: InvocationLimits) -> Result<Self, LimitError> {
        self.limits = limits.validate()?;
        Ok(self)
    }
    /// Select explicit schema environment preparation bounds.
    #[must_use]
    pub fn with_preparation_limits(mut self, limits: PreparationLimits) -> Self {
        self.preparation = limits;
        self
    }
    /// Ask for local diagnostic detail without changing validation verdicts.
    #[must_use]
    pub fn with_validation_detail(mut self, detail: Detail) -> Self {
        self.detail = detail;
        self
    }
}
impl<P: BindingInvoker + InvocationSupport, E: SchemaEvaluator> DefaultOperationInvoker<P, E> {
    /// Resolve and select, then return a lazy output-driven attempt. Schemas are
    /// prepared only as values reach their direction; absent schemas bypass gating.
    /// # Errors
    /// Returns malformed-request admission separately from the attempt terminal.
    pub fn invoke<'a>(
        &'a self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<
        Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P, E>>,
        BoundaryFailure,
    > {
        request.admit()?;
        let selected = resolve(&self.provider, request);
        let schemas = selected
            .as_ref()
            .map(|(_, operation)| [operation.input(), operation.output()])
            .unwrap_or([None, None]);
        let shared = Shared::new(self.limits, schemas.each_ref().map(Option::is_some));
        let mut gates = schemas.map(|schema| {
            schema.map(|schema| Gate {
                schema,
                evaluator: &self.evaluator,
                external: &self.external,
                limits: self.preparation,
                detail: self.detail,
                prepared: None,
            })
        });
        let provider = &self.provider;
        let io = ProviderIo::new(shared.clone());
        let local = shared.control();
        let future = drive(
            move || {
                let (request, _) = selected?;
                Ok(provider.handle_invoke(request, io, local))
            },
            move |direction, value, control| {
                let index = match direction {
                    Direction::Input => 0,
                    Direction::Output => 1,
                };
                gates[index]
                    .as_mut()
                    .expect("only declared directions request a check")
                    .check(direction, value, control)
            },
            control,
            shared.clone(),
        );
        Ok(Invocation::from_parts(shared, future))
    }
    /// Resolve and select exactly as invocation, then run advisory work only.
    /// This future borrows P, never E: local evaluators do not constrain its Send.
    /// # Errors
    /// Returns malformed-request admission before creating advisory work.
    pub fn preflight<'a>(
        &'a self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>> + 'a + use<'a, P, E>,
        BoundaryFailure,
    > {
        request.admit()?;
        let selected = resolve(&self.provider, request).map(|(request, _)| request);
        Ok(binding::preflight(
            &self.provider,
            self.limits,
            selected,
            control,
        ))
    }
}
fn resolve<P: InvocationSupport>(
    provider: &P,
    request: OperationRequest,
) -> Result<(BindingRequest, Operation), InvocationError> {
    let OperationRequest {
        interface,
        address,
        context,
    } = request;
    let (operation, binding) = match address {
        // Explicit addressing bypasses support selection, including for providers
        // whose optional routing knowledge cannot describe this binding.
        OperationAddress::Binding(key) => {
            let binding = interface
                .binding(&key)
                .ok_or_else(InvocationError::binding_not_found)?;
            let operation = binding.operation();
            (operation, key)
        }
        OperationAddress::Operation { name, selection } => {
            let operation = interface
                .resolve_operation(&name)
                .ok_or_else(|| error("ERR_OPERATION_NOT_FOUND"))?;
            let keys = match selection {
                Some(keys) => {
                    // Admit the entire list before any support query or prefix choice.
                    for key in &keys {
                        if !interface
                            .binding(key)
                            .is_some_and(|binding| binding.operation_key() == operation.key())
                        {
                            return Err(InvocationError::binding_not_found());
                        }
                    }
                    let selected = keys
                        .into_iter()
                        .find(|key| supported(provider, &interface, key));
                    (selected, false)
                }
                None => {
                    let mut supported = operation
                        .bindings()
                        .into_iter()
                        .filter(|binding| provider.can_invoke(binding.source().kind()))
                        .map(|binding| binding.key().to_owned());
                    let first = supported.next();
                    (first, supported.next().is_some())
                }
            };
            if keys.1 {
                return Err(error("ERR_BINDING_SELECTION_REQUIRED"));
            }
            (
                operation,
                keys.0.ok_or_else(InvocationError::binding_not_found)?,
            )
        }
    };
    Ok((
        BindingRequest {
            interface,
            binding,
            context,
        },
        operation,
    ))
}
fn supported<P: InvocationSupport>(provider: &P, document: &ConformantDocument, key: &str) -> bool {
    let binding = document.binding(key).expect("candidate key established");
    let source = binding.source();
    provider.can_invoke(source.kind())
}
struct Gate<'a, E: SchemaEvaluator> {
    schema: Schema,
    evaluator: &'a E,
    external: &'a ExternalSchemas,
    limits: PreparationLimits,
    detail: Detail,
    prepared: Option<Result<PreparedSchema<E::Validator>, PreparationFailure>>,
}
impl<E: SchemaEvaluator> Gate<'_, E> {
    fn check(
        &mut self,
        direction: Direction,
        value: &JsonValue,
        control: &CancellationToken,
    ) -> CheckResult {
        let prepared = self.prepared.get_or_insert_with(|| {
            self.schema
                .prepare_with_control(self.evaluator, self.external, &self.limits, control)
        });
        match prepared {
            Ok(prepared) => evaluate(prepared, direction, value, self.detail, control),
            Err(failure) => reject(direction, ValidationEvidence::Preparation(failure.clone())),
        }
    }
}
fn evaluate<V: SchemaValidator>(
    prepared: &PreparedSchema<V>,
    direction: Direction,
    value: &JsonValue,
    detail: Detail,
    control: &CancellationToken,
) -> CheckResult {
    let outcome = if detail == Detail::Explain {
        prepared.explain_with_control(value, control)
    } else {
        prepared.evaluate_with_control(value, control)
    };
    if matches!(outcome, ValueOutcome::Satisfies) {
        Ok(())
    } else {
        reject(direction, ValidationEvidence::Evaluation(outcome))
    }
}
fn reject(direction: Direction, evidence: ValidationEvidence) -> CheckResult {
    let code = match &evidence {
        ValidationEvidence::Preparation(failure)
            if matches!(failure.kind, PreparationFailureKind::Cancelled) =>
        {
            "ERR_CANCELLED"
        }
        ValidationEvidence::Evaluation(ValueOutcome::NoVerdict { detail })
            if detail.reason == NoVerdictReason::Cancelled =>
        {
            "ERR_CANCELLED"
        }
        ValidationEvidence::Evaluation(ValueOutcome::Fails { .. }) => {
            "ERR_OPERATION_VALIDATION_FAILED"
        }
        _ => "ERR_SCHEMA_UNRESOLVED",
    };
    Err((
        error(code),
        Some(Arc::new(ValidationFailure {
            direction,
            evidence,
        })),
    ))
}
async fn drive<F: Future<Output = Result<(), InvocationError>>>(
    start: impl FnOnce() -> Result<F, InvocationError>,
    mut check: impl FnMut(Direction, &JsonValue, &CancellationToken) -> CheckResult,
    external: CancellationToken,
    shared: Arc<Shared>,
) -> Result<(), InvocationError> {
    let local = shared.control();
    let _relay = binding::invocation_relay(&external, &shared);
    if local.is_cancelled() {
        return Err(InvocationError::cancelled());
    }
    let mut cancelled = pin!(local.cancelled());
    let mut job = pin!(start()?);
    poll_fn(|cx| {
        loop {
            if let Some(outcome) = shared.outcome() {
                return Poll::Ready(outcome);
            }
            if cancelled.as_mut().poll(cx).is_ready() {
                return Poll::Ready(Err(InvocationError::cancelled()));
            }
            let result = job.as_mut().poll(cx);
            if let Some(outcome) = shared.outcome() {
                return Poll::Ready(outcome);
            }
            if local.is_cancelled() {
                return Poll::Ready(Err(InvocationError::cancelled()));
            }
            if let Poll::Ready(result) = result {
                return Poll::Ready(binding::provider_result(result, &shared));
            }
            let mut progress = false;
            for (channel, direction) in [
                (channel::Direction::Input, Direction::Input),
                (channel::Direction::Output, Direction::Output),
            ] {
                if let Some(pending) = shared.pending_check(channel) {
                    progress = true;
                    let result = check(direction, &pending.value, &local);
                    // Observed cancellation is global, independent of the slot's live
                    // identity, even if the evaluator or another handle withdrew it.
                    if local.is_cancelled()
                        || result
                            .as_ref()
                            .err()
                            .is_some_and(|(error, _)| error.code() == "ERR_CANCELLED")
                    {
                        shared.settle(Err(InvocationError::cancelled()));
                    } else {
                        shared.checked(pending, result);
                    }
                    if let Some(outcome) = shared.outcome() {
                        return Poll::Ready(outcome);
                    }
                }
            }
            if !progress {
                return Poll::Pending;
            }
        }
    })
    .await
}

/// Invoke an operation with resolution, selection and schema validation.
/// Implementers return the full lazy invocation and own these interface semantics.
/// [`Invocation::new`] supplies lifecycle/framing for third-party implementations;
/// [`DefaultOperationInvoker`] supplies the SDK implementation over a binding invoker.
/// Futures may borrow local state and need not be Send. Concrete implementations
/// can preserve conditional Send; this trait promises no blanket Send bound.
pub trait OperationInvoker {
    /// Admit an operation request and construct a lazy invocation.
    /// # Errors
    /// Returns local admission failure separately from terminal invocation errors.
    fn invoke(
        &self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>;
    /// Resolve/select and perform advisory preflight without invoking the operation.
    /// # Errors
    /// Returns local admission failure before advisory work.
    fn preflight(
        &self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    >;
}
impl<P: BindingInvoker + InvocationSupport, E: SchemaEvaluator> OperationInvoker
    for DefaultOperationInvoker<P, E>
{
    fn invoke(
        &self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<Invocation<impl Future<Output = Result<(), InvocationError>>>, BoundaryFailure>
    {
        DefaultOperationInvoker::invoke(self, request, control)
    }
    fn preflight(
        &self,
        request: OperationRequest,
        control: CancellationToken,
    ) -> Result<
        impl Future<Output = Result<Option<ContextChallenge>, InvocationError>>,
        BoundaryFailure,
    > {
        DefaultOperationInvoker::preflight(self, request, control)
    }
}
macro_rules! forward_operation { ($($wrapper:ty),+) => { $(
    impl<P: OperationInvoker + ?Sized> OperationInvoker for $wrapper {
        fn invoke(&self, request: OperationRequest, control: CancellationToken)
            -> Result<Invocation<impl Future<Output=Result<(),InvocationError>>>, BoundaryFailure> {
            (**self).invoke(request,control)
        }
        fn preflight(&self, request: OperationRequest, control: CancellationToken)
            -> Result<impl Future<Output=Result<Option<ContextChallenge>, InvocationError>>, BoundaryFailure> {
            (**self).preflight(request,control)
        }
    }
)+ }; }
forward_operation!(&P, Box<P>, Arc<P>, std::rc::Rc<P>);

//! Explicit owned validator reuse without retaining an evaluator in the driver.
use super::*;
use crate::Context;

/// The binding choice still available after fixing an operation.
#[derive(Clone)]
pub enum BindingChoice {
    /// Require exactly one invocable candidate.
    Sole,
    /// Use the first invocable listed key, checking the whole list first.
    Ordered(Vec<String>),
    /// Use this binding of the prepared operation without support selection.
    Binding(String),
}
impl fmt::Debug for BindingChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sole => "Sole",
            Self::Ordered(_) => "Ordered { .. }",
            Self::Binding(_) => "Binding { .. }",
        })
    }
}
/// Preparation refused before an invocation exists; no per-value verdict is implied.
#[derive(Debug)]
pub enum OperationPreparationFailure {
    /// The named operation could not be resolved.
    Resolution(InvocationError),
    /// A declared direction could not be prepared, with original typed evidence.
    Schema(ValidationFailure),
    /// Cancellation outside a direction's schema preparation, including no schemas.
    Cancelled,
}
impl fmt::Display for OperationPreparationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Resolution(_) => "operation preparation name resolution failed",
            Self::Schema(_) => "operation schema preparation failed",
            Self::Cancelled => "operation preparation cancelled",
        })
    }
}
impl std::error::Error for OperationPreparationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::Schema(error) => match error.evidence() {
                ValidationEvidence::Preparation(error) => Some(error),
                ValidationEvidence::Evaluation(_) => None,
            },
            Self::Cancelled => None,
        }
    }
}

/// One operation's original snapshot and owned prepared validators. Consuming
/// this owner into an attempt requires no Sync on V. Clone is available only when
/// V is Clone; callers choose whether cloning copies or shares validator state.
pub struct PreparedOperation<V: SchemaValidator> {
    interface: ConformantDocument,
    operation: String,
    schemas: [Option<PreparedSchema<V>>; 2],
    limits: InvocationLimits,
    detail: Detail,
}
impl<V: SchemaValidator> fmt::Debug for PreparedOperation<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedOperation").finish_non_exhaustive()
    }
}
impl<V: SchemaValidator + Clone> Clone for PreparedOperation<V> {
    fn clone(&self) -> Self {
        Self {
            interface: self.interface.clone(),
            operation: self.operation.clone(),
            schemas: self.schemas.clone(),
            limits: self.limits,
            detail: self.detail,
        }
    }
}
impl<P: BindingInvoker, E: SchemaEvaluator> DefaultOperationInvoker<P, E> {
    /// Resolve one operation key or alias and prepare its declared directions.
    /// The returned owner retains no provider, evaluator or borrowed configuration.
    /// No binding is selected and no provider work starts.
    /// # Errors
    /// Distinguishes operation resolution, schema preparation and cancellation
    /// outside a schema. Any name, the empty string included, is resolved. A failed attempt does not poison later ones.
    pub fn prepare(
        &self,
        interface: ConformantDocument,
        name: &str,
        control: &CancellationToken,
    ) -> Result<PreparedOperation<E::Validator>, OperationPreparationFailure> {
        if control.is_cancelled() {
            return Err(OperationPreparationFailure::Cancelled);
        }
        let operation = interface.resolve_operation(name).ok_or_else(|| {
            OperationPreparationFailure::Resolution(error("ERR_OPERATION_NOT_FOUND"))
        })?;
        let schemas = [operation.input(), operation.output()];
        let mut prepared = [None, None];
        for (index, direction) in [Direction::Input, Direction::Output]
            .into_iter()
            .enumerate()
        {
            if let Some(schema) = &schemas[index] {
                prepared[index] = Some(
                    schema
                        .prepare_with_control(
                            &self.evaluator,
                            &self.external,
                            &self.preparation,
                            control,
                        )
                        .map_err(|failure| {
                            OperationPreparationFailure::Schema(ValidationFailure {
                                direction,
                                evidence: ValidationEvidence::Preparation(failure),
                            })
                        })?,
                );
            }
        }
        if control.is_cancelled() {
            return Err(OperationPreparationFailure::Cancelled);
        }
        Ok(PreparedOperation {
            interface,
            operation: operation.key().to_owned(),
            schemas: prepared,
            limits: self.limits,
            detail: self.detail,
        })
    }
}
impl<V: SchemaValidator> PreparedOperation<V> {
    /// Original document supplying binding meaning and schema reference context.
    pub fn interface(&self) -> &ConformantDocument {
        &self.interface
    }
    /// Canonical operation key resolved when this owner was prepared.
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// Consume this owner into one output-driven invocation. Selection is performed
    /// now from the retained snapshot. A Send V, Sync P and Send P::Invoke yield a
    /// Send driver even when V is not Sync and the original E is neither Send nor Sync.
    /// # Errors
    /// Rejects malformed local binding choice; selection failures become terminals.
    pub fn invoke<'a, P: BindingInvoker + InvocationSupport>(
        self,
        provider: &'a P,
        choice: BindingChoice,
        context: Option<Context>,
        control: CancellationToken,
    ) -> Result<
        Invocation<impl Future<Output = Result<(), InvocationError>> + 'a + use<'a, P, V>>,
        BoundaryFailure,
    >
    where
        V: 'a,
    {
        let address = match choice {
            BindingChoice::Sole => OperationAddress::Operation {
                name: self.operation.clone(),
                selection: None,
            },
            BindingChoice::Ordered(keys) => OperationAddress::Operation {
                name: self.operation.clone(),
                selection: Some(keys),
            },
            BindingChoice::Binding(key) => OperationAddress::Binding(key),
        };
        let request = OperationRequest {
            interface: self.interface,
            address,
            context,
        };
        request.admit()?;
        let selected = resolve(provider, request).and_then(|(request, operation)| {
            if operation.key() == self.operation {
                Ok(request)
            } else {
                Err(InvocationError::binding_not_found())
            }
        });
        let schemas = self.schemas;
        let detail = self.detail;
        let shared = Shared::new(self.limits, schemas.each_ref().map(Option::is_some));
        let io = ProviderIo::new(shared.clone());
        let local = shared.control();
        let future = drive(
            move || Ok(provider.handle_invoke(selected?, io, local)),
            move |direction, value, control| {
                let index = match direction {
                    Direction::Input => 0,
                    Direction::Output => 1,
                };
                evaluate(
                    schemas[index].as_ref().expect("declared direction"),
                    direction,
                    value,
                    detail,
                    control,
                )
            },
            control,
            shared.clone(),
        );
        Ok(Invocation::from_parts(shared, future))
    }
}

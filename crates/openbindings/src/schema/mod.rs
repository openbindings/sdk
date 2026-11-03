//! Contextual schemas retained from conformant document snapshots.
pub mod evaluation;
pub mod inventory;
pub mod references;
pub mod resources;
use crate::*;
pub use evaluation::*;
pub use inventory::{ResourceBase, ResourceIdentity, SchemaEnvironment, SchemaNode};
pub use references::{
    Reference, ReferenceRefusal, ReferenceRefusalReason, ReferenceReport, ReferenceResolution,
};
pub use references::{ReferenceDefect, ReferenceTarget, ResolutionFailure, ResolutionFailureKind};
pub use resources::*;
use std::fmt;

/// A contained schema and the conformant document that gives it meaning.
/// Cloning retains the same immutable snapshot. A schema remains usable after
/// its operation, document handle, or assessment has been dropped.
///
/// ```compile_fail,E0451
/// use openbindings::*;
/// fn forge(document: ConformantDocument, value: JsonValue) {
///     let _ = Schema { document, value };
/// }
/// ```
#[derive(Clone)]
pub struct Schema {
    document: ConformantDocument,
    value: JsonValue,
}
#[derive(Clone, Copy)]
pub(crate) enum Side {
    Input,
    Output,
}
impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema").finish_non_exhaustive()
    }
}
impl Schema {
    pub(crate) fn contained(document: &ConformantDocument, value: JsonValue) -> Self {
        Self {
            document: document.clone(),
            value,
        }
    }
    pub(crate) fn declared(operation: &Operation, side: Side) -> Option<Self> {
        operation
            .field(side)
            .map(|value| Self::contained(operation.document(), value.to_owned()))
    }
    /// Retain the original exact schema value. Absence is represented by the
    /// document slot's `None`, never by a fabricated true schema.
    pub fn json(&self) -> JsonValue {
        self.value.clone()
    }
    /// Borrow the document snapshot that establishes this schema's context.
    pub fn document(&self) -> &ConformantDocument {
        &self.document
    }
    /// Locate this schema in its original document, without a generated URI.
    pub fn location(&self) -> SchemaOrigin {
        SchemaOrigin::new(
            None,
            openbindings_internal_json::backend::exact_pointer(self.value.view()),
        )
    }
    /// Prepare with a concrete evaluator and explicit immutable external inputs.
    /// A ready validator may still return any of the four value outcomes.
    ///
    /// # Errors
    /// Preparation failure judges no value and does not establish undefinedness.
    pub fn prepare<E: SchemaEvaluator + ?Sized>(
        &self,
        evaluator: &E,
        external: &ExternalSchemas,
        limits: &PreparationLimits,
    ) -> Result<PreparedSchema<E::Validator>, PreparationFailure> {
        self.prepare_with_control(evaluator, external, limits, &crate::control::NeverCancelled)
    }
    /// Prepare with cooperative cancellation. A validator is not returned after
    /// cancellation, including cancellation observed as the evaluator finishes.
    /// Failed attempts do not poison immutable inputs or later retries.
    ///
    /// # Errors
    /// Returns preparation failure, separately from every per-value outcome.
    pub fn prepare_with_control<E: SchemaEvaluator + ?Sized>(
        &self,
        evaluator: &E,
        external: &ExternalSchemas,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<PreparedSchema<E::Validator>, PreparationFailure> {
        control.check()?;
        let result = evaluator.prepare(
            EvaluationRequest {
                schema: self,
                external_schemas: external,
            },
            limits,
            control,
        );
        control.check()?;
        result.map(|validator| PreparedSchema { validator })
    }
}

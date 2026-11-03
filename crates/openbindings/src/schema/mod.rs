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
    pub(crate) fn declared(operation: &ConformantOperationView, side: Side) -> Option<Self> {
        operation
            .field(side)
            .map(|value| Self::contained(operation.proof(), value.to_owned()))
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
        self.prepare_with(evaluator, ExternalSchemas::empty())
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
        resources: &ExternalSchemas,
    ) -> Result<PreparedValueContract, NoVerdict> {
        self.prepare_with_control(evaluator, resources, &crate::control::NeverCancelled)
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
        resources: &ExternalSchemas,
        control: &dyn WorkControl,
    ) -> Result<PreparedValueContract, NoVerdict> {
        control.check()?;
        let schema = self.value.view();
        let entry = self.value.clone();
        let document = &self.document;
        if !document
            .document()
            .schema_index()
            .position(openbindings_internal_json::backend::node_id(&entry))
            .is_some()
        {
            return Err(NoVerdict::new(
                NoVerdictReason::ConservativePreparation,
                "non-schema-entry",
                "the entry is not a schema position",
            )
            .with_location(SchemaOrigin {
                source_uri: None,
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

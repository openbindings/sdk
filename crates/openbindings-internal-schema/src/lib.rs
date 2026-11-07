#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
pub use openbindings_internal_json::{JsonKind, JsonRef, JsonValue};
pub mod dialect;
pub mod fixed_locations;
pub mod fixed_schema;
mod identifiers;
mod identity_budget;
pub mod preparation;
pub use identity_budget::DEFAULT_IDENTITY_BYTES;
mod input_policy;
pub mod schema_index;
mod space;
pub mod uri;
pub use input_policy::{IndexedDocument, InputPolicy, Supplied, SuppliedSet};
mod program;
pub use program::*;
use serde::Serialize;
pub use space::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    pub resource: Option<String>,
    /// Exact, from the resource's (or the document's) root.
    pub pointer: openbindings_internal_json::Pointer,
}
impl std::fmt::Debug for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Location").finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct ProjectedResource {
    pub uri: String,
    pub document: JsonValue,
}
#[derive(Clone)]
pub struct RawDocument {
    value: JsonValue,
    index: Arc<schema_index::SchemaIndex>,
}
impl std::fmt::Debug for RawDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawDocument").finish_non_exhaustive()
    }
}

impl RawDocument {
    pub fn new(value: JsonValue) -> Self {
        let index = Arc::new(schema_index::SchemaIndex::build(&value));
        Self { value, index }
    }
    /// The document a document index ([`schema_index::SchemaIndex::build`])
    /// was built for, with that index, shared rather than built again.
    pub fn shared(index: Arc<schema_index::SchemaIndex>) -> Self {
        let value = index.source().expect("a document index").clone();
        Self { value, index }
    }
    /// Whether every indexed schema is known to satisfy the 2020-12
    /// meta-schemas: each was checked against them through its index
    /// ([`schema_index::MetaChecks`]), as validating a document does, and
    /// none failed.
    pub fn schemas_checked(&self) -> bool {
        self.index.schemas_checked()
    }
    pub fn parse(text: impl AsRef<str>) -> Result<Self, openbindings_internal_json::InputError> {
        Ok(Self::new(JsonValue::parse(text.as_ref())?))
    }
    pub fn value(&self) -> &JsonValue {
        &self.value
    }
    pub fn schema_index(&self) -> &schema_index::SchemaIndex {
        &self.index
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureReason {
    UnsupportedCapability,
    ConservativePreparation,
    ResourceUnavailable,
    LimitExceeded,
    Cancelled,
    EvaluatorFailure,
}

#[derive(Clone, Serialize)]
pub struct Failure {
    pub reason: FailureReason,
    pub code: std::borrow::Cow<'static, str>,
    pub message: std::borrow::Cow<'static, str>,
    pub location: Option<Location>,
    /// Established static-reference semantics, independent of evaluator ability.
    #[serde(skip)]
    pub reference_defect: Option<StaticReferenceDefect>,
    /// Original-context resolution cause, retained without diagnostic parsing.
    #[serde(skip)]
    pub query_failure: Option<Box<QueryFailure>>,
}
impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("reason", &self.reason)
            .field("location", &self.location)
            .field("reference_defect", &self.reference_defect)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticReferenceDefect {
    PointerToNoSchema,
    MissingPlainName,
    DuplicatePlainName,
    MalformedReference,
}
impl Failure {
    pub fn new(
        reason: FailureReason,
        code: impl Into<std::borrow::Cow<'static, str>>,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self {
            reason,
            code: code.into(),
            message: message.into(),
            location: None,
            reference_defect: None,
            query_failure: None,
        }
    }
    #[must_use]
    pub fn located(mut self, at: Location) -> Self {
        self.location = Some(at);
        self
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
pub trait Control: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn check(&self) -> Result<(), Failure> {
        if self.is_cancelled() {
            Err(Failure::new(
                FailureReason::Cancelled,
                "caller-cancelled",
                "the caller cancelled this operation",
            ))
        } else {
            Ok(())
        }
    }
    /// Charge a cooperative SDK work site. Ordinary controls only observe
    /// cancellation; a preparation attempt owns the shared SDK allowance.
    /// Dependency and custom callback execution is not automatically counted.
    fn charge_work(&self, _units: usize) -> Result<(), Failure> {
        self.check()
    }
    /// Reserve a conservative estimate at an explicit SDK storage site before
    /// allocating. This is not allocator-wide accounting. Footprint limits remain
    /// separate; reservations are never refunds of retained data.
    fn charge_allocation(&self, _bytes: usize) -> Result<(), Failure> {
        self.check()
    }
    /// Keep an exhausted nested allocation allowance sticky in every enclosing
    /// attempt. Ordinary cancellation controls have no allocation state.
    fn allocation_limit_reached(&self) {}
    /// Observe an enclosing exhausted allocation allowance without performing
    /// another work charge. This prevents a child continuing after its parent
    /// refused a reservation.
    fn allocation_is_exhausted(&self) -> bool {
        false
    }
}
impl<F: Fn() -> bool + Send + Sync> Control for F {
    fn is_cancelled(&self) -> bool {
        self()
    }
}
impl Control for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for ProjectedResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectedResource").finish_non_exhaustive()
    }
}

/// Private SDK bridge used by evaluator adapters to reuse an original inventory.
/// This internal package is version-locked to the SDK, not an extension API.
#[doc(hidden)]
pub trait OriginalEnvironment {
    fn space(&self) -> &SchemaSpace;
    fn entry(&self) -> usize;
}

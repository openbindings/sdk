//! Private, shared OpenBindings embedding implementation. No evaluator planning.
pub use openbindings_internal_json::{JsonKind, JsonRef, JsonValue};
pub mod fixed_schema;
mod input_policy;
pub mod schema_index;
mod space;
pub mod uri;
pub use input_policy::{InputPolicy, Supplied};
mod program;
pub use program::*;
use serde::Serialize;
pub use space::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub resource: Option<String>,
    pub pointer: String,
}
#[derive(Clone, Debug)]
pub struct ProjectedResource {
    pub uri: String,
    pub document: JsonValue,
}
#[derive(Clone)]
pub struct RawDocument {
    value: JsonValue,
    index: Arc<schema_index::SchemaIndex>,
}
impl RawDocument {
    pub fn new(value: JsonValue) -> Self {
        let index = Arc::new(schema_index::SchemaIndex::build(&value));
        Self { value, index }
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
#[derive(Clone, Debug, Serialize)]
pub struct Failure {
    pub reason: FailureReason,
    pub code: String,
    pub message: String,
    pub location: Option<Location>,
}
impl Failure {
    pub fn new(reason: FailureReason, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            reason,
            code: code.into(),
            message: message.into(),
            location: None,
        }
    }
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

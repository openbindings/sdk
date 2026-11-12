//! Exact context carriage shared by invocation, pulling and kind checking.
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
use openbindings::{JsonAccessError, JsonObject, JsonStringRef, JsonValue};
use std::fmt;

/// Version of the adopted context document, independent of the SDK version.
pub const CONTEXT_VERSION: &str = "0.1";
/// Exact interfaces repository revision whose context document is applied.
pub const APPLIED_INTERFACE_REVISION: &str = "484c9399207de99bd56921cead5f44a7b5740d9e";

/// One opaque context object, supplied by value for an attempt.
///
/// Its members have no SDK interpretation. Cloning retains the exact immutable
/// value, including nulls, large numbers, unknown members and duplicate names.
#[derive(Clone)]
pub struct Context(JsonObject);
impl Context {
    /// Retain an object without interpreting its members or combining sources.
    pub fn new(value: JsonObject) -> Self {
        Self(value)
    }
    /// Borrow the exact context object. Presence remains distinct from omission.
    pub fn value(&self) -> &JsonObject {
        &self.0
    }
    /// Consume the context wrapper while retaining the exact object.
    pub fn into_value(self) -> JsonObject {
        self.0
    }
}
impl TryFrom<JsonValue> for Context {
    type Error = JsonAccessError;
    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        JsonObject::try_from(value).map(Self)
    }
}
impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

/// Structural admission failure for a context challenge.
///
/// Every variant describes a fixed carrier field. Formatting never exposes
/// caller-authored names, a producer's message, context, or schema contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChallengeShapeError {
    /// The challenge is not an object.
    ExpectedObject,
    /// The required message is absent.
    MissingMessage,
    /// The message is not a JSON string.
    ExpectedMessageString,
    /// The message has no decoded characters.
    EmptyMessage,
    /// The required schema is absent.
    MissingSchema,
    /// The schema is not in object form.
    ExpectedSchemaObject,
    /// An outer member occurs more than once under its decoded name.
    DuplicateMember,
    /// The outer object contains a member other than message or schema.
    UnexpectedMember,
}
impl fmt::Display for ChallengeShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExpectedObject => "a context challenge must be an object",
            Self::MissingMessage => "the challenge requires a message",
            Self::ExpectedMessageString => "the challenge message must be a string",
            Self::EmptyMessage => "the challenge message must not be empty",
            Self::MissingSchema => "the challenge requires a schema",
            Self::ExpectedSchemaObject => "the challenge schema must be an object",
            Self::DuplicateMember => "a challenge member is repeated",
            Self::UnexpectedMember => "the challenge contains an unexpected member",
        })
    }
}
impl std::error::Error for ChallengeShapeError {}

/// A producer's message and object-form schema of the context it requests.
///
/// This proves the closed carrier shape only. It does not prove JSON Schema
/// validity, evaluate that schema, trust the producer, acquire values or grant
/// permission to disclose anything. Schema contents remain exact and opaque.
#[derive(Clone)]
pub struct ContextChallenge(JsonObject);
impl ContextChallenge {
    /// Check the closed `{message, schema}` carrier and retain its original JSON.
    ///
    /// # Errors
    /// Returns a [`ChallengeShapeError`] for missing, repeated, unknown or
    /// incorrectly shaped outer fields, including an empty message.
    pub fn from_json(value: JsonObject) -> Result<Self, ChallengeShapeError> {
        let mut message = false;
        let mut schema = false;
        for member in value.value().view().members().expect("object view") {
            let present = if member.name.as_str() == Some("message") {
                &mut message
            } else if member.name.as_str() == Some("schema") {
                &mut schema
            } else {
                return Err(ChallengeShapeError::UnexpectedMember);
            };
            if std::mem::replace(present, true) {
                return Err(ChallengeShapeError::DuplicateMember);
            }
        }
        let message = value
            .value()
            .get("message")
            .ok_or(ChallengeShapeError::MissingMessage)?
            .as_string()
            .ok_or(ChallengeShapeError::ExpectedMessageString)?;
        if message.utf16_units().next().is_none() {
            return Err(ChallengeShapeError::EmptyMessage);
        }
        value
            .value()
            .get("schema")
            .ok_or(ChallengeShapeError::MissingSchema)?
            .to_owned()
            .as_object()
            .map_err(|_| ChallengeShapeError::ExpectedSchemaObject)?;
        Ok(Self(value))
    }
    /// Borrow the producer's exact message, including any unpaired UTF-16 units.
    /// Use [`JsonStringRef::as_str`] when Unicode scalar text is needed.
    pub fn message(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("message")
            .and_then(|value| value.as_string())
            .expect("admitted message")
    }
    /// Retain the original object-form schema and all its annotations.
    pub fn schema(&self) -> JsonObject {
        self.0
            .value()
            .get("schema")
            .expect("admitted schema")
            .to_owned()
            .as_object()
            .expect("admitted schema object")
    }
    /// Borrow the complete original challenge for exact forwarding.
    pub fn value(&self) -> &JsonObject {
        &self.0
    }
}
impl TryFrom<JsonValue> for ContextChallenge {
    type Error = ChallengeShapeError;
    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        Self::from_json(
            JsonObject::try_from(value).map_err(|_| ChallengeShapeError::ExpectedObject)?,
        )
    }
}
impl fmt::Debug for ContextChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextChallenge").finish_non_exhaustive()
    }
}

use crate::{CarrierError, ContextChallenge, model};
use openbindings::{JsonObject, JsonStringRef, JsonValue};
use std::fmt;

/// A check-specific rejection, preserving its code, readable message and optional data.
/// The closed object constructor is an SDK representation policy, not a separately
/// published interface wire schema. This is not an invocation error record.
#[derive(Clone)]
pub struct CheckRejection(JsonObject);
impl CheckRejection {
    /// Admit `{code, message, data?}` with nonempty exact strings. The reserved
    /// CONTEXT_REQUIRED code belongs to [`CheckFailure::ContextRequired`].
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = model::object(value, Some((&["code", "message", "data"], false)))?;
        let code = model::string(model::field(&object, "code")?, "code", true)?;
        if code == "CONTEXT_REQUIRED" {
            return Err(CarrierError::ReservedCode);
        }
        model::string(model::field(&object, "message")?, "message", true)?;
        Ok(Self(object))
    }
    /// Borrow the kind's exact rejection code.
    pub fn code(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("code")
            .and_then(|v| v.as_string())
            .expect("admitted code")
    }
    /// Borrow the exact message intended for the person checking the entry.
    pub fn message(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("message")
            .and_then(|v| v.as_string())
            .expect("admitted message")
    }
    /// Retain optional data, distinguishing absence from JSON null.
    pub fn data(&self) -> Option<JsonValue> {
        self.0.value().get("data").map(|value| value.to_owned())
    }
    /// Retain this SDK record's exact original object.
    pub fn to_json(&self) -> JsonObject {
        self.0.clone()
    }
}
impl fmt::Debug for CheckRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckRejection").finish_non_exhaustive()
    }
}

/// An unsuccessful source check, distinct from local caller admission or limits.
#[derive(Clone)]
#[non_exhaustive]
pub enum CheckFailure<E = std::convert::Infallible> {
    /// The kind requires caller-provided context; the caller never acquires or retries it.
    ContextRequired(ContextChallenge),
    /// The kind rejected the entry with a code and a human-readable message.
    Rejected(CheckRejection),
    /// The provider cooperatively stopped a cancelled call; not a portable rejection code.
    Cancelled,
    /// Native acquisition, decoding or carrier-admission failure. This makes no
    /// claim that the kind rejected the proposed entry.
    Operational(E),
}
impl<E> fmt::Debug for CheckFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ContextRequired(_) => "CheckFailure::ContextRequired { .. }",
            Self::Rejected(_) => "CheckFailure::Rejected { .. }",
            Self::Cancelled => "CheckFailure::Cancelled",
            Self::Operational(_) => "CheckFailure::Operational { .. }",
        })
    }
}
impl<E> fmt::Display for CheckFailure<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ContextRequired(_) => "source checking requires context",
            Self::Rejected(_) => "the kind rejected the proposed source",
            Self::Cancelled => "source checking was cancelled",
            Self::Operational(_) => "source checking could not complete",
        })
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CheckFailure<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Operational(error) => Some(error),
            _ => None,
        }
    }
}

/// Local caller failures are distinct from the provider's own failure type.
#[non_exhaustive]
pub enum KindError<E> {
    /// The provider's unsuccessful result, without converting it to a verdict.
    Provider(E),
    /// The caller cancelled the attempt.
    Cancelled,
    /// The response names a different kind from the request.
    KindMismatch,
    /// A configured carrier handoff bound was reached.
    LimitExceeded(&'static str),
}
impl<E> fmt::Debug for KindError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(_) => f.write_str("KindError::Provider { .. }"),
            Self::Cancelled => f.write_str("KindError::Cancelled"),
            Self::KindMismatch => f.write_str("KindError::KindMismatch"),
            Self::LimitExceeded(resource) => f
                .debug_tuple("KindError::LimitExceeded")
                .field(resource)
                .finish(),
        }
    }
}
impl<E> fmt::Display for KindError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Provider(_) => "the provider did not complete the request successfully",
            Self::Cancelled => "the caller cancelled the request",
            Self::KindMismatch => "the response names a different kind",
            Self::LimitExceeded(_) => "the carrier exceeds the configured handoff limit",
        })
    }
}
impl<E: std::error::Error + 'static> std::error::Error for KindError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            _ => None,
        }
    }
}

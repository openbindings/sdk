//! Portable failures and value-free local boundary diagnostics.

use crate::value::{ContextChallenge, encode, object, string};
use openbindings::{JsonLimits, JsonRef, JsonValue};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::fmt;

/// A local structural diagnostic, never a portable invocation failure.
///
/// Codes and paths come from a fixed SDK vocabulary and contain no source
/// values or caller-authored member names. A `*` path segment denotes any array
/// element. This record carries neither native causes nor protocol evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryError {
    code: &'static str,
    path: &'static str,
}

impl BoundaryError {
    pub(crate) const fn new(code: &'static str, path: &'static str) -> Self {
        Self { code, path }
    }

    /// Stable structural diagnostic code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Safe structural location; empty denotes the root.
    pub fn path(&self) -> &'static str {
        self.path
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.code, self.path)
    }
}

impl std::error::Error for BoundaryError {}

/// An immutable portable unsuccessful-completion record: code and optional data.
///
/// Unknown nonempty codes remain unchanged. Absent data and present JSON null
/// are distinct. Data must be governed portable application data, not raw native
/// errors, credentials, protocol envelopes, stacks, or debugging evidence.
/// Construction checks `CONTEXT_REQUIRED` structurally; the provider remains
/// responsible for its before-output and before-effects guarantees.
#[derive(Clone)]
pub struct InvocationError {
    code: String,
    data: Option<JsonValue>,
}

impl InvocationError {
    /// Check a nonempty code and, for `CONTEXT_REQUIRED`, its challenge data.
    ///
    /// This preserves the exact data snapshot. Its final delivery size is
    /// separately checked by the invocation's resource policy.
    pub fn new(code: impl Into<String>, data: Option<JsonValue>) -> Result<Self, BoundaryError> {
        let code = code.into();
        if code.is_empty() {
            return Err(BoundaryError::new("empty-string", "/code"));
        }
        if code == "CONTEXT_REQUIRED" {
            let value = data
                .as_ref()
                .ok_or_else(|| BoundaryError::new("missing-member", "/data"))?;
            ContextChallenge::validate(value.view())?;
        }
        Ok(Self { code, data })
    }

    /// Read a closed portable error object without collapsing duplicate fields.
    pub fn from_json(value: &JsonValue) -> Result<Self, BoundaryError> {
        Self::from_json_ref(value.view())
    }

    /// Read an exact borrowed portable error object, retaining its data owner.
    pub fn from_json_ref(value: JsonRef<'_>) -> Result<Self, BoundaryError> {
        object(value, &["code", "data"], false, "")?;
        let code = value
            .get("code")
            .ok_or_else(|| BoundaryError::new("missing-member", "/code"))?;
        Self::new(
            string(code, "/code")?,
            value.get("data").map(JsonRef::to_owned),
        )
    }

    /// Exact governed or implementation-defined code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Borrow associated exact application data, preserving absence versus null.
    pub fn data(&self) -> Option<&JsonValue> {
        self.data.as_ref()
    }

    /// Retain a validated context challenge when this is `CONTEXT_REQUIRED`.
    ///
    /// This cheap clone retains the backing arena. It is neither a trust verdict
    /// on the asserted target nor permission to resolve or release credentials.
    pub fn context_required(&self) -> Option<ContextChallenge> {
        (self.code == "CONTEXT_REQUIRED").then(|| {
            ContextChallenge::checked(self.data.as_ref().expect("checked challenge data").clone())
        })
    }

    /// Encode the portable shape under the default finite exact-JSON limits.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Encode under explicit byte, nesting and node limits, preserving exact data.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryError> {
        encode(&ErrorFields(self), limits)
    }

    /// Refuse before any observable effect of the requested operation.
    ///
    /// The provider may use this only when it can uphold that guarantee.
    pub fn refused() -> Self {
        Self::without_data("ERR_REFUSED")
    }

    /// Record caller cancellation without claiming remote rollback or no effects.
    pub fn cancelled() -> Self {
        Self::without_data("ERR_CANCELLED")
    }

    /// Complete unsuccessfully without making a dispatch-state or cause claim.
    pub fn execution_failed() -> Self {
        Self::without_data("ERR_EXECUTION_FAILED")
    }

    /// Report outer transport closure before an inner terminal was observed.
    pub fn transport_closed() -> Self {
        Self::without_data("ERR_TRANSPORT_CLOSED")
    }

    /// Report a malformed or temporally invalid invocation frame sequence.
    pub fn frame_protocol() -> Self {
        Self::without_data("ERR_FRAME_PROTOCOL")
    }

    /// Request context before output and observable operation effects.
    ///
    /// Validation cannot establish the provider's before-effects guarantee.
    pub fn challenge(challenge: ContextChallenge) -> Self {
        Self {
            code: "CONTEXT_REQUIRED".into(),
            data: Some(challenge.into_value()),
        }
    }

    fn without_data(code: &str) -> Self {
        Self {
            code: code.into(),
            data: None,
        }
    }
}

impl fmt::Debug for InvocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvocationError")
            .field("code", &"<redacted>")
            .field("data_present", &self.data.is_some())
            .finish()
    }
}

impl fmt::Display for InvocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invocation completed unsuccessfully")
    }
}

impl std::error::Error for InvocationError {}

impl Serialize for InvocationError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

struct ErrorFields<'a>(&'a InvocationError);

impl Serialize for ErrorFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("code", &self.0.code)?;
        if let Some(data) = &self.0.data {
            map.serialize_entry("data", data)?;
        }
        map.end()
    }
}

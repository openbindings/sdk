//! Local invocation admission and bounded encoding, separate from wire failures.
use openbindings::{JsonKind, JsonLimits, JsonRef, JsonValue};
use serde::Serialize;
use std::{collections::BTreeSet, fmt};

/// A local request, provider-result or resource-policy failure.
/// This record never becomes portable invocation error data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryFailure {
    kind: BoundaryFailureKind,
    path: &'static str,
}
impl BoundaryFailure {
    /// The structural or local policy cause.
    pub fn kind(&self) -> BoundaryFailureKind {
        self.kind
    }
    /// A fixed SDK path; no authored member name is copied into it.
    pub fn path(&self) -> &'static str {
        self.path
    }
}
impl fmt::Display for BoundaryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at {}", self.kind, self.path)
    }
}
impl std::error::Error for BoundaryFailure {}

/// Reasons local invocation admission cannot proceed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BoundaryFailureKind {
    /// Caller-selected invocation limits cannot form a valid handoff.
    InvalidLimits,
    /// An interpreted object contains an unknown member.
    UnexpectedMember,
    /// A required field is missing.
    MissingMember,
    /// An interpreted member's decoded name is repeated, or an object anywhere
    /// in `/context` repeats a decoded member name.
    DuplicateMember,
    /// A field requires an object.
    ExpectedObject,
    /// A field requires an array.
    ExpectedArray,
    /// A field requires a string.
    ExpectedString,
    /// A routing name or code cannot be represented as Unicode scalar text.
    UnpairedString,
    /// A required nonempty string is empty.
    EmptyString,
    /// An ordered selection is empty.
    EmptyArray,
    /// Operation and binding address fields do not form one valid choice.
    InvalidAddress,
    /// The supplied interface does not have a current conformance proof.
    InvalidDocument,
    /// Context challenge data does not have its shared closed shape.
    InvalidChallenge,
    /// An encoded value exceeds the selected exact-JSON bounds.
    EncodingLimit,
    /// A binding provider used an operation-layer reserved failure code.
    ReservedProviderCode,
    /// A terminal exceeds its finite byte allowance.
    TerminalResourceLimit,
    /// An output exceeds its finite byte allowance.
    OutputResourceLimit,
    /// A context challenge followed committed output.
    ChallengeAfterOutput,
    /// A private validation request identity cannot advance without reuse.
    RequestIdentityLimit,
}
pub(crate) fn failure(kind: BoundaryFailureKind, path: &'static str) -> BoundaryFailure {
    BoundaryFailure { kind, path }
}
pub(crate) fn object(
    value: JsonRef<'_>,
    allowed: &[&str],
    path: &'static str,
) -> Result<(), BoundaryFailure> {
    let fields = value
        .members()
        .ok_or_else(|| failure(BoundaryFailureKind::ExpectedObject, path))?;
    let mut seen = BTreeSet::new();
    for field in fields {
        let name = string(field.name, path)?;
        if !allowed.contains(&name) {
            return Err(failure(BoundaryFailureKind::UnexpectedMember, path));
        }
        if !seen.insert(name) {
            return Err(failure(BoundaryFailureKind::DuplicateMember, path));
        }
    }
    Ok(())
}
pub(crate) fn required<'a>(
    value: JsonRef<'a>,
    name: &str,
    path: &'static str,
) -> Result<JsonRef<'a>, BoundaryFailure> {
    value
        .get(name)
        .ok_or_else(|| failure(BoundaryFailureKind::MissingMember, path))
}
pub(crate) fn string<'a>(
    value: JsonRef<'a>,
    path: &'static str,
) -> Result<&'a str, BoundaryFailure> {
    value.as_str().ok_or_else(|| {
        failure(
            if value.kind() == JsonKind::String {
                BoundaryFailureKind::UnpairedString
            } else {
                BoundaryFailureKind::ExpectedString
            },
            path,
        )
    })
}
pub(crate) fn nonempty(value: &str, path: &'static str) -> Result<(), BoundaryFailure> {
    if value.is_empty() {
        Err(failure(BoundaryFailureKind::EmptyString, path))
    } else {
        Ok(())
    }
}
pub(crate) fn encode<T: Serialize>(
    value: &T,
    limits: JsonLimits,
) -> Result<JsonValue, BoundaryFailure> {
    struct Output {
        bytes: Vec<u8>,
        maximum: usize,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("invocation JSON byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        bytes: Vec::new(),
        maximum: limits.max_bytes,
    };
    serde_json::to_writer(&mut output, value)
        .map_err(|_| failure(BoundaryFailureKind::EncodingLimit, ""))?;
    JsonValue::parse_owned_with_limits(output.bytes, limits)
        .map_err(|_| failure(BoundaryFailureKind::EncodingLimit, ""))
}

impl From<crate::LimitError> for BoundaryFailure {
    fn from(_: crate::LimitError) -> Self {
        failure(BoundaryFailureKind::InvalidLimits, "/limits")
    }
}

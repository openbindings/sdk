//! By-value carrier admission shared by the companions that read or write one.
//!
//! These functions are delivery internals for the companions; no companion
//! reexports them. Codes come from the closed [`BoundaryCode`] set and paths are
//! fixed SDK strings, so no caller can mint a diagnostic outside that vocabulary.
use crate::BindingValue;
use openbindings::{JsonKind, JsonLimits, JsonRef, JsonValue, Preference};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{collections::HashSet, fmt};

/// A local structural diagnostic for a by-value carrier, never a portable
/// invocation failure.
///
/// Codes and paths come from a fixed SDK vocabulary and contain no source
/// values or caller-authored member names. A `*` path segment denotes any array
/// element. This record carries neither native causes nor protocol evidence.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BoundaryError {
    code: BoundaryCode,
    path: &'static str,
}

impl BoundaryError {
    /// Stable structural diagnostic code.
    pub fn code(&self) -> &'static str {
        self.code.as_str()
    }

    /// Safe structural location; empty denotes the root.
    pub fn path(&self) -> &'static str {
        self.path
    }
}

impl fmt::Debug for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundaryError")
            .field("code", &self.code())
            .field("path", &self.path)
            .finish()
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.code(), self.path)
    }
}

impl std::error::Error for BoundaryError {}

/// The closed set of structural diagnostic codes a [`BoundaryError`] carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryCode {
    /// A member is not in the carrier's closed set, or an extension name lacks `x-`.
    UnexpectedMember,
    /// A required member is absent.
    MissingMember,
    /// An object holds the same member name twice.
    DuplicateMember,
    /// A value that must be an object is not one.
    ExpectedObject,
    /// A value that must be an array is not one.
    ExpectedArray,
    /// A value that must be a string is not one.
    ExpectedString,
    /// A value that must be a Boolean is not one.
    ExpectedBoolean,
    /// A schema position holds neither an object nor a Boolean.
    ExpectedSchema,
    /// A string holds an unpaired UTF-16 unit.
    UnpairedString,
    /// A string that must be nonempty is empty.
    EmptyString,
    /// An array that must be nonempty is empty.
    EmptyArray,
    /// A requested kind is empty.
    EmptyKind,
    /// A binding's preference is not an integer in the supported range.
    InvalidPreference,
    /// A JSON Pointer is not valid RFC 6901 syntax.
    InvalidPointer,
    /// A carrier could not be encoded within the selected exact-JSON limits.
    EncodingLimit,
    /// A provider returned an error code the invoker facade reserves.
    ReservedProviderCode,
    /// A provider's terminal error exceeded the invocation's resource limits.
    TerminalResourceLimit,
    /// A provider's output exceeded the invocation's resource limits.
    OutputResourceLimit,
    /// A provider requested context after emitting output.
    ChallengeAfterOutput,
    /// A provider's advisory kind list fails the SDK's checks.
    AdvisoryList,
}

impl BoundaryCode {
    /// The code's stable kebab-case spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedMember => "unexpected-member",
            Self::MissingMember => "missing-member",
            Self::DuplicateMember => "duplicate-member",
            Self::ExpectedObject => "expected-object",
            Self::ExpectedArray => "expected-array",
            Self::ExpectedString => "expected-string",
            Self::ExpectedBoolean => "expected-boolean",
            Self::ExpectedSchema => "expected-schema",
            Self::UnpairedString => "unpaired-string",
            Self::EmptyString => "empty-string",
            Self::EmptyArray => "empty-array",
            Self::EmptyKind => "empty-kind",
            Self::InvalidPreference => "invalid-preference",
            Self::InvalidPointer => "invalid-pointer",
            Self::EncodingLimit => "encoding-limit",
            Self::ReservedProviderCode => "reserved-provider-code",
            Self::TerminalResourceLimit => "terminal-resource-limit",
            Self::OutputResourceLimit => "output-resource-limit",
            Self::ChallengeAfterOutput => "challenge-after-output",
            Self::AdvisoryList => "advisory-list",
        }
    }
}

/// Build a diagnostic from the SDK's closed code set. A function rather than a
/// constructor, so the companions that reexport [`BoundaryError`] expose none.
///
/// No caller can mint a code outside that set:
///
/// ```compile_fail,E0308
/// use openbindings_internal_interfaces::carrier::boundary_error;
/// let _ = boundary_error("arbitrary-code", "");
/// ```
pub const fn boundary_error(code: BoundaryCode, path: &'static str) -> BoundaryError {
    BoundaryError { code, path }
}

/// The closed code a diagnostic carries, for a companion that maps it into its
/// own error classification.
pub const fn boundary_code(error: &BoundaryError) -> BoundaryCode {
    error.code
}

/// Check a binding's extension names before it crosses a boundary.
pub fn admit_binding(binding: &BindingValue) -> Result<(), BoundaryError> {
    if binding
        .extensions
        .keys()
        .any(|name| !name.starts_with("x-"))
    {
        return Err(boundary_error(BoundaryCode::UnexpectedMember, "/binding"));
    }
    Ok(())
}

/// Write an admitted binding's members directly, without an intermediate value.
pub struct BindingFields<'a>(pub &'a BindingValue);

impl fmt::Debug for BindingFields<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BindingFields").field(self.0).finish()
    }
}

impl Serialize for BindingFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let b = self.0;
        let mut map = serializer.serialize_map(None)?;
        if let Some(v) = &b.operation {
            map.serialize_entry("operation", v)?;
        }
        if let Some(v) = &b.source {
            map.serialize_entry("source", v)?;
        }
        if let Some(v) = &b.content {
            map.serialize_entry("content", v)?;
        }
        if let Some(v) = &b.idempotent {
            map.serialize_entry("idempotent", v)?;
        }
        if let Some(v) = &b.preference {
            map.serialize_entry("preference", v)?;
        }
        if let Some(v) = &b.description {
            map.serialize_entry("description", v)?;
        }
        if let Some(v) = &b.deprecated {
            map.serialize_entry("deprecated", v)?;
        }
        for (name, value) in &b.extensions {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

/// Read a required member, or `missing-member` at `path`.
pub fn required<'a>(
    value: JsonRef<'a>,
    field: &str,
    path: &'static str,
) -> Result<JsonRef<'a>, BoundaryError> {
    value
        .get(field)
        .ok_or_else(|| boundary_error(BoundaryCode::MissingMember, path))
}

/// Require a nonempty array at `path`.
pub fn nonempty_array(value: JsonRef<'_>, path: &'static str) -> Result<(), BoundaryError> {
    if value.kind() != JsonKind::Array {
        return Err(boundary_error(BoundaryCode::ExpectedArray, path));
    }
    if value.is_empty() == Some(true) {
        return Err(boundary_error(BoundaryCode::EmptyArray, path));
    }
    Ok(())
}

/// Read a scalar string at `path`; a string with an unpaired UTF-16 unit is `unpaired-string`.
pub fn string<'a>(value: JsonRef<'a>, path: &'static str) -> Result<&'a str, BoundaryError> {
    value.as_str().ok_or_else(|| {
        boundary_error(
            if value.kind() == JsonKind::String {
                BoundaryCode::UnpairedString
            } else {
                BoundaryCode::ExpectedString
            },
            path,
        )
    })
}

/// Read a Boolean at `path`.
pub fn boolean(value: JsonRef<'_>, path: &'static str) -> Result<bool, BoundaryError> {
    value
        .as_bool()
        .ok_or_else(|| boundary_error(BoundaryCode::ExpectedBoolean, path))
}

/// Require an object at `path` whose member names are unique and either
/// `known` or, when `extensions` holds, `x-` extensions.
pub fn object(
    value: JsonRef<'_>,
    known: &[&str],
    extensions: bool,
    path: &'static str,
) -> Result<(), BoundaryError> {
    unique_object(value, path)?;
    for member in value.members().expect("checked object") {
        let name = member
            .name
            .as_str()
            .ok_or_else(|| boundary_error(BoundaryCode::UnexpectedMember, path))?;
        if !known.contains(&name) && !(extensions && name.starts_with("x-")) {
            return Err(boundary_error(BoundaryCode::UnexpectedMember, path));
        }
    }
    Ok(())
}

/// Require an object at `path` whose member names are unique.
pub fn unique_object(value: JsonRef<'_>, path: &'static str) -> Result<(), BoundaryError> {
    let members = value
        .members()
        .ok_or_else(|| boundary_error(BoundaryCode::ExpectedObject, path))?;
    let mut scalar_names = HashSet::new();
    let mut unpaired_names = HashSet::new();
    for member in members {
        let unique = if let Some(name) = member.name.as_str() {
            scalar_names.insert(name)
        } else {
            unpaired_names.insert(member.name.utf16_units().expect("member name"))
        };
        if !unique {
            return Err(boundary_error(BoundaryCode::DuplicateMember, path));
        }
    }
    Ok(())
}

// Reads a preference exactly as core's private document reader does; the
// differential test in the invocation crate's tests/values.rs holds the two aligned.
pub(crate) fn preference(value: JsonRef<'_>) -> Result<Preference, BoundaryError> {
    let invalid = || boundary_error(BoundaryCode::InvalidPreference, "/binding/preference");
    let text = value.number_text().ok_or_else(invalid)?;
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    // Saturation is safe: any exponent larger than the token's digit count plus
    // the target integer width cannot bring a nonzero value into range.
    let cap = i64::try_from(text.len())
        .unwrap_or(i64::MAX - 32)
        .saturating_add(32);
    let exponent_negative = exponent.starts_with('-');
    let exponent = exponent
        .trim_start_matches(['+', '-'])
        .bytes()
        .fold(0_i64, |n, b| {
            n.saturating_mul(10)
                .saturating_add(i64::from(b - b'0'))
                .min(cap)
        });
    let exponent = if exponent_negative {
        -exponent
    } else {
        exponent
    };
    let fractional = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = || mantissa.bytes().filter(|b| *b != b'.');
    let total = digits().count();
    let leading = digits().take_while(|b| *b == b'0').count();
    if leading == total {
        return Ok(Preference::new(0).expect("zero preference"));
    }
    let trailing = digits().rev().take_while(|b| *b == b'0').count();
    let power = exponent.saturating_sub(i64::try_from(fractional).map_err(|_| invalid())?);
    let integer_digits = i64::try_from(total - leading)
        .map_err(|_| invalid())?
        .saturating_add(power);
    if !(1..=16).contains(&integer_digits) || (power < 0 && power.unsigned_abs() > trailing as u64)
    {
        return Err(invalid());
    }
    let mut number = 0_i64;
    for digit in digits()
        .skip(leading)
        .chain(std::iter::repeat(b'0'))
        .take(integer_digits as usize)
    {
        number = number * 10 + i64::from(digit - b'0');
    }
    Preference::new(if negative { -number } else { number }).ok_or_else(invalid)
}

/// Serialize without building an unbounded output buffer or exposing error text.
/// Inspection and synthesis keep the same bounded writer for their own errors.
pub fn encode<T: Serialize>(value: &T, limits: JsonLimits) -> Result<JsonValue, BoundaryError> {
    struct Output {
        bytes: Vec<u8>,
        max: usize,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
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
        max: limits.max_bytes,
    };
    serde_json::to_writer(&mut output, value)
        .map_err(|_| boundary_error(BoundaryCode::EncodingLimit, ""))?;
    JsonValue::parse_with_limits(output.bytes, limits)
        .map_err(|_| boundary_error(BoundaryCode::EncodingLimit, ""))
}

#[cfg(test)]
mod tests {
    use super::{BoundaryCode, boundary_code, boundary_error};

    // Codes are part of each companion's contract, so their spellings stay fixed.
    #[test]
    fn every_boundary_code_keeps_its_stable_spelling() {
        use BoundaryCode::*;
        let codes = [
            (UnexpectedMember, "unexpected-member"),
            (MissingMember, "missing-member"),
            (DuplicateMember, "duplicate-member"),
            (ExpectedObject, "expected-object"),
            (ExpectedArray, "expected-array"),
            (ExpectedString, "expected-string"),
            (ExpectedBoolean, "expected-boolean"),
            (ExpectedSchema, "expected-schema"),
            (UnpairedString, "unpaired-string"),
            (EmptyString, "empty-string"),
            (EmptyArray, "empty-array"),
            (EmptyKind, "empty-kind"),
            (InvalidPreference, "invalid-preference"),
            (InvalidPointer, "invalid-pointer"),
            (EncodingLimit, "encoding-limit"),
            (ReservedProviderCode, "reserved-provider-code"),
            (TerminalResourceLimit, "terminal-resource-limit"),
            (OutputResourceLimit, "output-resource-limit"),
            (ChallengeAfterOutput, "challenge-after-output"),
            (AdvisoryList, "advisory-list"),
        ];
        for (code, spelling) in codes {
            let error = boundary_error(code, "/binding");
            assert_eq!(error.code(), spelling);
            assert_eq!(boundary_code(&error), code);
            assert_eq!(error.to_string(), format!("{spelling} at /binding"));
            assert_eq!(
                format!("{error:?}"),
                format!("BoundaryError {{ code: {spelling:?}, path: \"/binding\" }}")
            );
        }
    }
}

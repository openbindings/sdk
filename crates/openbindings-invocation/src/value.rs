//! Exact by-value carriers and structural admission.

use crate::BoundaryError;
use openbindings::{Binding, JsonKind, JsonLimits, JsonRef, JsonValue, Preference, Source};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

const BINDING_FIELDS: &[&str] = &[
    "operation",
    "source",
    "content",
    "idempotent",
    "preference",
    "description",
    "deprecated",
];

/// A binding passed by value, with no document lookup or operation selection.
///
/// Unlike the core authoring binding, the empty carrier is valid. Absence of
/// `operation`, `source`, or `content` is preserved; an empty string and JSON null
/// remain present values. Metadata is information, not invocation policy.
#[derive(Clone, Default)]
pub struct BindingValue {
    /// Optional author operation key, forwarded without interpretation.
    pub operation: Option<String>,
    /// Optional author source key, forwarded without interpretation.
    pub source: Option<String>,
    /// Exact kind-owned content; explicit null is distinct from absence.
    pub content: Option<JsonValue>,
    /// Optional author idempotency claim; does not authorize retries.
    pub idempotent: Option<bool>,
    /// Optional author preference, without ranking or selection behavior.
    pub preference: Option<Preference>,
    /// Optional human-facing description.
    pub description: Option<String>,
    /// Optional author deprecation annotation.
    pub deprecated: Option<bool>,
    /// Exact extension values. Names must start with `x-`; admission checks this.
    pub extensions: BTreeMap<String, JsonValue>,
}

impl BindingValue {
    /// Read a by-value binding without collapsing duplicate or unknown members.
    ///
    /// Opaque content and extension values retain exact snapshots, including
    /// duplicate names, number spelling and escaped unpaired UTF-16 units.
    ///
    /// # Errors
    /// Returns a boundary error for a malformed carrier: invalid field types, missing required fields, duplicate members, or unsupported fields. Opaque payloads are preserved.
    pub fn from_json(value: &JsonValue) -> Result<Self, BoundaryError> {
        Self::from_json_ref(value.view())
    }

    /// Read a borrowed by-value binding, retaining owners only for opaque values.
    ///
    /// # Errors
    /// Returns a boundary error for a malformed carrier: invalid field types, missing required fields, duplicate members, or unsupported fields. Opaque payloads are preserved.
    pub fn from_json_ref(value: JsonRef<'_>) -> Result<Self, BoundaryError> {
        object(value, BINDING_FIELDS, true, "/binding")?;
        let mut binding = Self::default();
        for member in value.members().expect("checked object") {
            let name = member.name.as_str().expect("checked carrier name");
            let v = member.value;
            match name {
                "operation" => binding.operation = Some(string(v, "/binding/operation")?.into()),
                "source" => binding.source = Some(string(v, "/binding/source")?.into()),
                "content" => binding.content = Some(v.to_owned()),
                "idempotent" => binding.idempotent = Some(boolean(v, "/binding/idempotent")?),
                "preference" => binding.preference = Some(preference(v)?),
                "description" => {
                    binding.description = Some(string(v, "/binding/description")?.into())
                }
                "deprecated" => binding.deprecated = Some(boolean(v, "/binding/deprecated")?),
                extension => {
                    binding.extensions.insert(extension.into(), v.to_owned());
                }
            }
        }
        Ok(binding)
    }

    pub(crate) fn admit(&self) -> Result<(), BoundaryError> {
        if self.extensions.keys().any(|name| !name.starts_with("x-")) {
            return Err(BoundaryError::new("unexpected-member", "/binding"));
        }
        Ok(())
    }

    /// Encode this carrier under the default finite exact-JSON limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Check extension names and encode with explicit exact-JSON limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryError> {
        self.admit()?;
        encode(&BindingFields(self), limits)
    }
}

impl From<&Binding> for BindingValue {
    fn from(binding: &Binding) -> Self {
        Self {
            operation: Some(binding.operation.clone()),
            source: Some(binding.source.clone()),
            content: binding.content.clone(),
            idempotent: binding.idempotent,
            preference: binding.preference,
            description: binding.description.clone(),
            deprecated: binding.deprecated,
            extensions: binding.additional_fields.clone(),
        }
    }
}

impl fmt::Debug for BindingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingValue")
            .field("operation_present", &self.operation.is_some())
            .field("source_present", &self.source.is_some())
            .field("content_present", &self.content.is_some())
            .finish_non_exhaustive()
    }
}

impl Serialize for BindingValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

struct BindingFields<'a>(&'a BindingValue);

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

/// Immutable borrowed carriers for one invocation or advisory preflight.
///
/// Context is an optional opaque object. Admission checks its object boundary,
/// without interpreting, normalizing, resolving, or mutating any member.
#[derive(Clone, Copy)]
pub struct BindingRequest<'a> {
    /// Source whose exact kind determines content interpretation.
    pub source: &'a Source,
    /// Binding passed directly, with no document lookup.
    pub binding: &'a BindingValue,
    /// Optional exact context object; `None` remains distinct from `{}`.
    pub context: Option<&'a JsonValue>,
}

impl BindingRequest<'_> {
    pub(crate) fn admit(&self) -> Result<(), BoundaryError> {
        if self.source.kind.is_empty() {
            return Err(BoundaryError::new("empty-string", "/source/kind"));
        }
        if self
            .source
            .additional_fields
            .keys()
            .any(|name| !name.starts_with("x-"))
        {
            return Err(BoundaryError::new("unexpected-member", "/source"));
        }
        self.binding.admit()?;
        if self
            .context
            .is_some_and(|value| value.kind() != JsonKind::Object)
        {
            return Err(BoundaryError::new("expected-object", "/context"));
        }
        Ok(())
    }
}

impl fmt::Debug for BindingRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingRequest")
            .field("source", &"<redacted>")
            .field("binding", &"<redacted>")
            .field("context_present", &self.context.is_some())
            .finish()
    }
}

/// Owned initial invocation payload read through the strict by-value boundary.
///
/// Opaque values retain their original exact backing arenas. Use [`Self::request`]
/// to borrow this carrier for repeated attempts without serializing the source.
#[derive(Clone)]
pub struct BindingInvocationInput {
    /// Source whose kind governs this invocation.
    pub source: Source,
    /// Direct binding value; the empty object is valid.
    pub binding: BindingValue,
    /// Optional opaque context object, distinct from absent context.
    pub context: Option<JsonValue>,
}

impl BindingInvocationInput {
    /// Read the closed input envelope and all interpreted carrier members.
    ///
    /// # Errors
    /// Returns a boundary error for a malformed carrier: invalid field types, missing required fields, duplicate members, or unsupported fields. Opaque payloads are preserved.
    pub fn from_json(value: &JsonValue) -> Result<Self, BoundaryError> {
        Self::from_json_ref(value.view())
    }

    /// Read a borrowed exact input without collapsing any duplicate carrier field.
    ///
    /// # Errors
    /// Returns a boundary error for a malformed carrier: invalid field types, missing required fields, duplicate members, or unsupported fields. Opaque payloads are preserved.
    pub fn from_json_ref(value: JsonRef<'_>) -> Result<Self, BoundaryError> {
        object(value, &["source", "binding", "context"], false, "")?;
        let input = Self {
            source: source_from_json(required(value, "source", "/source")?)?,
            binding: BindingValue::from_json_ref(required(value, "binding", "/binding")?)?,
            context: value.get("context").map(JsonRef::to_owned),
        };
        input.request().admit()?;
        Ok(input)
    }

    /// Borrow the same owned values for an invocation or advisory preflight.
    pub fn request(&self) -> BindingRequest<'_> {
        BindingRequest {
            source: &self.source,
            binding: &self.binding,
            context: self.context.as_ref(),
        }
    }

    /// Encode the admitted payload under default finite exact-JSON limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Check native carrier fields and encode under explicit exact-JSON limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryError> {
        self.request().admit()?;
        encode(&InputFields(self), limits)
    }
}

impl fmt::Debug for BindingInvocationInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingInvocationInput")
            .field("source", &"<redacted>")
            .field("binding", &"<redacted>")
            .field("context_present", &self.context.is_some())
            .finish()
    }
}

impl Serialize for BindingInvocationInput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

struct InputFields<'a>(&'a BindingInvocationInput);

impl Serialize for InputFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("source", &self.0.source)?;
        map.serialize_entry("binding", &BindingFields(&self.0.binding))?;
        if let Some(context) = &self.0.context {
            map.serialize_entry("context", context)?;
        }
        map.end()
    }
}

pub(crate) fn source_from_json(value: JsonRef<'_>) -> Result<Source, BoundaryError> {
    object(value, &["kind", "content", "description"], true, "/source")?;
    let kind = string(required(value, "kind", "/source/kind")?, "/source/kind")?;
    if kind.is_empty() {
        return Err(BoundaryError::new("empty-string", "/source/kind"));
    }
    let mut source = Source {
        kind: kind.into(),
        ..Source::default()
    };
    for member in value.members().expect("checked object") {
        let name = member.name.as_str().expect("checked carrier name");
        match name {
            "kind" => {}
            "content" => source.content = Some(member.value.to_owned()),
            "description" => {
                source.description = Some(string(member.value, "/source/description")?.into())
            }
            extension => {
                source
                    .additional_fields
                    .insert(extension.into(), member.value.to_owned());
            }
        }
    }
    Ok(source)
}

/// An immutable, structurally checked context challenge.
///
/// Exact family-specific fields remain available through [`Self::value`]. This
/// type validates structure only: it never evaluates schemas, validates target
/// trust, resolves credentials, or grants permission to persist context.
#[derive(Clone)]
pub struct ContextChallenge(JsonValue);

impl ContextChallenge {
    /// Check the closed outer shapes and open requirement objects.
    ///
    /// Alternatives and requirements must be nonempty. `config.value` requires
    /// string `point` and RFC 6901 `path`, and admits an optional object or Boolean
    /// schema. Empty opaque targets and empty pointers are valid.
    ///
    /// # Errors
    /// Returns a boundary error for a malformed carrier: invalid field types, missing required fields, duplicate members, or unsupported fields. Opaque payloads are preserved.
    pub fn new(value: JsonValue) -> Result<Self, BoundaryError> {
        Self::validate(value.view())?;
        Ok(Self(value))
    }

    pub(crate) fn checked(value: JsonValue) -> Self {
        Self(value)
    }

    pub(crate) fn validate(value: JsonRef<'_>) -> Result<(), BoundaryError> {
        object(value, &["target", "alternatives"], false, "/data")?;
        string(required(value, "target", "/data/target")?, "/data/target")?;
        let alternatives = required(value, "alternatives", "/data/alternatives")?;
        nonempty_array(alternatives, "/data/alternatives")?;
        for alternative in alternatives.elements().expect("checked array") {
            object(
                alternative,
                &["requirements"],
                false,
                "/data/alternatives/*",
            )?;
            let requirements = required(
                alternative,
                "requirements",
                "/data/alternatives/*/requirements",
            )?;
            nonempty_array(requirements, "/data/alternatives/*/requirements")?;
            for requirement in requirements.elements().expect("checked array") {
                validate_requirement(requirement)?;
            }
        }
        Ok(())
    }

    /// Borrow the complete exact challenge, including unknown requirement fields.
    pub fn value(&self) -> &JsonValue {
        &self.0
    }

    /// Read the opaque target assertion without interpreting it as a trusted key.
    pub fn target(&self) -> &str {
        self.0
            .get("target")
            .and_then(JsonRef::as_str)
            .expect("checked target")
    }

    /// Borrow the exact nonempty alternatives array in provider order.
    pub fn alternatives(&self) -> JsonRef<'_> {
        self.0.get("alternatives").expect("checked alternatives")
    }

    /// Copy the exact challenge into a standalone snapshot under default limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Copy the exact challenge under explicit byte, nesting and node limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryError> {
        JsonValue::parse_with_limits(self.0.bytes(), limits)
            .map_err(|_| BoundaryError::new("encoding-limit", ""))
    }

    /// Consume the wrapper and retain its complete exact value.
    pub fn into_value(self) -> JsonValue {
        self.0
    }
}

impl fmt::Debug for ContextChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextChallenge").finish_non_exhaustive()
    }
}

impl Serialize for ContextChallenge {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

fn validate_requirement(value: JsonRef<'_>) -> Result<(), BoundaryError> {
    const ROOT: &str = "/data/alternatives/*/requirements/*";
    unique_object(value, ROOT)?;
    let family = string(
        required(value, "type", "/data/alternatives/*/requirements/*/type")?,
        "/data/alternatives/*/requirements/*/type",
    )?;
    if family.is_empty() {
        return Err(BoundaryError::new(
            "empty-string",
            "/data/alternatives/*/requirements/*/type",
        ));
    }
    if let Some(name) = value.get("name")
        && string(name, "/data/alternatives/*/requirements/*/name")?.is_empty()
    {
        return Err(BoundaryError::new(
            "empty-string",
            "/data/alternatives/*/requirements/*/name",
        ));
    }
    if let Some(description) = value.get("description") {
        string(
            description,
            "/data/alternatives/*/requirements/*/description",
        )?;
    }
    if let Some(durable) = value.get("durable") {
        boolean(durable, "/data/alternatives/*/requirements/*/durable")?;
    }
    if let Some(point) = value.get("point") {
        string(point, "/data/alternatives/*/requirements/*/point")?;
    }
    if let Some(path) = value.get("path") {
        let path = string(path, "/data/alternatives/*/requirements/*/path")?;
        if family == "config.value" && !valid_pointer(path) {
            return Err(BoundaryError::new(
                "invalid-pointer",
                "/data/alternatives/*/requirements/*/path",
            ));
        }
    }
    if let Some(schema) = value.get("schema")
        && !matches!(schema.kind(), JsonKind::Object | JsonKind::Boolean)
    {
        return Err(BoundaryError::new(
            "expected-schema",
            "/data/alternatives/*/requirements/*/schema",
        ));
    }
    if family == "config.value" {
        required(value, "point", "/data/alternatives/*/requirements/*/point")?;
        required(value, "path", "/data/alternatives/*/requirements/*/path")?;
    }
    Ok(())
}

fn valid_pointer(pointer: &str) -> bool {
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return false;
    }
    let mut bytes = pointer.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return false;
        }
    }
    true
}

fn required<'a>(
    value: JsonRef<'a>,
    field: &str,
    path: &'static str,
) -> Result<JsonRef<'a>, BoundaryError> {
    value
        .get(field)
        .ok_or_else(|| BoundaryError::new("missing-member", path))
}

fn nonempty_array(value: JsonRef<'_>, path: &'static str) -> Result<(), BoundaryError> {
    if value.kind() != JsonKind::Array {
        return Err(BoundaryError::new("expected-array", path));
    }
    if value.is_empty() == Some(true) {
        return Err(BoundaryError::new("empty-array", path));
    }
    Ok(())
}

pub(crate) fn string<'a>(value: JsonRef<'a>, path: &'static str) -> Result<&'a str, BoundaryError> {
    value.as_str().ok_or_else(|| {
        BoundaryError::new(
            if value.kind() == JsonKind::String {
                "unpaired-string"
            } else {
                "expected-string"
            },
            path,
        )
    })
}

fn boolean(value: JsonRef<'_>, path: &'static str) -> Result<bool, BoundaryError> {
    value
        .as_bool()
        .ok_or_else(|| BoundaryError::new("expected-boolean", path))
}

pub(crate) fn object(
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
            .ok_or_else(|| BoundaryError::new("unexpected-member", path))?;
        if !known.contains(&name) && !(extensions && name.starts_with("x-")) {
            return Err(BoundaryError::new("unexpected-member", path));
        }
    }
    Ok(())
}

fn unique_object(value: JsonRef<'_>, path: &'static str) -> Result<(), BoundaryError> {
    let members = value
        .members()
        .ok_or_else(|| BoundaryError::new("expected-object", path))?;
    let mut scalar_names = HashSet::new();
    let mut unpaired_names = HashSet::new();
    for member in members {
        let unique = if let Some(name) = member.name.as_str() {
            scalar_names.insert(name)
        } else {
            unpaired_names.insert(member.name.utf16_units().expect("member name"))
        };
        if !unique {
            return Err(BoundaryError::new("duplicate-member", path));
        }
    }
    Ok(())
}

fn preference(value: JsonRef<'_>) -> Result<Preference, BoundaryError> {
    let invalid = || BoundaryError::new("invalid-preference", "/binding/preference");
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
pub(crate) fn encode<T: Serialize>(
    value: &T,
    limits: JsonLimits,
) -> Result<JsonValue, BoundaryError> {
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
        .map_err(|_| BoundaryError::new("encoding-limit", ""))?;
    JsonValue::parse_with_limits(output.bytes, limits)
        .map_err(|_| BoundaryError::new("encoding-limit", ""))
}

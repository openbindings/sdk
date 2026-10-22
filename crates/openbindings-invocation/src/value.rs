//! Exact by-value carriers and structural admission.

use openbindings::{JsonKind, JsonLimits, JsonRef, JsonValue, Source};
use openbindings_internal_interfaces::{
    BindingValue, BoundaryError,
    carrier::{
        BindingFields, BoundaryCode, admit_binding, boolean, boundary_error, encode,
        nonempty_array, object, required, string, unique_object,
    },
};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::fmt;

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
            return Err(boundary_error(BoundaryCode::EmptyString, "/source/kind"));
        }
        if self
            .source
            .additional_fields
            .keys()
            .any(|name| !name.starts_with("x-"))
        {
            return Err(boundary_error(BoundaryCode::UnexpectedMember, "/source"));
        }
        admit_binding(self.binding)?;
        if self
            .context
            .is_some_and(|value| value.kind() != JsonKind::Object)
        {
            return Err(boundary_error(BoundaryCode::ExpectedObject, "/context"));
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
        return Err(boundary_error(BoundaryCode::EmptyString, "/source/kind"));
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
            .map_err(|_| boundary_error(BoundaryCode::EncodingLimit, ""))
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
        return Err(boundary_error(
            BoundaryCode::EmptyString,
            "/data/alternatives/*/requirements/*/type",
        ));
    }
    if let Some(name) = value.get("name")
        && string(name, "/data/alternatives/*/requirements/*/name")?.is_empty()
    {
        return Err(boundary_error(
            BoundaryCode::EmptyString,
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
            return Err(boundary_error(
                BoundaryCode::InvalidPointer,
                "/data/alternatives/*/requirements/*/path",
            ));
        }
    }
    if let Some(schema) = value.get("schema")
        && !matches!(schema.kind(), JsonKind::Object | JsonKind::Boolean)
    {
        return Err(boundary_error(
            BoundaryCode::ExpectedSchema,
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

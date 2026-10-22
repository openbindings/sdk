//! The by-value binding the source inspector and binding invoker interfaces share.
use crate::carrier::{
    BindingFields, BoundaryError, admit_binding, boolean, encode, object, preference, string,
};
use openbindings::{Binding, JsonLimits, JsonRef, JsonValue, Preference};
use serde::{Serialize, Serializer};
use std::{collections::BTreeMap, fmt};

const BINDING_FIELDS: &[&str] = &[
    "operation",
    "source",
    "content",
    "idempotent",
    "preference",
    "description",
    "deprecated",
];

/// A binding passed by value, as an OpenBindings 0.2 document carries it or
/// would carry it, with no document lookup or operation selection.
///
/// The source inspector and binding invoker interfaces define this carrier
/// identically, so it is both a bindable target's binding and the binding an
/// invoker receives: an inspected target's binding invokes as it is. Unlike the
/// core authoring [`Binding`], every field is optional and the empty carrier is
/// valid. `operation` and `source` are the author's keys, forwarded without
/// interpretation; an inspected target carries neither until the author assigns
/// them with [`Self::bind`]. Absence of `operation`, `source` or `content` is
/// preserved; an empty string and JSON null remain present values. Metadata is
/// information, not invocation policy. `Debug` shows only which keys and
/// content are present.
///
/// ```
/// use openbindings::JsonValue;
/// use openbindings_internal_interfaces::BindingValue;
/// let binding = BindingValue {
///     content: Some(JsonValue::string("lookup")?),
///     ..Default::default()
/// };
/// let authored = binding.bind("catalog.lookup", "catalog")?;
/// assert_eq!(authored.operation, "catalog.lookup");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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
        admit_binding(self)?;
        encode(&BindingFields(self), limits)
    }

    /// Assign the author's operation and source keys, replacing any this value
    /// carries, and consume it into a core binding for a document.
    ///
    /// Rejects illegal extension names, including `operation` and `source`, even
    /// for a value built directly rather than read or returned by inspection.
    /// The completed document still needs ordinary core conformance validation.
    ///
    /// # Errors
    /// Returns a boundary error (`unexpected-member` at `/binding`) if an extension name does not start with `x-`.
    pub fn bind(
        self,
        operation: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Binding, BoundaryError> {
        admit_binding(&self)?;
        Ok(Binding {
            operation: operation.into(),
            source: source.into(),
            content: self.content,
            idempotent: self.idempotent,
            preference: self.preference,
            description: self.description,
            deprecated: self.deprecated,
            additional_fields: self.extensions,
        })
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

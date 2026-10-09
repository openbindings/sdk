//! Typed authoring of the normative vocabulary. Opaque fields retain exact JSON.
use crate::{AUTHORING_VERSION, InputError, JsonKind, JsonRef, JsonValue};
use jsonschema_value::ob_decimal::Decimal;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoringError {
    pub location: Option<crate::SourceLocation>,
    pub message: String,
}
impl fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AuthoringError {}
fn error(at: JsonRef<'_>, message: impl Into<String>) -> AuthoringError {
    AuthoringError {
        location: Some(at.location()),
        message: message.into(),
    }
}
trait Read: Sized {
    fn read(value: JsonRef<'_>) -> Result<Self, AuthoringError>;
}
impl Read for String {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| error(v, "expected a Unicode scalar string"))
    }
}
impl Read for bool {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        v.as_bool().ok_or_else(|| error(v, "expected a boolean"))
    }
}
impl Read for JsonValue {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        Ok(v.to_owned())
    }
}
impl<T: Read> Read for Vec<T> {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        v.elements()
            .ok_or_else(|| error(v, "expected an array"))?
            .map(T::read)
            .collect()
    }
}
impl<T: Read> Read for BTreeMap<String, T> {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        v.members()
            .ok_or_else(|| error(v, "expected an object"))?
            .map(|m| Ok((String::read(m.name)?, T::read(m.value)?)))
            .collect()
    }
}

/// An interoperable preference integer (inclusive ±9,007,199,254,740,991).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Preference(i64);
impl Preference {
    pub fn new(value: i64) -> Option<Self> {
        (-9_007_199_254_740_991..=9_007_199_254_740_991)
            .contains(&value)
            .then_some(Self(value))
    }
    pub fn get(self) -> i64 {
        self.0
    }
}
impl Read for Preference {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        let text = v
            .number_text()
            .ok_or_else(|| error(v, "expected a preference integer"))?;
        let decimal = Decimal::parse(text);
        if !decimal.is_integer()
            || decimal
                .compare(&Decimal::parse("-9007199254740991"))
                .is_lt()
            || decimal.compare(&Decimal::parse("9007199254740991")).is_gt()
        {
            return Err(error(
                v,
                "preference is outside the interoperable integer range",
            ));
        }
        // A checked integer in the interoperable range converts exactly through binary64.
        let n = text
            .parse::<f64>()
            .map_err(|_| error(v, "preference conversion failed"))? as i64;
        Ok(Self(n))
    }
}

#[derive(Clone, Debug)]
pub struct DocumentBuilder {
    pub openbindings: String,
    pub operations: BTreeMap<String, Operation>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub schemas: Option<BTreeMap<String, JsonValue>>,
    pub dependencies: Option<BTreeMap<String, Dependency>>,
    pub sources: Option<BTreeMap<String, Source>>,
    pub bindings: Option<BTreeMap<String, Binding>>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for DocumentBuilder {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &[
            "openbindings",
            "operations",
            "name",
            "version",
            "description",
            "schemas",
            "dependencies",
            "sources",
            "bindings",
        ];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("openbindings", &self.openbindings)?;
        map.serialize_entry("operations", &self.operations)?;
        if let Some(value) = &self.name {
            map.serialize_entry("name", value)?;
        }
        if let Some(value) = &self.version {
            map.serialize_entry("version", value)?;
        }
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        if let Some(value) = &self.schemas {
            map.serialize_entry("schemas", value)?;
        }
        if let Some(value) = &self.dependencies {
            map.serialize_entry("dependencies", value)?;
        }
        if let Some(value) = &self.sources {
            map.serialize_entry("sources", value)?;
        }
        if let Some(value) = &self.bindings {
            map.serialize_entry("bindings", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for DocumentBuilder {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            openbindings: <String>::read(
                v.get("openbindings")
                    .ok_or_else(|| error(v, "missing openbindings"))?,
            )?,
            operations: <BTreeMap<String, Operation>>::read(
                v.get("operations")
                    .ok_or_else(|| error(v, "missing operations"))?,
            )?,
            name: v.get("name").map(<String>::read).transpose()?,
            version: v.get("version").map(<String>::read).transpose()?,
            description: v.get("description").map(<String>::read).transpose()?,
            schemas: v
                .get("schemas")
                .map(<BTreeMap<String, JsonValue>>::read)
                .transpose()?,
            dependencies: v
                .get("dependencies")
                .map(<BTreeMap<String, Dependency>>::read)
                .transpose()?,
            sources: v
                .get("sources")
                .map(<BTreeMap<String, Source>>::read)
                .transpose()?,
            bindings: v
                .get("bindings")
                .map(<BTreeMap<String, Binding>>::read)
                .transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !([
                        "openbindings",
                        "operations",
                        "name",
                        "version",
                        "description",
                        "schemas",
                        "dependencies",
                        "sources",
                        "bindings",
                    ])
                    .contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Operation {
    pub description: Option<String>,
    pub deprecated: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    pub input: Option<JsonValue>,
    pub output: Option<JsonValue>,
    pub examples: Option<BTreeMap<String, OperationExample>>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Operation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &[
            "description",
            "deprecated",
            "tags",
            "aliases",
            "input",
            "output",
            "examples",
        ];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        if let Some(value) = &self.deprecated {
            map.serialize_entry("deprecated", value)?;
        }
        if let Some(value) = &self.tags {
            map.serialize_entry("tags", value)?;
        }
        if let Some(value) = &self.aliases {
            map.serialize_entry("aliases", value)?;
        }
        if let Some(value) = &self.input {
            map.serialize_entry("input", value)?;
        }
        if let Some(value) = &self.output {
            map.serialize_entry("output", value)?;
        }
        if let Some(value) = &self.examples {
            map.serialize_entry("examples", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for Operation {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            description: v.get("description").map(<String>::read).transpose()?,
            deprecated: v.get("deprecated").map(<bool>::read).transpose()?,
            tags: v.get("tags").map(<Vec<String>>::read).transpose()?,
            aliases: v.get("aliases").map(<Vec<String>>::read).transpose()?,
            input: v.get("input").map(<JsonValue>::read).transpose()?,
            output: v.get("output").map(<JsonValue>::read).transpose()?,
            examples: v
                .get("examples")
                .map(<BTreeMap<String, OperationExample>>::read)
                .transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !([
                        "description",
                        "deprecated",
                        "tags",
                        "aliases",
                        "input",
                        "output",
                        "examples",
                    ])
                    .contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct OperationExample {
    pub description: Option<String>,
    pub input: Option<JsonValue>,
    pub output: Option<JsonValue>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for OperationExample {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["description", "input", "output"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        if let Some(value) = &self.input {
            map.serialize_entry("input", value)?;
        }
        if let Some(value) = &self.output {
            map.serialize_entry("output", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for OperationExample {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            description: v.get("description").map(<String>::read).transpose()?,
            input: v.get("input").map(<JsonValue>::read).transpose()?,
            output: v.get("output").map(<JsonValue>::read).transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !(["description", "input", "output"]).contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Dependency {
    pub operation: String,
    pub kinds: Option<Vec<String>>,
    pub description: Option<String>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Dependency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["operation", "kinds", "description"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("operation", &self.operation)?;
        if let Some(value) = &self.kinds {
            map.serialize_entry("kinds", value)?;
        }
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for Dependency {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            operation: <String>::read(
                v.get("operation")
                    .ok_or_else(|| error(v, "missing operation"))?,
            )?,
            kinds: v.get("kinds").map(<Vec<String>>::read).transpose()?,
            description: v.get("description").map(<String>::read).transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !(["operation", "kinds", "description"])
                        .contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Source {
    pub kind: String,
    pub content: Option<JsonValue>,
    pub description: Option<String>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["kind", "content", "description"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        if let Some(value) = &self.content {
            map.serialize_entry("content", value)?;
        }
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for Source {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            kind: <String>::read(v.get("kind").ok_or_else(|| error(v, "missing kind"))?)?,
            content: v.get("content").map(<JsonValue>::read).transpose()?,
            description: v.get("description").map(<String>::read).transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !(["kind", "content", "description"]).contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Binding {
    pub operation: String,
    pub source: String,
    pub content: Option<JsonValue>,
    pub idempotent: Option<bool>,
    pub preference: Option<Preference>,
    pub description: Option<String>,
    pub deprecated: Option<bool>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &[
            "operation",
            "source",
            "content",
            "idempotent",
            "preference",
            "description",
            "deprecated",
        ];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(format!(
                    "additional field shadows typed member {key}"
                )));
            }
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("operation", &self.operation)?;
        map.serialize_entry("source", &self.source)?;
        if let Some(value) = &self.content {
            map.serialize_entry("content", value)?;
        }
        if let Some(value) = &self.idempotent {
            map.serialize_entry("idempotent", value)?;
        }
        if let Some(value) = &self.preference {
            map.serialize_entry("preference", value)?;
        }
        if let Some(value) = &self.description {
            map.serialize_entry("description", value)?;
        }
        if let Some(value) = &self.deprecated {
            map.serialize_entry("deprecated", value)?;
        }
        for (k, v) in &self.additional_fields {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl Read for Binding {
    fn read(v: JsonRef<'_>) -> Result<Self, AuthoringError> {
        if v.kind() != JsonKind::Object {
            return Err(error(v, "expected an object"));
        }
        Ok(Self {
            operation: <String>::read(
                v.get("operation")
                    .ok_or_else(|| error(v, "missing operation"))?,
            )?,
            source: <String>::read(v.get("source").ok_or_else(|| error(v, "missing source"))?)?,
            content: v.get("content").map(<JsonValue>::read).transpose()?,
            idempotent: v.get("idempotent").map(<bool>::read).transpose()?,
            preference: v.get("preference").map(<Preference>::read).transpose()?,
            description: v.get("description").map(<String>::read).transpose()?,
            deprecated: v.get("deprecated").map(<bool>::read).transpose()?,
            additional_fields: v
                .members()
                .unwrap()
                .filter(|m| {
                    !([
                        "operation",
                        "source",
                        "content",
                        "idempotent",
                        "preference",
                        "description",
                        "deprecated",
                    ])
                    .contains(&m.name.as_str().unwrap_or(""))
                })
                .map(|m| Ok((String::read(m.name)?, m.value.to_owned())))
                .collect::<Result<_, AuthoringError>>()?,
        })
    }
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self {
            openbindings: AUTHORING_VERSION.into(),
            operations: BTreeMap::new(),
            name: None,
            version: None,
            description: None,
            schemas: None,
            dependencies: None,
            sources: None,
            bindings: None,
            additional_fields: BTreeMap::new(),
        }
    }
}
impl DocumentBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Reads only representable normative fields; assessment remains available for every parsed input.
    pub fn from_json(value: &JsonValue) -> Result<Self, AuthoringError> {
        if value.has_duplicate_names() {
            return Err(error(
                value.view(),
                "duplicate member names prevent typed authoring",
            ));
        }
        Self::read(value.view())
    }
    /// Creates a new independent snapshot. This does not claim conformance.
    pub fn to_json(&self) -> Result<JsonValue, AuthoringError> {
        let bytes = serde_json::to_vec(self).map_err(|e| AuthoringError {
            location: None,
            message: e.to_string(),
        })?;
        JsonValue::parse(bytes).map_err(|e: InputError| AuthoringError {
            location: None,
            message: e.to_string(),
        })
    }
}
impl Dependency {
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.kinds
            .as_ref()
            .is_none_or(|kinds| kinds.iter().any(|k| k == kind))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absence_null_and_unknown_survive() {
        let value=JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{"go":{"input":true,"examples":{"one":{"input":null}}}},"sources":{"x":{"kind":"custom","content":null}},"x-empty":[],"unknown":9007199254740993}"#).unwrap();
        let model = DocumentBuilder::from_json(&value).unwrap();
        let output = model.to_json().unwrap();
        assert_eq!(value.semantic_eq(&output), Some(true));
    }
    #[test]
    fn optional_typed_null_is_an_error() {
        let value =
            JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{},"name":null}"#).unwrap();
        assert!(DocumentBuilder::from_json(&value).is_err());
    }
    #[test]
    fn reserved_member_collision_is_not_serialized() {
        let mut b = DocumentBuilder::new();
        b.additional_fields
            .insert("operations".into(), JsonValue::null());
        assert!(b.to_json().is_err());
        assert!(serde_json::to_string(&b).is_err());
    }
    #[test]
    fn preference_mathematical_forms() {
        for n in [
            "1.0",
            "10e-1",
            "9007199254740991.0",
            "-9.007199254740991e15",
        ] {
            let value=JsonValue::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{}},"bindings":{{"b":{{"operation":"x","source":"s","preference":{n}}}}}}}"#)).unwrap();
            assert!(DocumentBuilder::from_json(&value).is_ok(), "{n}");
        }
    }
}

//! Typed authoring of the normative vocabulary. Opaque fields retain exact JSON.
use crate::{EditError, JsonRef, JsonValue};
use openbindings_internal_json::decimal::Decimal;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::collections::BTreeMap;

fn check_fields(
    fields: &BTreeMap<String, JsonValue>,
    reserved: &[&str],
    path: &[&str],
) -> Result<(), EditError> {
    for key in fields.keys() {
        if !reserved.contains(&key.as_str()) {
            continue;
        }
        let mut pointer = String::new();
        let mut omitted = false;
        'segments: for segment in path
            .iter()
            .copied()
            .chain(["additional_fields", key.as_str()])
        {
            if pointer.len() == 4096 {
                omitted = true;
                break;
            }
            pointer.push('/');
            for ch in segment.chars() {
                let mut buf = [0; 4];
                let text = match ch {
                    '~' => "~0",
                    '/' => "~1",
                    _ => ch.encode_utf8(&mut buf),
                };
                if text.len() > 4096 - pointer.len() {
                    omitted = true;
                    break 'segments;
                }
                pointer.push_str(text);
            }
        }
        return Err(EditError::FieldCollision {
            part_pointer: (!omitted).then_some(pointer),
        });
    }
    Ok(())
}

/// An interoperable preference integer (inclusive ±9,007,199,254,740,991).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Preference(i64);
impl Preference {
    /// Return `None` outside the inclusive interoperable integer range ±9,007,199,254,740,991.
    pub fn new(value: i64) -> Option<Self> {
        (-9_007_199_254_740_991..=9_007_199_254_740_991)
            .contains(&value)
            .then_some(Self(value))
    }
    /// Return the exact checked integer, without conversion to floating point.
    pub fn get(self) -> i64 {
        self.0
    }
    // Exact conversion shared by the retained normative read views; `None` for a
    // value that is not an integer in the interoperable range.
    pub(crate) fn from_json_value(v: JsonRef<'_>) -> Option<Self> {
        let text = v.number_text()?;
        let decimal = Decimal::parse(text);
        if !decimal.is_integer()
            || decimal
                .compare(&Decimal::parse("-9007199254740991"))
                .is_lt()
            || decimal.compare(&Decimal::parse("9007199254740991")).is_gt()
        {
            return None;
        }
        // A checked integer in the interoperable range converts exactly through binary64.
        Some(Self(text.parse::<f64>().ok()? as i64))
    }
}

#[derive(Clone, Default)]
/// Editable operation metadata and optional input/output schemas. A present schema is not evidence that an evaluator supports it.
pub struct Operation {
    /// Optional human-facing operation description.
    pub description: Option<String>,
    /// Optional deprecation annotation; omission is distinct from explicit false.
    pub deprecated: Option<bool>,
    /// Optional documentation labels for grouping and filtering. Their order carries no
    /// meaning; the draft keeps them as authored.
    pub tags: Option<Vec<String>>,
    /// Optional alternative operation names in the shared primary-key/alias namespace.
    pub aliases: Option<Vec<String>>,
    /// Optional input JSON Schema; absence means no input contract, while false is a present schema.
    pub input: Option<JsonValue>,
    /// Optional output JSON Schema; absence means no output contract, while false is a present schema.
    pub output: Option<JsonValue>,
    /// Optional named examples carrying exact input/output instances.
    pub examples: Option<BTreeMap<String, Example>>,
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
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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

#[derive(Clone, Default)]
/// Editable example values; these are instances, not schemas or proof that an operation accepts them.
pub struct Example {
    /// Optional human-facing explanation of this example.
    pub description: Option<String>,
    /// Optional exact input instance; explicit JSON null is preserved.
    pub input: Option<JsonValue>,
    /// Optional exact output instance; explicit JSON null is preserved.
    pub output: Option<JsonValue>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Example {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["description", "input", "output"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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

#[derive(Clone, Default)]
/// Editable dependency: a consumption point where the described component consumes a
/// realization of one of this document's operations, optionally limited to listed kinds.
pub struct Dependency {
    /// Referenced operation name; document validation checks the applicable normative constraints.
    pub operation: String,
    /// Allowed source kinds. Absence accepts every kind; a conformant present list
    /// must be nonempty. An empty list is representable but fails normative validation.
    pub kinds: Option<Vec<String>>,
    /// Optional human-facing dependency description.
    pub description: Option<String>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Dependency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["operation", "kinds", "description"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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

#[derive(Clone, Default)]
/// Editable source metadata and opaque kind-specific content. Core preserves content without acquiring resources or executing it.
pub struct Source {
    /// Kind: the exact, opaque string that selects how this source and its bindings are read.
    pub kind: String,
    /// Optional exact kind-specific content; explicit JSON null remains present.
    pub content: Option<JsonValue>,
    /// Optional human-facing source description.
    pub description: Option<String>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const RESERVED: &[&str] = &["kind", "content", "description"];
        for key in self.additional_fields.keys() {
            if RESERVED.contains(&key.as_str()) {
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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

#[derive(Clone, Default)]
/// Editable binding: an author-declared realization of an operation through a source. Core retains binding content and annotations; invocation and selection policy belong to the consumer.
pub struct Binding {
    /// Primary operation name referenced by this binding.
    pub operation: String,
    /// Key of the source through which this binding realizes its operation.
    pub source: String,
    /// Optional exact source-kind-specific binding content.
    pub content: Option<JsonValue>,
    /// Optional idempotence annotation; it does not cause retries or execution.
    pub idempotent: Option<bool>,
    /// Optional exact interoperable preference integer; core does not rank bindings.
    pub preference: Option<Preference>,
    /// Optional human-facing binding description.
    pub description: Option<String>,
    /// Optional deprecation annotation, independent of operation deprecation.
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
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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

impl Dependency {
    /// Test the declared kind filter with exact string equality; absence accepts all.
    /// An empty list matches none but is nonconformant. This does not establish
    /// conformance or resolve or execute a dependency.
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.kinds
            .as_ref()
            .is_none_or(|kinds| kinds.iter().any(|k| k == kind))
    }
}
impl std::fmt::Debug for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Example {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Example").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dependency").finish_non_exhaustive()
    }
}

// Parts are checked before serialization so native paths remain precise.
pub(crate) trait Part: Serialize {
    fn check(&self, path: &[&str]) -> Result<(), EditError>;
}
impl<T: Part> Part for BTreeMap<String, T> {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        // Reuse one diagnostic path for the entire namespace.
        let mut child = Vec::with_capacity(path.len() + 1);
        child.extend_from_slice(path);
        for (key, value) in self {
            child.push(key);
            value.check(&child)?;
            child.pop();
        }
        Ok(())
    }
}
impl Part for Operation {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        check_fields(
            &self.additional_fields,
            &[
                "description",
                "deprecated",
                "tags",
                "aliases",
                "input",
                "output",
                "examples",
            ],
            path,
        )?;
        if let Some(examples) = &self.examples {
            let mut path = path.to_vec();
            path.push("examples");
            examples.check(&path)?;
        }
        Ok(())
    }
}
impl Part for Example {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        check_fields(
            &self.additional_fields,
            &["description", "input", "output"],
            path,
        )
    }
}
impl Part for Binding {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        check_fields(
            &self.additional_fields,
            &[
                "operation",
                "source",
                "content",
                "idempotent",
                "preference",
                "description",
                "deprecated",
            ],
            path,
        )
    }
}
impl Part for Source {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        check_fields(
            &self.additional_fields,
            &["kind", "content", "description"],
            path,
        )
    }
}
impl Part for Dependency {
    fn check(&self, path: &[&str]) -> Result<(), EditError> {
        check_fields(
            &self.additional_fields,
            &["operation", "kinds", "description"],
            path,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn absence_null_and_unknown_survive() {
        let value = JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{"go":{"input":true,"examples":{"one":{"input":null}}}},"sources":{"x":{"kind":"custom","content":null}},"x-empty":[],"unknown":9007199254740993}"#).unwrap();
        let model = Document::from_json(value.clone());
        assert_eq!(value.semantic_eq(model.value()), Some(true));
        assert!(model.name().unwrap().is_none());
        assert_eq!(
            model
                .source("x")
                .unwrap()
                .unwrap()
                .content()
                .unwrap()
                .kind(),
            crate::JsonKind::Null
        );
    }
    #[test]
    fn optional_typed_null_is_an_error() {
        let model =
            Document::parse(r#"{"openbindings":"0.2.0","operations":{},"name":null}"#).unwrap();
        assert!(model.name().is_err());
    }
    #[test]
    fn reserved_member_collision_is_not_serialized() {
        let mut operation = Operation::default();
        operation
            .additional_fields
            .insert("input".into(), JsonValue::null());
        let mut model = Document::new();
        assert!(matches!(
            model.set_operation("go", &operation),
            Err(EditError::FieldCollision { .. })
        ));
        assert!(serde_json::to_string(&operation).is_err());
    }
    #[test]
    fn preference_mathematical_forms() {
        for n in [
            "1.0",
            "10e-1",
            "9007199254740991.0",
            "-9.007199254740991e15",
        ] {
            assert!(
                Preference::from_json_value(JsonValue::parse(n).unwrap().view()).is_some(),
                "{n}"
            );
        }
    }
}

//! Typed authoring of the normative vocabulary. Opaque fields retain exact JSON.
use crate::{AUTHORING_VERSION, InputError, JsonKind, JsonRef, JsonValue};
use jsonschema_value::ob_decimal::Decimal;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{collections::BTreeMap, fmt};

/// Stable authoring failures, separate from document conformance findings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthoringErrorKind {
    /// An additional field would shadow a typed normative member.
    FieldCollision,
    /// A field cannot be represented by the typed authoring model.
    InvalidField,
    /// Repeated object names prevent lossless typed authoring.
    DuplicateMembers,
    /// The draft could not be encoded as supported exact JSON.
    Serialization,
    /// Encoding or parsing exceeded an admission limit.
    Limit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Expected authoring failure. A native draft pointer and an original source location are distinct coordinates; inspect the available one. Messages explain failures, while `kind` supports branching.
pub struct AuthoringError {
    kind: AuthoringErrorKind,
    draft_pointer: Option<String>,
    source_location: Option<crate::SourceLocation>,
    message: String,
    path_omitted_for_limit: bool,
}
impl AuthoringError {
    /// Return the stable failure category; callers must allow future categories.
    pub fn kind(&self) -> AuthoringErrorKind {
        self.kind
    }
    /// Logical native draft path, including `additional_fields`; not a source location.
    pub fn draft_pointer(&self) -> Option<&str> {
        self.draft_pointer.as_deref()
    }
    /// Borrow the original parsed location when converting source JSON; native drafts have no invented byte coordinates.
    pub fn source_location(&self) -> Option<&crate::SourceLocation> {
        self.source_location.as_ref()
    }
    /// Borrow the explanatory message. Use `kind` for program logic and escape text for the presentation context.
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Whether a draft pointer was omitted because its escaped UTF-8 spelling would exceed 4096 bytes; an omitted path is not the root.
    pub fn path_omitted_for_limit(&self) -> bool {
        self.path_omitted_for_limit
    }
    fn plain(kind: AuthoringErrorKind, message: &str) -> Self {
        Self {
            kind,
            draft_pointer: None,
            source_location: None,
            message: message.into(),
            path_omitted_for_limit: false,
        }
    }
}
impl fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AuthoringError {}
fn error(at: JsonRef<'_>, message: impl Into<String>) -> AuthoringError {
    AuthoringError {
        kind: AuthoringErrorKind::InvalidField,
        draft_pointer: None,
        source_location: Some(at.location()),
        message: message.into(),
        path_omitted_for_limit: false,
    }
}
fn check_fields(
    fields: &BTreeMap<String, JsonValue>,
    reserved: &[&str],
    path: &[&str],
) -> Result<(), AuthoringError> {
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
        return Err(AuthoringError {
            kind: AuthoringErrorKind::FieldCollision,
            draft_pointer: (!omitted).then_some(pointer),
            source_location: None,
            message: "additional field shadows a typed member".into(),
            path_omitted_for_limit: omitted,
        });
    }
    Ok(())
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
/// Owned, editable normative vocabulary. `None` omits an optional member; opaque `JsonValue::null()` remains present JSON null. Building checks representability and collisions, not conformance. Assess the new immutable snapshot before publication.
pub struct DocumentBuilder {
    /// Declared specification version; defaults to [`AUTHORING_VERSION`].
    pub openbindings: String,
    /// Primary operation keys and draft operations; aliases share their namespace during interpretation.
    pub operations: BTreeMap<String, Operation>,
    /// Optional human-facing interface name.
    pub name: Option<String>,
    /// Optional interface-defined version, independent of the `openbindings` declaration.
    pub version: Option<String>,
    /// Optional human-facing interface description.
    pub description: Option<String>,
    /// Optional named JSON Schema values; exact opaque contents remain unmodified.
    pub schemas: Option<BTreeMap<String, JsonValue>>,
    /// Optional named dependencies on operation contracts.
    pub dependencies: Option<BTreeMap<String, Dependency>>,
    /// Optional named binding sources, interpreted by their source kind outside core.
    pub sources: Option<BTreeMap<String, Source>>,
    /// Optional named operation-to-source bindings; core does not invoke or rank them.
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
                return Err(serde::ser::Error::custom(
                    "additional field shadows a typed member",
                ));
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
/// Editable operation metadata and optional input/output schemas. A present schema is not evidence that an evaluator supports it.
pub struct Operation {
    /// Optional human-facing operation description.
    pub description: Option<String>,
    /// Optional deprecation annotation; omission is distinct from explicit false.
    pub deprecated: Option<bool>,
    /// Optional application-facing classification tags, preserving order.
    pub tags: Option<Vec<String>>,
    /// Optional alternative operation names in the shared primary-key/alias namespace.
    pub aliases: Option<Vec<String>>,
    /// Optional input JSON Schema; absence means no input contract, while false is a present schema.
    pub input: Option<JsonValue>,
    /// Optional output JSON Schema; absence means no output contract, while false is a present schema.
    pub output: Option<JsonValue>,
    /// Optional named examples carrying exact input/output instances.
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
/// Editable example values; these are instances, not schemas or proof that an operation accepts them.
pub struct OperationExample {
    /// Optional human-facing explanation of this example.
    pub description: Option<String>,
    /// Optional exact input instance; explicit JSON null is preserved.
    pub input: Option<JsonValue>,
    /// Optional exact output instance; explicit JSON null is preserved.
    pub output: Option<JsonValue>,
    /// Extension and unknown members, retained verbatim; conformance determines whether each is allowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl Serialize for OperationExample {
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
/// Editable dependency on another operation, with an optional source-kind restriction.
pub struct Dependency {
    /// Referenced operation name; document assessment checks the applicable normative constraints.
    pub operation: String,
    /// Allowed source kinds. Absence accepts every kind; an empty list accepts none.
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
/// Editable source metadata and opaque kind-specific content. Core preserves content without acquiring resources or executing it.
pub struct Source {
    /// Source-kind identifier selecting the external binding vocabulary.
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
/// Editable link from an operation to a source. Core retains binding content and annotations; invocation and selection policy belong to the consumer.
pub struct Binding {
    /// Primary operation name referenced by this binding.
    pub operation: String,
    /// Key of the source containing this binding's interpretation context.
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
    /// Create an empty draft declaring [`AUTHORING_VERSION`], with all optional fields absent.
    pub fn new() -> Self {
        Self::default()
    }
    /// Reads only representable normative fields; assessment remains available for every parsed input.
    pub fn from_json(value: &JsonValue) -> Result<Self, AuthoringError> {
        if value.has_duplicate_names() {
            let mut error = error(
                value.view(),
                "duplicate member names prevent typed authoring",
            );
            error.kind = AuthoringErrorKind::DuplicateMembers;
            return Err(error);
        }
        Self::read(value.view())
    }
    /// Creates a new independent snapshot. This does not claim conformance.
    pub fn to_json(&self) -> Result<JsonValue, AuthoringError> {
        self.check_collisions()?;
        struct BoundedOutput {
            bytes: Vec<u8>,
            max: usize,
            exceeded: bool,
        }
        impl std::io::Write for BoundedOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                    self.exceeded = true;
                    return Err(std::io::Error::other("authoring byte limit exceeded"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = BoundedOutput {
            bytes: Vec::new(),
            max: crate::JsonLimits::default().max_bytes,
            exceeded: false,
        };
        serde_json::to_writer(&mut output, self).map_err(|_| {
            AuthoringError::plain(
                if output.exceeded {
                    AuthoringErrorKind::Limit
                } else {
                    AuthoringErrorKind::Serialization
                },
                "draft could not be serialized within the supported JSON profile",
            )
        })?;
        JsonValue::parse(output.bytes).map_err(|e: InputError| {
            AuthoringError::plain(
                if e.kind == crate::InputErrorKind::Limit {
                    AuthoringErrorKind::Limit
                } else {
                    AuthoringErrorKind::Serialization
                },
                e.code,
            )
        })
    }
}
impl Dependency {
    /// Test the declared kind filter with exact string equality; absence accepts all and an empty list accepts none. This does not resolve or execute a dependency.
    pub fn accepts_kind(&self, kind: &str) -> bool {
        self.kinds
            .as_ref()
            .is_none_or(|kinds| kinds.iter().any(|k| k == kind))
    }
}
impl DocumentBuilder {
    fn check_collisions(&self) -> Result<(), AuthoringError> {
        check_fields(
            &self.additional_fields,
            &[
                "openbindings",
                "operations",
                "name",
                "version",
                "description",
                "schemas",
                "dependencies",
                "sources",
                "bindings",
            ],
            &[],
        )?;
        for (key, operation) in &self.operations {
            check_fields(
                &operation.additional_fields,
                &[
                    "description",
                    "deprecated",
                    "tags",
                    "aliases",
                    "input",
                    "output",
                    "examples",
                ],
                &["operations", key],
            )?;
            if let Some(examples) = &operation.examples {
                for (name, example) in examples {
                    check_fields(
                        &example.additional_fields,
                        &["description", "input", "output"],
                        &["operations", key, "examples", name],
                    )?;
                }
            }
        }
        if let Some(entries) = &self.dependencies {
            for (key, entry) in entries {
                check_fields(
                    &entry.additional_fields,
                    &["operation", "kinds", "description"],
                    &["dependencies", key],
                )?;
            }
        }
        if let Some(entries) = &self.sources {
            for (key, entry) in entries {
                check_fields(
                    &entry.additional_fields,
                    &["kind", "content", "description"],
                    &["sources", key],
                )?;
            }
        }
        if let Some(entries) = &self.bindings {
            for (key, entry) in entries {
                check_fields(
                    &entry.additional_fields,
                    &[
                        "operation",
                        "source",
                        "content",
                        "idempotent",
                        "preference",
                        "description",
                        "deprecated",
                    ],
                    &["bindings", key],
                )?;
            }
        }
        Ok(())
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

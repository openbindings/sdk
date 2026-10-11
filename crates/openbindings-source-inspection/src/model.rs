use crate::{InspectionError, InspectionErrorKind};
use openbindings::{Binding, JsonLimits, JsonValue, Operation, Preference};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use std::collections::{BTreeMap, HashSet};

/// Advisory description of a supported exact kind; omission implies nothing.
#[derive(Clone, Debug, Serialize)]
pub struct SupportedKind {
    /// Exact, opaque source kind.
    pub kind: String,
    /// Optional human-facing description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Authoritative support verdict for this provider's inspection job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindSupportVerdict {
    /// Exact requested kind, unchanged.
    pub kind: String,
    /// Whether the provider understands this kind for inspection, not a promise
    /// that every artifact can be handled completely.
    pub supported: bool,
}

/// Binding data returned before an author assigns operation and source keys.
///
/// Absent content and explicit JSON null remain distinct. Metadata is information,
/// not execution policy. Only source-evidenced idempotency claims belong here.
#[derive(Clone, Debug, Default)]
pub struct BindingFragment {
    /// Optional exact kind-owned target description.
    pub content: Option<JsonValue>,
    /// Idempotency stated by the observed source; do not infer a retry policy.
    pub idempotent: Option<bool>,
    /// Optional authoring preference.
    pub preference: Option<Preference>,
    /// Optional human-facing description.
    pub description: Option<String>,
    /// Optional deprecation annotation.
    pub deprecated: Option<bool>,
    /// Extension members. Keys must begin with `x-`; checked before use.
    pub extensions: BTreeMap<String, JsonValue>,
}

impl BindingFragment {
    pub(crate) fn validate(&self) -> Result<(), InspectionError> {
        if let Some(key) = self.extensions.keys().find(|key| !key.starts_with("x-")) {
            return Err(InspectionError::result(format!(
                "binding fragment member {key:?} is not an x- extension"
            )));
        }
        Ok(())
    }

    /// Assign the author's keys and consume this fragment into a core binding.
    ///
    /// Rejects illegal extension keys, including `operation` and `source`, even
    /// when a fragment was constructed directly rather than returned by inspection.
    /// The completed document still needs ordinary core conformance assessment.
    pub fn bind(
        self,
        operation: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Binding, InspectionError> {
        self.validate()?;
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

impl Serialize for BindingFragment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut map = serializer.serialize_map(None)?;
        if let Some(v) = &self.content {
            map.serialize_entry("content", v)?;
        }
        if let Some(v) = &self.idempotent {
            map.serialize_entry("idempotent", v)?;
        }
        if let Some(v) = &self.preference {
            map.serialize_entry("preference", v)?;
        }
        if let Some(v) = &self.description {
            map.serialize_entry("description", v)?;
        }
        if let Some(v) = &self.deprecated {
            map.serialize_entry("deprecated", v)?;
        }
        for (key, value) in &self.extensions {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// One target admitted by the source's kind, with optional operation framing.
#[derive(Clone, Debug)]
pub struct BindableTarget {
    /// Stable source-local identity, unique within this inspection by exact string
    /// equality. Cross-provider equality exists only if the
    /// kind defines it; synthesis in this same provider must use the same identity.
    pub source_ref: String,
    /// Binding data without author-owned operation/source keys.
    pub binding: BindingFragment,
    /// Optional suggestion; the author owns the actual operation key.
    pub operation_key: Option<String>,
    /// Optional sound projection of this target. It must not carry `idempotent`.
    /// This is authoring data, not an already validated operation contract.
    pub operation: Option<Operation>,
}

/// Evidence explaining omitted or unestablished admitted targets.
#[derive(Clone, Debug, Serialize)]
pub struct InspectionLimitation {
    /// Nonempty, stable namespaced reason code.
    pub code: String,
    /// Nonempty explanation of what may be missing and why.
    pub message: String,
    /// Optional structured evidence; exact values are preserved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, JsonValue>>,
}

/// An inventory of targets the kind admits, in stable provider-defined order.
///
/// Complete empty inventories are valid. Excluded upstream interactions are not
/// targets. An admitted target that cannot be established or represented requires
/// `exhaustive: false` and a limitation. An unreadable source is a refusal instead.
#[derive(Clone, Debug)]
pub struct SourceInspection {
    /// Bindable targets; this inventory does not silently apply a relevance filter.
    pub targets: Vec<BindableTarget>,
    /// Whether every admitted target was reported.
    pub exhaustive: bool,
    /// Required for an incomplete inventory. The role also permits evidence on a
    /// complete result; consumers use `exhaustive` as the completeness signal.
    pub limitation: Option<InspectionLimitation>,
}

impl SourceInspection {
    /// Encode a checked inventory as exact JSON with the core's default limits.
    /// Opaque content is written as raw JSON, never converted through binary64.
    pub fn to_json(&self) -> Result<JsonValue, InspectionError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Encode with explicit byte, node and depth admission limits. The byte
    /// budget bounds emitted storage; it is not a bound on existing result owners.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, InspectionError> {
        self.validate()?;
        struct Output {
            bytes: Vec<u8>,
            max: usize,
        }
        impl std::io::Write for Output {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                    return Err(std::io::Error::other("inspection JSON byte limit exceeded"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let encoding = |message: String| {
            InspectionError::new(
                InspectionErrorKind::Encoding,
                "openbindings.inspection.encoding",
                message,
            )
        };
        let mut output = Output {
            bytes: Vec::new(),
            max: limits.max_bytes,
        };
        serde_json_compat::to_writer(&mut output, &InspectionFields(self))
            .map_err(|error| encoding(error.to_string()))?;
        JsonValue::parse_with_limits(output.bytes, limits)
            .map_err(|error| encoding(error.to_string()))
    }

    /// A complete inventory, including the valid empty case.
    pub fn complete(targets: Vec<BindableTarget>) -> Self {
        Self {
            targets,
            exhaustive: true,
            limitation: None,
        }
    }
    /// An incomplete inventory with explicit evidence.
    pub fn partial(targets: Vec<BindableTarget>, limitation: InspectionLimitation) -> Self {
        Self {
            targets,
            exhaustive: false,
            limitation: Some(limitation),
        }
    }
    /// Check role structure without claiming to prove source correspondence,
    /// inventory completeness, or the semantic soundness of operation framing.
    pub fn validate(&self) -> Result<(), InspectionError> {
        if !self.exhaustive && self.limitation.is_none() {
            return Err(InspectionError::result(
                "incomplete inspection requires a limitation",
            ));
        }
        if let Some(limitation) = &self.limitation
            && (limitation.code.is_empty() || limitation.message.is_empty())
        {
            return Err(InspectionError::result(
                "limitation code and message must be nonempty",
            ));
        }
        let mut identities = HashSet::with_capacity(self.targets.len());
        for (index, target) in self.targets.iter().enumerate() {
            target.validate().map_err(|error| {
                InspectionError::result(format!("target {index}: {}", error.message()))
            })?;
            if !identities.insert(target.source_ref.as_str()) {
                return Err(InspectionError::result(format!(
                    "target {index}: duplicate sourceRef {:?}",
                    target.source_ref
                )));
            }
        }
        Ok(())
    }
}

impl BindableTarget {
    fn validate(&self) -> Result<(), InspectionError> {
        if self.source_ref.is_empty() {
            return Err(InspectionError::result("requires a nonempty sourceRef"));
        }
        self.binding.validate()?;
        if let Some(operation) = &self.operation {
            for key in operation.additional_fields.keys() {
                if [
                    "idempotent",
                    "description",
                    "deprecated",
                    "tags",
                    "aliases",
                    "input",
                    "output",
                    "examples",
                ]
                .contains(&key.as_str())
                {
                    return Err(InspectionError::result(format!(
                        "operation framing has forbidden or shadowed member {key:?}"
                    )));
                }
            }
            if let Some(examples) = &operation.examples {
                for (name, example) in examples {
                    if let Some(key) = example
                        .additional_fields
                        .keys()
                        .find(|key| ["description", "input", "output"].contains(&key.as_str()))
                    {
                        return Err(InspectionError::result(format!(
                            "operation example {name:?} shadows typed member {key:?}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Serialize for BindableTarget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        TargetFields(self).serialize(serializer)
    }
}
impl Serialize for SourceInspection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        InspectionFields(self).serialize(serializer)
    }
}

// Private wire views are used only after validation. They share one field mapping
// across public Serde and bounded exact encoding without rechecking the full
// inventory or allocating a second target collection during serialization.
struct TargetFields<'a>(&'a BindableTarget);
impl Serialize for TargetFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let target = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("sourceRef", &target.source_ref)?;
        map.serialize_entry("binding", &target.binding)?;
        if let Some(value) = &target.operation_key {
            map.serialize_entry("operationKey", value)?;
        }
        if let Some(value) = &target.operation {
            map.serialize_entry("operation", value)?;
        }
        map.end()
    }
}
struct TargetFieldsList<'a>(&'a [BindableTarget]);
impl Serialize for TargetFieldsList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for target in self.0 {
            sequence.serialize_element(&TargetFields(target))?;
        }
        sequence.end()
    }
}
struct InspectionFields<'a>(&'a SourceInspection);
impl Serialize for InspectionFields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("targets", &TargetFieldsList(&self.0.targets))?;
        map.serialize_entry("exhaustive", &self.0.exhaustive)?;
        if let Some(value) = &self.0.limitation {
            map.serialize_entry("limitation", value)?;
        }
        map.end()
    }
}

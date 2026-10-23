use crate::{InspectionError, InspectionErrorKind};
use openbindings::{JsonLimits, JsonValue, Operation};
use openbindings_internal_interfaces::{BindingValue, carrier::BindingFields};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use std::collections::{BTreeMap, HashSet};

/// One target admitted by the source's kind, with optional operation framing.
///
/// Providers build one with [`Self::new`], [`Self::with_operation_key`] and
/// [`Self::with_operation`]. Later versions may add fields: read them by name
/// and match with `..`.
/// ```compile_fail,E0639
/// use openbindings_source_inspection::{BindableTarget, BindingValue};
/// let _ = BindableTarget { source_ref: "value".into(), binding: BindingValue::default(),
///     operation_key: None, operation: None };
/// ```
/// ```
/// use openbindings_source_inspection::{BindableTarget, BindingValue};
/// let target = BindableTarget::new("value", BindingValue::default()).with_operation_key("get");
/// let BindableTarget { source_ref, binding, operation_key, operation, .. } = target;
/// assert_eq!(operation_key.as_deref(), Some("get"));
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct BindableTarget {
    /// Stable source-local identity. This SDK requires uniqueness within one
    /// inspection by exact string equality. Cross-provider equality exists only if the
    /// kind defines it; synthesis in this same provider must use the same identity.
    pub source_ref: String,
    /// The target's binding by value, without the author-owned `operation` and
    /// `source` keys, which the author assigns with [`BindingValue::bind`]. A
    /// binding invoker accepts it as it is. Only source-evidenced idempotency
    /// claims belong here; do not infer a retry policy.
    pub binding: BindingValue,
    /// Optional suggestion; the author owns the actual operation key.
    pub operation_key: Option<String>,
    /// Optional sound projection of this target. It must not carry `idempotent`.
    /// This is authoring data, not an already conformant operation contract.
    pub operation: Option<Operation>,
}
impl std::fmt::Debug for BindableTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BindableTarget")
            .field("binding", &self.binding)
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}

/// Evidence explaining omitted or unestablished admitted targets.
///
/// Providers build one with [`Self::new`], [`Self::with_details`] and
/// [`Self::with_additional_fields`]. Later versions may add fields: read them by
/// name and match with `..`.
#[derive(Clone)]
#[non_exhaustive]
pub struct InspectionLimitation {
    /// Nonempty, stable namespaced reason code.
    pub code: String,
    /// Nonempty explanation of what may be missing and why.
    pub message: String,
    /// Optional structured evidence; exact values are preserved.
    pub details: Option<BTreeMap<String, JsonValue>>,
    /// Additional evidence members permitted by Source Inspector 0.1. Typed
    /// members (`code`, `message`, `details`) must not be shadowed.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl std::fmt::Debug for InspectionLimitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InspectionLimitation")
            .finish_non_exhaustive()
    }
}

impl InspectionLimitation {
    /// Evidence with a stable namespaced code and an explanation, both nonempty.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            additional_fields: BTreeMap::new(),
        }
    }
    /// Attach structured evidence; exact values are preserved.
    #[must_use]
    pub fn with_details(mut self, details: BTreeMap<String, JsonValue>) -> Self {
        self.details = Some(details);
        self
    }
    /// Attach additional evidence members; they must not shadow `code`,
    /// `message` or `details`.
    #[must_use]
    pub fn with_additional_fields(mut self, fields: BTreeMap<String, JsonValue>) -> Self {
        self.additional_fields = fields;
        self
    }
    fn validate(&self) -> Result<(), InspectionError> {
        if self.code.is_empty() || self.message.is_empty() {
            return Err(InspectionError::result(
                "limitation code and message must be nonempty",
            ));
        }
        if let Some(key) = self
            .additional_fields
            .keys()
            .find(|key| ["code", "message", "details"].contains(&key.as_str()))
        {
            return Err(InspectionError::result(format!(
                "limitation shadows typed member {key:?}"
            )));
        }
        Ok(())
    }
}

impl Serialize for InspectionLimitation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("code", &self.code)?;
        map.serialize_entry("message", &self.message)?;
        if let Some(details) = &self.details {
            map.serialize_entry("details", details)?;
        }
        for (key, value) in &self.additional_fields {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// An inventory of targets the kind admits, in stable provider-defined order.
///
/// Complete empty inventories are valid. Excluded upstream interactions are not
/// targets. An admitted target that cannot be established or represented requires
/// `exhaustive: false` and a limitation. An unreadable source is a refusal instead.
///
/// Providers build one with [`Self::complete`] or [`Self::partial`], and
/// [`Self::with_limitation`]. Later versions may add fields: read them by name
/// and match with `..`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SourceInspection {
    /// Bindable targets; this inventory does not silently apply a relevance filter.
    pub targets: Vec<BindableTarget>,
    /// Whether every admitted target was reported.
    pub exhaustive: bool,
    /// Required for an incomplete inventory. The interface also permits evidence on a
    /// complete result; consumers use `exhaustive` as the completeness signal.
    pub limitation: Option<InspectionLimitation>,
}

impl SourceInspection {
    /// Encode a checked inventory as exact JSON with the core's default limits.
    /// Opaque content is written as raw JSON, never converted through binary64.
    ///
    /// # Errors
    /// Returns an inspection error if the inventory fails carrier validation, cannot be serialized, or its encoded JSON exceeds the admission limits.
    pub fn to_json(&self) -> Result<JsonValue, InspectionError> {
        self.to_json_with_limits(JsonLimits::default())
    }

    /// Encode with explicit byte, node and depth admission limits. The byte
    /// budget bounds emitted storage; it is not a bound on existing result owners.
    ///
    /// # Errors
    /// Returns an inspection error if the inventory fails carrier validation, cannot be serialized, or its encoded JSON exceeds the admission limits.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, InspectionError> {
        self.validate()?;
        // Invocation and synthesis keep the same bounded writer for their own errors.
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
        serde_json::to_writer(&mut output, &InspectionFields(self))
            .map_err(|error| encoding(error.to_string()))?;
        JsonValue::parse_owned_with_limits(output.bytes, limits)
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
    /// Attach evidence, keeping completeness unchanged. The interface also
    /// permits evidence on a complete inventory.
    #[must_use]
    pub fn with_limitation(mut self, limitation: InspectionLimitation) -> Self {
        self.limitation = Some(limitation);
        self
    }
    /// Check the SDK's result structure and policy, including duplicate rejection.
    /// This is stricter than the shared interface and does not prove source
    /// correspondence, inventory completeness, or sound operation framing.
    ///
    /// # Errors
    /// Returns an inspection error if targets, limitations, or additional fields violate the inspection carrier requirements.
    pub fn validate(&self) -> Result<(), InspectionError> {
        if !self.exhaustive && self.limitation.is_none() {
            return Err(InspectionError::result(
                "incomplete inspection requires a limitation",
            ));
        }
        if let Some(limitation) = &self.limitation {
            limitation.validate()?;
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
    /// A target with its source-local identity and binding data, without
    /// operation framing.
    pub fn new(source_ref: impl Into<String>, binding: BindingValue) -> Self {
        Self {
            source_ref: source_ref.into(),
            binding,
            operation_key: None,
            operation: None,
        }
    }
    /// Suggest an operation key; the author owns the actual key.
    #[must_use]
    pub fn with_operation_key(mut self, operation_key: impl Into<String>) -> Self {
        self.operation_key = Some(operation_key.into());
        self
    }
    /// Frame the target as an operation. It must not carry `idempotent`.
    #[must_use]
    pub fn with_operation(mut self, operation: Operation) -> Self {
        self.operation = Some(operation);
        self
    }
    fn validate(&self) -> Result<(), InspectionError> {
        if self.source_ref.is_empty() {
            return Err(InspectionError::result("requires a nonempty sourceRef"));
        }
        let author_keys = [
            ("operation", self.binding.operation.is_some()),
            ("source", self.binding.source.is_some()),
        ];
        if let Some((key, _)) = author_keys.into_iter().find(|(_, present)| *present) {
            return Err(InspectionError::result(format!(
                "binding carries the author-owned {key:?} key"
            )));
        }
        if let Some(key) = self
            .binding
            .extensions
            .keys()
            .find(|key| !key.starts_with("x-"))
        {
            return Err(InspectionError::result(format!(
                "binding member {key:?} is not an x- extension"
            )));
        }
        if let Some(operation) = &self.operation {
            for key in operation.additional_fields.keys() {
                if !key.starts_with("x-") {
                    return Err(InspectionError::result(format!(
                        "operation framing member {key:?} is not an x- extension"
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
        map.serialize_entry("binding", &BindingFields(&target.binding))?;
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

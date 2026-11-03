//! Closed operation opening payload with one explicit addressing form.
use crate::{
    BoundaryFailure,
    boundary::{BoundaryFailureKind, encode, failure, nonempty, object, string},
    value,
};
use openbindings::{ConformantDocument, JsonLimits, JsonRef, JsonValue};
use openbindings_context::Context;
use serde::{Serialize, Serializer, ser::SerializeMap};

/// One operation address. Binding selection is caller policy, never context.
#[derive(Clone)]
pub enum OperationAddress {
    /// A primary operation name or alias and optional caller-ordered binding keys.
    Operation {
        /// Exact name in the document's flat operation key-and-alias namespace.
        name: String,
        /// If present, a nonempty ordered list with no implicit fallback.
        selection: Option<Vec<String>>,
    },
    /// An explicit binding; derive its operation without selecting alternatives.
    Binding(String),
}
/// An immutable original interface, address and opaque attempt context.
#[derive(Clone)]
pub struct OperationRequest {
    /// Complete original conformant interface snapshot.
    pub interface: ConformantDocument,
    /// Exactly one addressing form.
    pub address: OperationAddress,
    /// Optional opaque context, unchanged when forwarded to the provider.
    pub context: Option<Context>,
}
impl OperationRequest {
    /// Address an operation name or alias without choosing among its bindings.
    pub fn operation(interface: ConformantDocument, name: impl Into<String>) -> Self {
        Self {
            interface,
            address: OperationAddress::Operation {
                name: name.into(),
                selection: None,
            },
            context: None,
        }
    }
    /// Address one binding directly, deriving its operation for value validation.
    pub fn binding(interface: ConformantDocument, binding: impl Into<String>) -> Self {
        Self {
            interface,
            address: OperationAddress::Binding(binding.into()),
            context: None,
        }
    }
    /// Address an operation with an explicit ordered binding selection.
    pub fn ordered(
        interface: ConformantDocument,
        name: impl Into<String>,
        selection: Vec<String>,
    ) -> Self {
        Self {
            interface,
            address: OperationAddress::Operation {
                name: name.into(),
                selection: Some(selection),
            },
            context: None,
        }
    }
    /// Supply context only for this request.
    #[must_use]
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub(crate) fn admit(&self) -> Result<(), BoundaryFailure> {
        match &self.address {
            OperationAddress::Binding(key) => nonempty(key, "/binding"),
            OperationAddress::Operation { name, selection } => {
                nonempty(name, "/operation")?;
                if let Some(keys) = selection {
                    if keys.is_empty() {
                        return Err(failure(BoundaryFailureKind::EmptyArray, "/selection"));
                    }
                    for key in keys {
                        nonempty(key, "/selection")
                            .map_err(|_| failure(BoundaryFailureKind::EmptyString, "/selection"))?;
                    }
                }
                Ok(())
            }
        }
    }
    /// Admit an exact closed opening payload and prove document conformance.
    /// # Errors
    /// Rejects malformed/ambiguous outer fields, addressing or document admission.
    pub fn from_json(value: &JsonValue) -> Result<Self, BoundaryFailure> {
        Self::from_json_ref(value.view())
    }
    /// Admit a borrowed exact opening payload, retaining original JSON owners.
    /// # Errors
    /// Returns the same local failures as [`Self::from_json`].
    pub fn from_json_ref(value: JsonRef<'_>) -> Result<Self, BoundaryFailure> {
        object(
            value,
            &["interface", "operation", "binding", "selection", "context"],
            "",
        )?;
        let address = match (
            value.get("operation"),
            value.get("binding"),
            value.get("selection"),
        ) {
            (Some(name), None, selection) => {
                let name = string(name, "/operation")?.to_owned();
                let selection = selection
                    .map(|keys| {
                        keys.elements()
                            .ok_or_else(|| {
                                failure(BoundaryFailureKind::ExpectedArray, "/selection")
                            })?
                            .map(|key| string(key, "/selection").map(str::to_owned))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?;
                OperationAddress::Operation { name, selection }
            }
            (None, Some(key), None) => {
                OperationAddress::Binding(string(key, "/binding")?.to_owned())
            }
            _ => return Err(failure(BoundaryFailureKind::InvalidAddress, "")),
        };
        let request = Self {
            interface: value::interface(value)?,
            address,
            context: value::context(value)?,
        };
        request.admit()?;
        Ok(request)
    }
    /// Encode the opening payload with default exact-JSON limits.
    /// # Errors
    /// Rejects malformed addressing or a local encoding resource limit.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryFailure> {
        self.to_json_with_limits(JsonLimits::default())
    }
    /// Encode under explicit bounds without copying into lossy value carriers.
    /// # Errors
    /// Rejects malformed addressing or a local encoding resource limit.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryFailure> {
        self.admit()?;
        encode(&Fields(self), limits)
    }
}
impl std::fmt::Debug for OperationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationRequest").finish_non_exhaustive()
    }
}
struct Fields<'a>(&'a OperationRequest);
impl Serialize for Fields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let request = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("interface", &request.interface.value().to_owned())?;
        match &request.address {
            OperationAddress::Binding(key) => map.serialize_entry("binding", key)?,
            OperationAddress::Operation { name, selection } => {
                map.serialize_entry("operation", name)?;
                if let Some(selection) = selection {
                    map.serialize_entry("selection", selection)?;
                }
            }
        }
        if let Some(context) = &request.context {
            map.serialize_entry("context", context.value().value())?;
        }
        map.end()
    }
}

impl std::fmt::Debug for OperationAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binding(_) => f.write_str("Binding { .. }"),
            Self::Operation { selection, .. } => f
                .debug_struct("Operation")
                .field("selection_present", &selection.is_some())
                .finish_non_exhaustive(),
        }
    }
}

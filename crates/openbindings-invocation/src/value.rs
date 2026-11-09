//! Owned original-document requests; no source/binding copying or slicing.
use crate::boundary::{
    BoundaryFailure, BoundaryFailureKind, encode, failure, nonempty, object, required, string,
};
use openbindings::{ConformantDocument, Document, JsonLimits, JsonRef, JsonValue};
use openbindings_context::Context;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::fmt;

/// One binding of an immutable conformant interface, with optional opaque context.
/// The provider can retain the complete original snapshot and resolve kind-defined
/// references from it. No operation declaration is implicitly read on its behalf.
#[derive(Clone)]
pub struct BindingRequest {
    /// Complete original interface snapshot, supplied inline by value.
    pub interface: ConformantDocument,
    /// The exact binding key; selection is already complete.
    pub binding: String,
    /// Optional attempt context. Absence remains distinct from an empty object.
    pub context: Option<Context>,
}
impl BindingRequest {
    /// Retain the original document and the explicitly selected binding key.
    pub fn new(interface: ConformantDocument, binding: impl Into<String>) -> Self {
        Self {
            interface,
            binding: binding.into(),
            context: None,
        }
    }
    /// Supply opaque context for this attempt without interpreting its members.
    #[must_use = "use the returned request to retain its context"]
    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }
    pub(crate) fn admit(&self) -> Result<(), BoundaryFailure> {
        nonempty(&self.binding, "/binding")
    }
    /// Read the closed wire opening payload and require current OBI conformance.
    /// A well-shaped missing binding key remains an invocation terminal, not an
    /// admission failure. The context object remains exact and opaque.
    ///
    /// For a borrowed frame, pass `&frame.to_owned()`; this retains its shared
    /// exact storage with a reference-count clone.
    ///
    /// # Errors
    /// Rejects malformed fields, ambiguous outer names, or a document for which
    /// the core cannot establish conformance. It does not acquire external data.
    pub fn from_json(value: &JsonValue) -> Result<Self, BoundaryFailure> {
        Self::from_json_ref(value.view())
    }
    /// Read an exact opening payload while retaining its original owners.
    ///
    /// # Errors
    /// Returns the same local admission failures as [`Self::from_json`].
    fn from_json_ref(value: JsonRef<'_>) -> Result<Self, BoundaryFailure> {
        object(value, &["interface", "binding", "context"], "")?;
        let interface = interface(value)?;
        let binding = string(required(value, "binding", "/binding")?, "/binding")?.to_owned();
        let context = context(value)?;
        let request = Self {
            interface,
            binding,
            context,
        };
        request.admit()?;
        Ok(request)
    }
    /// Encode the current opening payload under default exact-JSON bounds.
    ///
    /// # Errors
    /// Rejects an empty binding key or an encoding exceeding its resource bounds.
    pub fn to_json(&self) -> Result<JsonValue, BoundaryFailure> {
        self.to_json_with_limits(JsonLimits::default())
    }
    /// Encode under explicit bounds, preserving original interface/context values.
    ///
    /// # Errors
    /// Rejects an empty binding key or an encoding exceeding its resource bounds.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, BoundaryFailure> {
        self.admit()?;
        encode(&Fields(self), limits)
    }
}
impl fmt::Debug for BindingRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindingRequest")
            .field("context_present", &self.context.is_some())
            .finish_non_exhaustive()
    }
}
impl Serialize for BindingRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.admit().map_err(serde::ser::Error::custom)?;
        Fields(self).serialize(serializer)
    }
}
struct Fields<'a>(&'a BindingRequest);
impl Serialize for Fields<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2 + usize::from(self.0.context.is_some())))?;
        map.serialize_entry("interface", &self.0.interface.value().to_owned())?;
        map.serialize_entry("binding", &self.0.binding)?;
        if let Some(context) = &self.0.context {
            map.serialize_entry("context", context.value().value())?;
        }
        map.end()
    }
}

pub(crate) fn interface(value: JsonRef<'_>) -> Result<ConformantDocument, BoundaryFailure> {
    let document = Document::from_json(required(value, "interface", "/interface")?.to_owned());
    document
        .validate()
        .map_err(|_| failure(BoundaryFailureKind::InvalidDocument, "/interface"))?
        .conformant()
        .ok_or_else(|| failure(BoundaryFailureKind::InvalidDocument, "/interface"))
}
pub(crate) fn context(value: JsonRef<'_>) -> Result<Option<Context>, BoundaryFailure> {
    value
        .get("context")
        .map(|value| {
            Context::try_from(value.to_owned())
                .map_err(|_| failure(BoundaryFailureKind::ExpectedObject, "/context"))
        })
        .transpose()
}

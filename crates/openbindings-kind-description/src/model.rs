use openbindings::{JsonKind, JsonLimits, JsonObject, JsonRef, JsonStringRef, JsonValue, Pointer};
use openbindings_context::{Context, ContextShapeError};
use std::{collections::BTreeSet, fmt};

/// A carrier field or local carrier relationship is invalid.
/// Diagnostics never include provider data, descriptions, context or kind names.
#[derive(Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CarrierErrorKind {
    /// An object-form carrier was required.
    ExpectedObject,
    /// An outer decoded member name occurs more than once, or an object anywhere
    /// in a request's context repeats a decoded member name.
    DuplicateMember,
    /// A closed carrier contains a member it does not define.
    UnexpectedMember,
    /// A required published field is absent. A name several carriers share is
    /// labelled with its carrier, as `check/description`.
    /// Labels are available for inspection but omitted from Display and Debug.
    MissingField(&'static str),
    /// A published field has the wrong JSON type, labelled the same way.
    FieldType(&'static str),
    /// A required nonempty string is empty, labelled the same way.
    EmptyString(&'static str),
    /// Encoding a typed request exceeds the default exact-JSON admission limits.
    InputLimit,
    /// The operation-key support list repeats a key.
    DuplicateSupport,
    /// CONTEXT_REQUIRED must carry a validated context challenge, not a rejection.
    ReservedCode,
    /// The check disclosure contradicts the explicit support claims.
    InconsistentCheckSupport,
    /// The pull disclosure contradicts the explicit support claim.
    InconsistentPullSupport,
    /// The invocation disclosure contradicts the explicit support claim.
    InconsistentInvokeSupport,
}
impl fmt::Debug for CarrierErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExpectedObject => "ExpectedObject",
            Self::DuplicateMember => "DuplicateMember",
            Self::UnexpectedMember => "UnexpectedMember",
            Self::MissingField(_) => "MissingField { .. }",
            Self::FieldType(_) => "FieldType { .. }",
            Self::EmptyString(_) => "EmptyString { .. }",
            Self::InputLimit => "InputLimit",
            Self::DuplicateSupport => "DuplicateSupport",
            Self::ReservedCode => "ReservedCode",
            Self::InconsistentCheckSupport => "InconsistentCheckSupport",
            Self::InconsistentPullSupport => "InconsistentPullSupport",
            Self::InconsistentInvokeSupport => "InconsistentInvokeSupport",
        })
    }
}
impl fmt::Display for CarrierErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedObject => f.write_str("expected an object carrier"),
            Self::DuplicateMember => f.write_str("a carrier member is repeated"),
            Self::UnexpectedMember => f.write_str("the carrier contains an unexpected member"),
            Self::MissingField(_) => f.write_str("a required carrier field is absent"),
            Self::FieldType(_) => f.write_str("a carrier field has an invalid value"),
            Self::EmptyString(_) => f.write_str("a carrier field must not be empty"),
            Self::InputLimit => f.write_str("the request exceeds exact JSON admission limits"),
            Self::DuplicateSupport => f.write_str("the support list repeats an operation key"),
            Self::ReservedCode => f.write_str("the reserved code requires a context challenge"),
            Self::InconsistentPullSupport => {
                f.write_str("pull disclosure contradicts support claims")
            }
            Self::InconsistentInvokeSupport => {
                f.write_str("invoke disclosure contradicts support claims")
            }
            Self::InconsistentCheckSupport => {
                f.write_str("check disclosure contradicts support claims")
            }
        }
    }
}
/// Invalid local carrier shape, with a location when the decoder knows it.
/// Locations contain only fixed carrier fields and numeric support indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarrierError {
    kind: CarrierErrorKind,
    pointer: Option<Pointer>,
}
impl CarrierError {
    /// Read the Copy category independently of the optional location.
    pub fn kind(&self) -> CarrierErrorKind {
        self.kind
    }
    /// Borrow the failing location relative to the admitted carrier.
    /// Ambiguous member names locate their containing object; context failures
    /// locate `/context` without repeating any opaque member name.
    pub fn pointer(&self) -> Option<&Pointer> {
        self.pointer.as_ref()
    }
    fn in_field(mut self, field: &'static str) -> Self {
        if let Some(pointer) = &self.pointer {
            self.pointer = Some(
                format!("/{field}{}", pointer.as_str().expect("fixed pointer"))
                    .parse()
                    .expect("fixed nested pointer"),
            );
        }
        self
    }
}
impl From<CarrierErrorKind> for CarrierError {
    fn from(kind: CarrierErrorKind) -> Self {
        Self {
            kind,
            pointer: None,
        }
    }
}
impl CarrierErrorKind {
    pub(crate) fn at_field(self, field: &'static str) -> CarrierError {
        let field = field.rsplit('/').next().expect("fixed field");
        CarrierError {
            kind: self,
            pointer: Some(format!("/{field}").parse().expect("fixed field pointer")),
        }
    }
    fn at_object(self) -> CarrierError {
        CarrierError {
            kind: self,
            pointer: Some("".parse().expect("root pointer")),
        }
    }
}
impl fmt::Display for CarrierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}
impl std::error::Error for CarrierError {}

pub(crate) fn object(
    value: JsonValue,
    allowed: Option<(&[&str], bool)>,
) -> Result<JsonObject, CarrierError> {
    let object = value
        .as_object()
        .map_err(|_| CarrierErrorKind::ExpectedObject.at_object())?;
    let mut names = BTreeSet::new();
    for member in value.view().members().expect("object") {
        let name = member.name.as_string().expect("member name");
        if !names.insert(name.utf16_units().collect::<Vec<_>>()) {
            return Err(CarrierErrorKind::DuplicateMember.at_object());
        }
        if let Some((fields, extensions)) = allowed
            && !(name.as_str().is_some_and(|name| fields.contains(&name))
                || (extensions && name.utf16_units().take(2).eq([b'x' as u16, b'-' as u16])))
        {
            return Err(CarrierErrorKind::UnexpectedMember.at_object());
        }
    }
    Ok(object)
}
pub(crate) fn field<'a>(
    object: &'a JsonObject,
    name: &'static str,
) -> Result<JsonRef<'a>, CarrierError> {
    object
        .value()
        .get(name)
        .ok_or_else(|| CarrierErrorKind::MissingField(name).at_field(name))
}
pub(crate) fn string<'a>(
    value: JsonRef<'a>,
    name: &'static str,
    nonempty: bool,
) -> Result<JsonStringRef<'a>, CarrierError> {
    let text = value
        .as_string()
        .ok_or_else(|| CarrierErrorKind::FieldType(name).at_field(name))?;
    if nonempty && text.utf16_units().next().is_none() {
        return Err(CarrierErrorKind::EmptyString(name).at_field(name));
    }
    Ok(text)
}
// Check, pull and invoke each sit at one place in a kind description, and share
// their field names, so their errors name that place.
fn disclosure(
    value: JsonValue,
    description: &'static str,
    executes: &'static str,
) -> Result<JsonObject, CarrierError> {
    let object = object(value, None)?;
    let text = object.value().get("description");
    string(
        text.ok_or_else(|| CarrierErrorKind::MissingField(description).at_field(description))?,
        description,
        true,
    )?;
    let flag = object.value().get("executes");
    flag.ok_or_else(|| CarrierErrorKind::MissingField(executes).at_field(executes))?
        .as_bool()
        .ok_or_else(|| CarrierErrorKind::FieldType(executes).at_field(executes))?;
    Ok(object)
}
fn optional_string(object: &JsonObject, name: &'static str) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get(name) {
        string(value, name, false)?;
    }
    Ok(())
}
fn kind(object: &JsonObject) -> Result<(), CarrierError> {
    string(field(object, "kind")?, "kind", true).map(|_| ())
}
fn supports(object: &JsonObject) -> Result<(), CarrierError> {
    let values = field(object, "supports")?;
    let values = values
        .elements()
        .ok_or_else(|| CarrierErrorKind::FieldType("supports").at_field("supports"))?;
    let mut seen = BTreeSet::new();
    for (index, value) in values.enumerate() {
        let at_index = |mut error: CarrierError| {
            error.pointer = Some(
                format!("/supports/{index}")
                    .parse()
                    .expect("support pointer"),
            );
            error
        };
        let name = string(value, "supports", true).map_err(at_index)?;
        if !seen.insert(name.utf16_units().collect::<Vec<_>>()) {
            return Err(at_index(
                CarrierErrorKind::DuplicateSupport.at_field("supports"),
            ));
        }
    }
    Ok(())
}
fn schema(object: &JsonObject, name: &'static str) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get(name)
        && !matches!(value.kind(), JsonKind::Object | JsonKind::Boolean)
    {
        return Err(CarrierErrorKind::FieldType(name).at_field(name));
    }
    Ok(())
}

macro_rules! carrier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone)]
        pub struct $name(JsonObject);
        impl $name {
            /// Borrow the exact admitted object, including allowed output members.
            pub fn value(&self) -> &JsonObject {
                &self.0
            }
        }
        impl TryFrom<JsonValue> for $name {
            type Error = CarrierError;
            fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
                Self::from_json(value)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    };
}
macro_rules! kind_accessor {
    () => {
        /// Borrow the provider's exact kind string without normalization.
        pub fn kind(&self) -> JsonStringRef<'_> {
            self.0
                .value()
                .get("kind")
                .and_then(|value| value.as_string())
                .expect("admitted kind")
        }
    };
}
macro_rules! supports_accessor {
    () => {
        /// Iterate the distinct claimed operation keys in their authored order.
        pub fn supports(&self) -> impl ExactSizeIterator<Item = JsonStringRef<'_>> {
            self.0
                .value()
                .get("supports")
                .expect("admitted supports")
                .elements()
                .expect("array")
                .map(|value| value.as_string().expect("admitted operation key"))
        }
    };
}
carrier!(
    KindSummary,
    "One kind and the operation keys this implementation claims to perform under it."
);
impl KindSummary {
    /// Admit the open listing-entry carrier and its distinct operation-key claims.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = object(value, None)?;
        kind(&object)?;
        supports(&object)?;
        optional_string(&object, "description")?;
        Ok(Self(object))
    }
    kind_accessor!();
    supports_accessor!();
    /// Borrow an optional description, retaining absence and an empty string distinctly.
    pub fn description(&self) -> Option<JsonStringRef<'_>> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
    }
}
carrier!(
    CheckDescription,
    "A check's human-readable disclosure and whether it executes something the source names."
);
impl CheckDescription {
    /// Admit the open check-disclosure carrier; this does not grant permission to execute.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        disclosure(value, "check/description", "check/executes").map(Self)
    }
    /// Borrow the exact nonempty disclosure.
    pub fn description(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
            .expect("admitted description")
    }
    /// Whether the implementation says its check executes something the source data names.
    pub fn executes(&self) -> bool {
        self.0
            .value()
            .get("executes")
            .and_then(|value| value.as_bool())
            .expect("admitted executes")
    }
}
carrier!(
    PullDescription,
    "A pull's human-readable disclosure and whether it executes something the source names."
);
impl PullDescription {
    /// Admit the open pull-disclosure carrier; this does not grant permission to execute.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        disclosure(value, "pull/description", "pull/executes").map(Self)
    }
    /// Borrow the exact nonempty disclosure.
    pub fn description(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
            .expect("admitted description")
    }
    /// Whether the implementation says its pull executes something the source data names.
    pub fn executes(&self) -> bool {
        self.0
            .value()
            .get("executes")
            .and_then(|value| value.as_bool())
            .expect("admitted executes")
    }
}
carrier!(
    InvokeDescription,
    "An invocation's human-readable disclosure and whether it executes something the source names."
);
impl InvokeDescription {
    /// Admit the open invocation-disclosure carrier; this does not grant permission to execute.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        disclosure(value, "invoke/description", "invoke/executes").map(Self)
    }
    /// Borrow the exact nonempty disclosure.
    pub fn description(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
            .expect("admitted description")
    }
    /// Whether the implementation says its invocation executes something the source or binding data names.
    pub fn executes(&self) -> bool {
        self.0
            .value()
            .get("executes")
            .and_then(|value| value.as_bool())
            .expect("admitted executes")
    }
}
carrier!(
    KindDescription,
    "A kind's description, per-operation claims, operation disclosures and two data schemas."
);
impl KindDescription {
    /// Admit the open description carrier without evaluating either data schema.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = object(value, None)?;
        kind(&object)?;
        supports(&object)?;
        string(field(&object, "description")?, "description", true)?;
        schema(&object, "sourceData")?;
        schema(&object, "bindingData")?;
        if let Some(check) = object.value().get("check") {
            CheckDescription::from_json(check.to_owned())
                .map_err(|error| error.in_field("check"))?;
        }
        let operations = field(&object, "supports")?;
        let declares_check = operations
            .elements()
            .expect("admitted supports")
            .any(|value| value.as_str() == Some("openbindings.kind-descriptor.checkSource"));
        let has_check = object.value().get("check").is_some();
        if declares_check != has_check {
            return Err(CarrierErrorKind::InconsistentCheckSupport.at_field("check"));
        }
        for (name, operation, error) in [
            (
                "pull",
                "openbindings.source-puller.pullSource",
                CarrierErrorKind::InconsistentPullSupport,
            ),
            (
                "invoke",
                "openbindings.binding-invoker.invokeBinding",
                CarrierErrorKind::InconsistentInvokeSupport,
            ),
        ] {
            let disclosure = object.value().get(name);
            let supported = operations
                .elements()
                .expect("admitted supports")
                .any(|value| value.as_str() == Some(operation));
            if supported != disclosure.is_some() {
                return Err(error.at_field(name));
            }
            if let Some(value) = disclosure {
                match name {
                    "pull" => {
                        PullDescription::from_json(value.to_owned())
                            .map_err(|error| error.in_field("pull"))?;
                    }
                    _ => {
                        InvokeDescription::from_json(value.to_owned())
                            .map_err(|error| error.in_field("invoke"))?;
                    }
                }
            }
        }
        Ok(Self(object))
    }
    kind_accessor!();
    supports_accessor!();
    /// Borrow the exact nonempty kind description.
    pub fn description(&self) -> JsonStringRef<'_> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
            .expect("admitted description")
    }
    /// Retain the optional check disclosure independently of this owner.
    pub fn check(&self) -> Option<CheckDescription> {
        self.0
            .value()
            .get("check")
            .map(|value| CheckDescription(value.to_owned().as_object().expect("admitted check")))
    }
    /// Retain the pull disclosure, present exactly when the kind claims pullSource.
    pub fn pull(&self) -> Option<PullDescription> {
        self.0
            .value()
            .get("pull")
            .map(|value| PullDescription(value.to_owned().as_object().expect("admitted pull")))
    }
    /// Retain the call disclosure, present exactly when the kind claims invokeBinding.
    pub fn invoke(&self) -> Option<InvokeDescription> {
        self.0
            .value()
            .get("invoke")
            .map(|value| InvokeDescription(value.to_owned().as_object().expect("admitted invoke")))
    }
    /// Retain the optional exact source-data schema. Absence is not an empty schema.
    pub fn source_data(&self) -> Option<JsonValue> {
        self.0
            .value()
            .get("sourceData")
            .map(|value| value.to_owned())
    }
    /// Retain the optional exact binding-data schema. This is not a core Schema proof.
    pub fn binding_data(&self) -> Option<JsonValue> {
        self.0
            .value()
            .get("bindingData")
            .map(|value| value.to_owned())
    }
}
carrier!(
    DescribeKindRequest,
    "The closed request identifying one kind to describe."
);
impl DescribeKindRequest {
    /// Construct the closed request from a nonempty kind name.
    ///
    /// # Errors
    /// Rejects an empty kind or exceeded default exact-JSON admission limits.
    pub fn new(kind: impl Into<String>) -> Result<Self, CarrierError> {
        Self::from_json(request_value(kind.into(), None, None)?)
    }
    /// Admit exactly the required kind field.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = object(value, Some((&["kind"], false)))?;
        kind(&object)?;
        Ok(Self(object))
    }
    kind_accessor!();
}
carrier!(
    CheckSourceRequest,
    "A proposed source kind/data pair with separate optional runtime context."
);
impl CheckSourceRequest {
    /// Construct a request from a nonempty kind, optional exact data and opaque
    /// object context. Omission remains distinct from JSON null; nested values
    /// retain number spellings and unpaired UTF-16 units. Data may retain duplicate
    /// nested members; an admitted [`Context`] has none.
    ///
    /// # Errors
    /// Rejects an empty kind or exceeded default exact-JSON admission limits.
    pub fn new(
        kind: impl Into<String>,
        data: Option<JsonValue>,
        context: Option<Context>,
    ) -> Result<Self, CarrierError> {
        Self::from_json(request_value(kind.into(), data, context)?)
    }
    /// Admit kind, optional data and optional object context without interpreting them.
    ///
    /// # Errors
    /// Rejects carrier shape failures, including [`CarrierErrorKind::DuplicateMember`]
    /// for context in which any object, at any depth, repeats a decoded member name.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = object(value, Some((&["kind", "data", "context"], false)))?;
        kind(&object)?;
        if let Some(context) = object.value().get("context") {
            Context::try_from(context.to_owned()).map_err(|error| match error {
                ContextShapeError::DuplicateMember => {
                    CarrierErrorKind::DuplicateMember.at_field("context")
                }
                _ => CarrierErrorKind::FieldType("context").at_field("context"),
            })?;
        }
        Ok(Self(object))
    }
    kind_accessor!();
    /// Retain the exact proposed data, distinguishing omission from JSON null.
    pub fn data(&self) -> Option<JsonValue> {
        self.0.value().get("data").map(|value| value.to_owned())
    }
    /// Retain the caller's opaque context without merging or interpreting members.
    pub fn context(&self) -> Option<Context> {
        self.0
            .value()
            .get("context")
            .map(|value| Context::try_from(value.to_owned()).expect("admitted context"))
    }
}

fn request_value(
    kind: String,
    data: Option<JsonValue>,
    context: Option<Context>,
) -> Result<JsonValue, CarrierError> {
    if kind.is_empty() {
        return Err(CarrierErrorKind::EmptyString("kind").at_field("kind"));
    }
    let kind = JsonValue::string(&kind).map_err(|_| CarrierErrorKind::InputLimit)?;
    let fields = [
        ("kind", Some(kind)),
        ("data", data),
        (
            "context",
            context.map(|context| context.value().value().clone()),
        ),
    ];
    let limits = JsonLimits::default();
    // Each field includes a comma; the first uses the opening brace instead.
    // The initial byte accounts for the closing brace.
    let mut size = 1usize;
    for (name, value) in &fields {
        if let Some(value) = value {
            size = size
                .checked_add(name.len() + 4)
                .and_then(|size| size.checked_add(value.bytes().len()))
                .filter(|size| *size <= limits.max_bytes)
                .ok_or(CarrierErrorKind::InputLimit)?;
        }
    }
    let mut text = String::with_capacity(size);
    text.push('{');
    for (name, value) in &fields {
        if let Some(value) = value {
            if text.len() > 1 {
                text.push(',');
            }
            text.push('"');
            text.push_str(name);
            text.push_str("\":");
            text.push_str(value.text());
        }
    }
    text.push('}');
    JsonValue::parse_owned_with_limits(text.into_bytes(), limits)
        .map_err(|_| CarrierError::from(CarrierErrorKind::InputLimit))
}
carrier!(
    CheckedSource,
    "A source entry the kind stands behind; not a document-conformance or availability proof."
);
impl CheckedSource {
    /// Admit core source fields and x- extensions. Runtime context is not a source field.
    pub fn from_json(value: JsonValue) -> Result<Self, CarrierError> {
        let object = object(value, Some((&["kind", "data", "description"], true)))?;
        kind(&object)?;
        optional_string(&object, "description")?;
        Ok(Self(object))
    }
    kind_accessor!();
    /// Retain source data exactly; omitted data remains absent.
    pub fn data(&self) -> Option<JsonValue> {
        self.0.value().get("data").map(|value| value.to_owned())
    }
    /// Borrow the optional description derived by the kind.
    pub fn description(&self) -> Option<JsonStringRef<'_>> {
        self.0
            .value()
            .get("description")
            .and_then(|value| value.as_string())
    }
}

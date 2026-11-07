use openbindings::{JsonKind, JsonObject, JsonRef, JsonStringRef, JsonValue};
use std::{collections::BTreeSet, fmt};

/// Invalid local carrier shape or proposal authority, not an OBI finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CarrierError {
    /// An object carrier was required.
    ExpectedObject,
    /// The carrier repeats a decoded member name.
    DuplicateMember,
    /// A required published field is absent. A name several carriers share is
    /// labelled with its carrier, as `existing/message`.
    MissingField(&'static str),
    /// A published field has an invalid type or finite value, labelled the same way.
    FieldType(&'static str),
    /// A required nonempty string is empty, labelled the same way.
    EmptyString(&'static str),
    /// An existing entry names zero or multiple entry types.
    ExistingTarget,
    /// Status and its accompanying fields contradict each other.
    StatusFields,
    /// A proposal carries a known field outside the kind's edit authority.
    ProposalAuthority,
    /// A binding proposal carries none of data, input, output or remove.
    EmptyBindingProposal,
    /// A uses list repeats an exact label.
    DuplicateUse,
}
impl fmt::Display for CarrierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedObject => f.write_str("expected an object carrier"),
            Self::DuplicateMember => f.write_str("a carrier member is repeated"),
            Self::MissingField(field) => write!(f, "the carrier requires {field}"),
            Self::FieldType(field) => write!(f, "the carrier field {field} has an invalid value"),
            Self::EmptyString(field) => write!(f, "the carrier field {field} must not be empty"),
            Self::ExistingTarget => {
                f.write_str("an existing entry must name exactly one entry type")
            }
            Self::StatusFields => {
                f.write_str("the report status contradicts its accompanying fields")
            }
            Self::ProposalAuthority => {
                f.write_str("the proposal exceeds the kind's edit authority")
            }
            Self::EmptyBindingProposal => {
                f.write_str("a binding proposal requires data, input, output or remove")
            }
            Self::DuplicateUse => f.write_str("the uses list repeats a label"),
        }
    }
}
impl std::error::Error for CarrierError {}
pub(crate) fn object(value: JsonValue) -> Result<JsonObject, CarrierError> {
    let object = value
        .as_object()
        .map_err(|_| CarrierError::ExpectedObject)?;
    let mut names = BTreeSet::new();
    for member in value.view().members().expect("object") {
        if !names.insert(
            member
                .name
                .as_string()
                .expect("name")
                .utf16_units()
                .collect::<Vec<_>>(),
        ) {
            return Err(CarrierError::DuplicateMember);
        }
    }
    Ok(object)
}
pub(crate) fn field<'a>(
    object: &'a JsonObject,
    name: &'static str,
) -> Result<JsonRef<'a>, CarrierError> {
    labelled(object, name, name)
}
// Reports a name several carriers share under its carrier, as `existing/message`.
pub(crate) fn labelled<'a>(
    object: &'a JsonObject,
    name: &'static str,
    label: &'static str,
) -> Result<JsonRef<'a>, CarrierError> {
    object
        .value()
        .get(name)
        .ok_or(CarrierError::MissingField(label))
}
pub(crate) fn string<'a>(
    value: JsonRef<'a>,
    name: &'static str,
    nonempty: bool,
) -> Result<JsonStringRef<'a>, CarrierError> {
    let string = value.as_string().ok_or(CarrierError::FieldType(name))?;
    if nonempty && string.utf16_units().next().is_none() {
        return Err(CarrierError::EmptyString(name));
    }
    Ok(string)
}
pub(crate) fn optional_string(
    object: &JsonObject,
    name: &'static str,
    nonempty: bool,
) -> Result<(), CarrierError> {
    optional_labelled(object, name, name, nonempty)
}
pub(crate) fn optional_labelled(
    object: &JsonObject,
    name: &'static str,
    label: &'static str,
    nonempty: bool,
) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get(name) {
        string(value, label, nonempty)?;
    }
    Ok(())
}
pub(crate) fn schema(value: JsonRef<'_>, name: &'static str) -> Result<(), CarrierError> {
    if matches!(value.kind(), JsonKind::Object | JsonKind::Boolean) {
        Ok(())
    } else {
        Err(CarrierError::FieldType(name))
    }
}
pub(crate) fn details(object: &JsonObject, label: &'static str) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get("details")
        && value.kind() != JsonKind::Object
    {
        return Err(CarrierError::FieldType(label));
    }
    Ok(())
}
pub(crate) fn uses(object: &JsonObject) -> Result<(), CarrierError> {
    if let Some(values) = object.value().get("uses") {
        let mut labels = BTreeSet::new();
        for value in values.elements().ok_or(CarrierError::FieldType("uses"))? {
            let label = string(value, "uses", true)?;
            if !labels.insert(label.utf16_units().collect::<Vec<_>>()) {
                return Err(CarrierError::DuplicateUse);
            }
        }
    }
    Ok(())
}
pub(crate) fn forbidden(object: &JsonObject, fields: &[&str]) -> bool {
    fields.iter().any(|name| object.value().get(name).is_some())
}
pub(crate) fn boolean(object: &JsonObject, name: &'static str) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get(name)
        && value.kind() != JsonKind::Boolean
    {
        return Err(CarrierError::FieldType(name));
    }
    Ok(())
}
pub(crate) fn binding(value: JsonValue) -> Result<(), CarrierError> {
    let object = object(value)?;
    string(field(&object, "operation")?, "operation", false)?;
    string(field(&object, "source")?, "source", true)?;
    optional_string(&object, "description", false)?;
    for name in ["idempotent", "deprecated"] {
        boolean(&object, name)?;
    }
    if let Some(preference) = object.value().get("preference") {
        use openbindings_internal_json::decimal::Decimal;
        let number = Decimal::parse(
            preference
                .number_text()
                .ok_or(CarrierError::FieldType("preference"))?,
        );
        if !number.is_integer()
            || number.compare(&Decimal::parse("-9007199254740991")).is_lt()
            || number.compare(&Decimal::parse("9007199254740991")).is_gt()
        {
            return Err(CarrierError::FieldType("preference"));
        }
    }
    // Whole core-field/name relationships are checked on the prospective
    // document; this is the published open Binding carrier's structural check.
    Ok(())
}

/// Validate a nonempty list of labels without assigning meaning to opaque data.
pub(crate) fn labels(object: &JsonObject, name: &'static str) -> Result<(), CarrierError> {
    if let Some(value) = object.value().get(name) {
        let values = value.elements().ok_or(CarrierError::FieldType(name))?;
        if values.len() == 0 {
            return Err(CarrierError::FieldType(name));
        }
        for value in values {
            string(value, name, true)?;
        }
    }
    Ok(())
}
pub(crate) fn removals(object: &JsonObject, allowed: &[&str]) -> Result<(), CarrierError> {
    labels(object, "remove")?;
    if let Some(value) = object.value().get("remove") {
        let mut seen = BTreeSet::new();
        for value in value.elements().expect("removal list") {
            let name = value.as_str().ok_or(CarrierError::FieldType("remove"))?;
            if !allowed.contains(&name) || !seen.insert(name) || object.value().get(name).is_some()
            {
                return Err(CarrierError::FieldType("remove"));
            }
        }
    }
    Ok(())
}

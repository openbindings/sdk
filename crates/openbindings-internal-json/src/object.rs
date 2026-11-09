//! Owning object access without conflating shape with semantic validity.
use crate::{JsonKind, JsonValue};
use std::fmt;

/// A shape-checked exact JSON object. This proves neither document conformance
/// nor JSON Schema validity. Clones retain the same immutable backing value.
#[derive(Clone)]
pub struct JsonObject(JsonValue);

/// Exact-object access failed. Diagnostic formatting exposes no member name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonAccessError {
    /// The value is not a JSON object.
    ExpectedObject,
    /// The requested decoded member name occurs more than once.
    DuplicateMember,
}
impl fmt::Display for JsonAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExpectedObject => "expected a JSON object",
            Self::DuplicateMember => "the requested JSON member is ambiguous",
        })
    }
}
impl std::error::Error for JsonAccessError {}
impl fmt::Debug for JsonObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonObject").finish_non_exhaustive()
    }
}
impl TryFrom<JsonValue> for JsonObject {
    type Error = JsonAccessError;
    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        if value.kind() == JsonKind::Object {
            Ok(Self(value))
        } else {
            Err(JsonAccessError::ExpectedObject)
        }
    }
}
impl JsonValue {
    /// Retain an owning object view after checking only JSON shape.
    ///
    /// # Errors
    /// Returns [JsonAccessError::ExpectedObject] for a non-object value.
    pub fn as_object(&self) -> Result<JsonObject, JsonAccessError> {
        JsonObject::try_from(self.clone())
    }
}
impl JsonObject {
    /// Borrow the exact object, including extensions and unknown members.
    pub fn value(&self) -> &JsonValue {
        &self.0
    }
    /// Consume this object view and retain its exact value.
    #[must_use]
    pub fn into_value(self) -> JsonValue {
        self.0
    }
    /// Retain a member by its decoded name. Absence differs from present null.
    /// Duplicate occurrences of other names do not make this lookup ambiguous.
    ///
    /// # Errors
    /// Returns [JsonAccessError::DuplicateMember] when the requested decoded
    /// name occurs more than once. No first/last-member winner is selected.
    pub fn member(&self, name: &str) -> Result<Option<JsonValue>, JsonAccessError> {
        if !self.0.has_duplicate_names() {
            return Ok(self.0.get(name).map(|value| value.to_owned()));
        }
        let mut found = None;
        for member in self.0.view().members().expect("checked object") {
            if member.name.as_str() == Some(name) {
                if found.is_some() {
                    return Err(JsonAccessError::DuplicateMember);
                }
                found = Some(member.value.to_owned());
            }
        }
        Ok(found)
    }
    /// Serialize exact object text without converting through native numbers.
    pub fn to_json(&self) -> String {
        self.0.text().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_shape_does_not_forge_uniqueness_or_document_validity() {
        let raw = JsonValue::parse(r#"{"a":1,"\\u0061":2,"a":3,"null":null}"#).unwrap();
        let object = raw.as_object().unwrap();
        assert_eq!(
            object.member("a").unwrap_err(),
            JsonAccessError::DuplicateMember
        );
        assert!(object.member("absent").unwrap().is_none());
        assert_eq!(
            object.member("null").unwrap().unwrap().kind(),
            JsonKind::Null
        );
        assert_eq!(
            JsonValue::parse("[]").unwrap().as_object().unwrap_err(),
            JsonAccessError::ExpectedObject
        );
    }

    #[test]
    fn decoded_duplicates_are_ambiguous_and_exact_values_outlive_the_object() {
        let raw =
            JsonValue::parse(r#"{"a":1,"\u0061":2,"x":{"n":9007199254740993,"huge":1e999999}}"#)
                .unwrap();
        let object = raw.as_object().unwrap();
        assert_eq!(
            object.member("a").unwrap_err(),
            JsonAccessError::DuplicateMember
        );
        let child = object.member("x").unwrap().unwrap();
        assert_eq!(object.to_json(), raw.text());
        drop(object);
        drop(raw);
        assert_eq!(child.text(), r#"{"n":9007199254740993,"huge":1e999999}"#);
    }
}

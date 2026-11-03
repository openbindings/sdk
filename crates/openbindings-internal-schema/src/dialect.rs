//! Interpretation of original external schema resources. Contained OBI schema
//! positions retain their separate, fixed 2020-12 admission walk.
use crate::{JsonKind, JsonRef, schema_index::SchemaSelector};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Draft7,
    Draft201909,
    Draft202012,
}

impl Dialect {
    pub fn from_uri(uri: &str) -> Option<Self> {
        match uri.strip_suffix('#').unwrap_or(uri) {
            "http://json-schema.org/draft-07/schema" => Some(Self::Draft7),
            "https://json-schema.org/draft/2019-09/schema" => Some(Self::Draft201909),
            "https://json-schema.org/draft/2020-12/schema" => Some(Self::Draft202012),
            _ => None,
        }
    }

    /// A reference object replaces all its sibling interpretation in draft-07.
    pub fn reference_object(self, schema: JsonRef<'_>) -> bool {
        self == Self::Draft7 && schema.get("$ref").is_some()
    }

    pub fn active(self, keyword: &str) -> bool {
        match keyword {
            "$dynamicRef" | "$dynamicAnchor" | "prefixItems" => self == Self::Draft202012,
            "$recursiveRef" | "$recursiveAnchor" => self == Self::Draft201909,
            "$anchor"
            | "$defs"
            | "$vocabulary"
            | "dependentSchemas"
            | "dependentRequired"
            | "unevaluatedProperties"
            | "unevaluatedItems"
            | "minContains"
            | "maxContains"
            | "contentSchema" => self != Self::Draft7,
            "dependencies" => self == Self::Draft7,
            "additionalItems" => self != Self::Draft202012,
            _ => true,
        }
    }

    pub fn anchor_name(self, name: &str) -> bool {
        match self {
            Self::Draft7 => false,
            Self::Draft201909 => draft7_name(name),
            Self::Draft202012 => crate::schema_index::plain_name(name),
        }
    }

    /// Inventory includes deprecated schema-valued positions described by the
    /// standard meta-schema even when their parent is not an active applicator.
    pub fn selector(self, keyword: &str, value: JsonRef<'_>) -> Option<SchemaSelector> {
        use SchemaSelector::{Array, Direct, Map, MapOrStringArray};
        Some(match keyword {
            "definitions" | "properties" | "patternProperties" => Map,
            "dependencies" => MapOrStringArray,
            "$defs" | "dependentSchemas" if self != Self::Draft7 => Map,
            "items" if self != Self::Draft202012 && value.kind() == JsonKind::Array => Array,
            "additionalItems" if self != Self::Draft202012 => Direct,
            "allOf" | "anyOf" | "oneOf" => Array,
            "prefixItems" if self == Self::Draft202012 => Array,
            "additionalProperties"
            | "propertyNames"
            | "items"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => Direct,
            "unevaluatedProperties" | "unevaluatedItems" | "contentSchema"
                if self != Self::Draft7 =>
            {
                Direct
            }
            _ => return None,
        })
    }

    pub fn each_child<'a>(self, schema: JsonRef<'a>, mut visit: impl FnMut(JsonRef<'a>)) {
        if self.reference_object(schema) {
            return;
        }
        if let Some(fields) = schema.members() {
            for field in fields {
                let Some(selector) = field
                    .name
                    .as_str()
                    .and_then(|k| self.selector(k, field.value))
                else {
                    continue;
                };
                crate::schema_index::selected_children(field.value, selector, &mut visit);
            }
        }
    }
}

/// Draft-07 plain-name fragments use their own grammar, including colon.
pub fn draft7_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic()
                || (i > 0 && (c.is_ascii_digit() || matches!(c, b'_' | b'.' | b':' | b'-')))
        })
}

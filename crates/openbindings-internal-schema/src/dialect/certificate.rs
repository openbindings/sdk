//! Limited identity knowledge for supplied editions that the evaluator does
//! not implement. These rules may exclude a possible name collision; they
//! never establish schema validity, active evaluation keywords, or real nodes.
//!
//! Authority: the separately pinned draft-06 and v1 core/validation sources in
//! tools/qualification/inputs/identity-authority. Unknown extensions or normalization work
//! outside this guard leave the namespace uncertain.
use crate::{JsonKind, JsonRef, schema_index::SchemaSelector};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CertificateDialect {
    Draft6,
    V1,
}

impl CertificateDialect {
    pub(crate) fn from_uri(uri: &str) -> Option<Self> {
        match uri {
            "http://json-schema.org/draft-06/schema"
            | "http://json-schema.org/draft-06/schema#" => Some(Self::Draft6),
            "https://json-schema.org/v1" => Some(Self::V1),
            _ => None,
        }
    }

    pub(crate) fn reference_object(self, value: JsonRef<'_>) -> bool {
        self == Self::Draft6 && value.get("$ref").is_some()
    }

    /// v1 extensions can introduce identifiers or schema positions. Only the
    /// published keywords and explicitly reserved annotation prefix are known.
    /// Proposed vocabularies/propertyDependencies are deliberately not assumed
    /// to have the rules of any earlier edition.
    #[cfg(test)]
    pub(crate) fn known_namespace(self, value: JsonRef<'_>) -> bool {
        self == Self::Draft6
            || value.members().is_none_or(|mut fields| {
                fields.all(|field| {
                    field
                        .name
                        .as_str()
                        .is_some_and(|name| self.known_keyword(name))
                })
            })
    }

    pub(crate) fn known_keyword(self, name: &str) -> bool {
        self == Self::Draft6 || {
            name.starts_with("x-")
                || matches!(
                    name,
                    "$id"
                        | "$schema"
                        | "$ref"
                        | "$anchor"
                        | "$dynamicRef"
                        | "$dynamicAnchor"
                        | "$comment"
                        | "$defs"
                        | "title"
                        | "description"
                        | "default"
                        | "deprecated"
                        | "readOnly"
                        | "writeOnly"
                        | "examples"
                        | "prefixItems"
                        | "items"
                        | "maxContains"
                        | "minContains"
                        | "contains"
                        | "additionalProperties"
                        | "properties"
                        | "patternProperties"
                        | "dependentSchemas"
                        | "propertyNames"
                        | "if"
                        | "then"
                        | "else"
                        | "allOf"
                        | "anyOf"
                        | "oneOf"
                        | "not"
                        | "unevaluatedItems"
                        | "unevaluatedProperties"
                        | "type"
                        | "const"
                        | "enum"
                        | "multipleOf"
                        | "maximum"
                        | "exclusiveMaximum"
                        | "minimum"
                        | "exclusiveMinimum"
                        | "maxLength"
                        | "minLength"
                        | "pattern"
                        | "maxItems"
                        | "minItems"
                        | "uniqueItems"
                        | "maxProperties"
                        | "minProperties"
                        | "required"
                        | "dependentRequired"
                        | "format"
                        | "contentEncoding"
                        | "contentMediaType"
                        | "contentSchema"
                )
        }
    }

    pub(crate) fn selector(self, keyword: &str, value: JsonRef<'_>) -> Option<SchemaSelector> {
        use SchemaSelector::{Array, Direct, Map, MapOrStringArray};
        Some(match keyword {
            "properties" | "patternProperties" => Map,
            "definitions" if self == Self::Draft6 => Map,
            "dependencies" if self == Self::Draft6 => MapOrStringArray,
            "$defs" | "dependentSchemas" if self == Self::V1 => Map,
            "items" if self == Self::Draft6 && value.kind() == JsonKind::Array => Array,
            "allOf" | "anyOf" | "oneOf" => Array,
            "prefixItems" if self == Self::V1 => Array,
            "additionalItems" if self == Self::Draft6 => Direct,
            "additionalProperties" | "propertyNames" | "items" | "contains" | "not" => Direct,
            "if"
            | "then"
            | "else"
            | "unevaluatedProperties"
            | "unevaluatedItems"
            | "contentSchema"
                if self == Self::V1 =>
            {
                Direct
            }
            _ => return None,
        })
    }

    /// An identifier form whose base/alias implications the potential scanner
    /// can account for. This does not admit the surrounding schema.
    pub(crate) fn identifier(self, text: &str) -> bool {
        match self {
            Self::Draft6 => {
                crate::uri::valid(text)
                    && (text.strip_prefix('#').is_some_and(super::draft7_name)
                        || crate::uri::fragment(text).is_none_or(str::is_empty))
            }
            Self::V1 => normalized_ascii_reference(text) && !text.contains('#'),
        }
    }

    pub(crate) fn anchor(self, name: &str) -> bool {
        match self {
            Self::Draft6 => super::draft7_name(name),
            // This is the ASCII subset of v1's NCName, not a claim that v1
            // restricts names to ASCII. Wider names leave certification open.
            Self::V1 => crate::schema_index::plain_name(name),
        }
    }
}

/// A deliberately narrow subset for which RFC 3987 syntax normalization has
/// no work left: ASCII, no percent escapes, lowercase scheme/authority, and
/// no dot segments. In particular, URI parsing alone is insufficient for v1.
pub(crate) fn normalized_ascii_reference(text: &str) -> bool {
    if !text.is_ascii() || text.contains('%') {
        return false;
    }
    let Ok(uri) = fluent_uri::UriRef::parse(text) else {
        return false;
    };
    if uri
        .scheme()
        .is_some_and(|s| s.as_str().bytes().any(|b| b.is_ascii_uppercase()))
        || uri
            .authority()
            .is_some_and(|a| a.as_str().bytes().any(|b| b.is_ascii_uppercase()))
    {
        return false;
    }
    !uri.path()
        .as_str()
        .split('/')
        .any(|part| matches!(part, "." | ".."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonValue;

    #[test]
    fn certificate_knowledge_does_not_claim_evaluation_support() {
        for uri in [
            "http://json-schema.org/draft-06/schema#",
            "https://json-schema.org/v1",
        ] {
            assert!(CertificateDialect::from_uri(uri).is_some());
            assert!(super::super::Profile::from_uri(uri).is_none());
        }
        assert!(CertificateDialect::from_uri("https://json-schema.org/v1#").is_none());
        assert!(CertificateDialect::from_uri("https://json-schema.org/v1/unknown").is_none());
    }

    #[test]
    fn edition_selectors_and_reference_siblings_stay_distinct() {
        let value =
            JsonValue::parse(r##"{"$ref":"#","items":[true],"definitions":{"a":true}}"##).unwrap();
        assert!(CertificateDialect::Draft6.reference_object(value.view()));
        assert!(!CertificateDialect::V1.reference_object(value.view()));
        let items = value.view().get("items").unwrap();
        assert!(matches!(
            CertificateDialect::Draft6.selector("items", items),
            Some(SchemaSelector::Array)
        ));
        assert!(matches!(
            CertificateDialect::V1.selector("items", items),
            Some(SchemaSelector::Direct)
        ));
        assert!(
            CertificateDialect::V1
                .selector("definitions", items)
                .is_none()
        );
        assert!(CertificateDialect::Draft6.selector("if", items).is_none());
    }

    #[test]
    fn v1_unknown_extensions_preserve_namespace_uncertainty() {
        for text in [
            r#"{"x-note":{"$id":"other"}}"#,
            r#"{"$defs":{"a":true},"type":"string"}"#,
        ] {
            assert!(CertificateDialect::V1.known_namespace(JsonValue::parse(text).unwrap().view()));
        }
        for text in [
            r#"{"definitions":{}}"#,
            r#"{"$vocabulary":{}}"#,
            r#"{"propertyDependencies":{}}"#,
            r#"{"extension":{"$id":"other"}}"#,
            r#"{"\ud800":true}"#,
        ] {
            assert!(
                !CertificateDialect::V1.known_namespace(JsonValue::parse(text).unwrap().view())
            );
        }
    }

    #[test]
    fn normalized_identity_guard_refuses_unimplemented_normalization() {
        for text in [
            "https://example.test/A",
            "urn:example:schema",
            "child.json",
            "a/b",
        ] {
            assert!(CertificateDialect::V1.identifier(text), "{text}");
        }
        for text in [
            "HTTPS://example.test/a",
            "https://EXAMPLE.test/a",
            "https://example.test/%61",
            "https://example.test/é",
            "../child",
            "a/./b",
            "#",
            "#name",
            "https://example.test/a#",
        ] {
            assert!(!CertificateDialect::V1.identifier(text), "{text}");
        }
        assert!(CertificateDialect::Draft6.identifier("#name:part"));
        assert!(CertificateDialect::Draft6.identifier("../child#"));
        assert!(!CertificateDialect::Draft6.identifier("other#name"));
        assert!(!CertificateDialect::Draft6.identifier("#9name"));
        assert!(CertificateDialect::V1.anchor("_name"));
        assert!(!CertificateDialect::V1.anchor("name:part"));
        assert!(!CertificateDialect::V1.anchor("é"));
    }
}

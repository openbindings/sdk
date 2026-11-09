//! A schema's interpreted keyword set, separate from the meta-schema that
//! declares it. This value alone is neither a carrier-stability certificate
//! nor proof that any schema satisfies that meta-schema.
use super::Dialect;
use crate::{JsonRef, schema_index::SchemaSelector};

const CORE: u8 = 1;
const APPLICATOR: u8 = 2;
const VALIDATION: u8 = 4;
const UNEVALUATED: u8 = 8;
const METADATA: u8 = 16;
const CONTENT: u8 = 32;
const FORMAT: u8 = 64;
const ALL: u8 = CORE | APPLICATOR | VALIDATION | UNEVALUATED | METADATA | CONTENT | FORMAT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileFailure {
    MissingVocabularyMap,
    InvalidVocabularyMap,
    MissingCore,
    CoreNotRequired,
    UnsupportedRequiredVocabulary,
}

#[derive(Clone, Debug)]
pub enum ProfileConstructionFailure {
    Declaration(ProfileFailure),
    Work(crate::Failure),
}
impl From<ProfileFailure> for ProfileConstructionFailure {
    fn from(failure: ProfileFailure) -> Self {
        Self::Declaration(failure)
    }
}
impl From<crate::Failure> for ProfileConstructionFailure {
    fn from(failure: crate::Failure) -> Self {
        Self::Work(failure)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    edition: Dialect,
    vocabularies: u8,
    required_format_assertion: bool,
    custom: bool,
}

impl Profile {
    pub const fn standard(edition: Dialect) -> Self {
        Self {
            edition,
            vocabularies: ALL,
            required_format_assertion: false,
            custom: false,
        }
    }

    /// Extract only the directly authored 2020-12 vocabulary requirements.
    /// The caller must separately certify the original meta-schema carrier and
    /// validate consumer schemas against that carrier's own interpretation.
    pub fn from_vocabulary(map: Option<JsonRef<'_>>) -> Result<Self, ProfileFailure> {
        match Self::from_vocabulary_with_control(map, &|| false) {
            Ok(profile) => Ok(profile),
            Err(ProfileConstructionFailure::Declaration(failure)) => Err(failure),
            Err(ProfileConstructionFailure::Work(_)) => {
                unreachable!("uncancelled unbounded profile extraction")
            }
        }
    }

    /// Bounded extraction for environment construction. Namespace declarations
    /// and exhausted work remain separate failures; no proof is cached here.
    pub fn from_vocabulary_with_control(
        map: Option<JsonRef<'_>>,
        control: &dyn crate::Control,
    ) -> Result<Self, ProfileConstructionFailure> {
        control.check()?;
        let map = map.ok_or(ProfileFailure::MissingVocabularyMap)?;
        let fields = map.members().ok_or(ProfileFailure::InvalidVocabularyMap)?;
        let mut profile = Self {
            edition: Dialect::Draft202012,
            vocabularies: 0,
            required_format_assertion: false,
            custom: true,
        };
        for field in fields {
            control.check()?;
            let bytes = field.name.text().len();
            control.charge_work(bytes.saturating_mul(16))?;
            control.charge_allocation(bytes.saturating_mul(8).saturating_add(64))?;
            let uri = field
                .name
                .as_str()
                .ok_or(ProfileFailure::InvalidVocabularyMap)?;
            if !normalized_vocabulary_uri(uri) {
                return Err(ProfileFailure::InvalidVocabularyMap.into());
            }
            let required = field
                .value
                .as_bool()
                .ok_or(ProfileFailure::InvalidVocabularyMap)?;
            // Vocabulary identifiers name processing contracts. Schema-resource
            // retrieval aliases (including an empty fragment) do not rename
            // those contracts.
            let vocabulary = match uri {
                "https://json-schema.org/draft/2020-12/vocab/core" => {
                    if !required {
                        return Err(ProfileFailure::CoreNotRequired.into());
                    }
                    CORE
                }
                "https://json-schema.org/draft/2020-12/vocab/applicator" => APPLICATOR,
                "https://json-schema.org/draft/2020-12/vocab/validation" => VALIDATION,
                "https://json-schema.org/draft/2020-12/vocab/unevaluated" => UNEVALUATED,
                "https://json-schema.org/draft/2020-12/vocab/meta-data" => METADATA,
                "https://json-schema.org/draft/2020-12/vocab/content" => CONTENT,
                "https://json-schema.org/draft/2020-12/vocab/format-annotation" => FORMAT,
                "https://json-schema.org/draft/2020-12/vocab/format-assertion" => {
                    profile.required_format_assertion |= required;
                    FORMAT
                }
                _ if required => return Err(ProfileFailure::UnsupportedRequiredVocabulary.into()),
                _ => 0,
            };
            // false makes a known vocabulary optional, not inactive. OBI's
            // optional-format policy is represented separately above.
            profile.vocabularies |= vocabulary;
        }
        if profile.vocabularies & CORE == 0 {
            return Err(ProfileFailure::MissingCore.into());
        }
        Ok(profile)
    }

    pub fn from_uri(uri: &str) -> Option<Self> {
        Dialect::from_uri(uri).map(Self::standard)
    }
    pub fn reference_object(self, schema: JsonRef<'_>) -> bool {
        self.edition.reference_object(schema)
    }
    pub fn anchor_name(self, name: &str) -> bool {
        self.edition.anchor_name(name)
    }

    pub const fn edition(self) -> Dialect {
        self.edition
    }
    pub const fn is_custom(self) -> bool {
        self.custom
    }
    pub const fn asserts_format(self) -> bool {
        self.required_format_assertion
    }

    pub fn active(self, keyword: &str) -> bool {
        if !self.custom {
            return self.edition.active(keyword);
        }
        let vocabulary = match keyword {
            "$schema" | "$id" | "$ref" | "$dynamicRef" | "$anchor" | "$dynamicAnchor" | "$defs"
            | "$vocabulary" | "$comment" => CORE,
            "properties"
            | "patternProperties"
            | "additionalProperties"
            | "propertyNames"
            | "dependentSchemas"
            | "items"
            | "prefixItems"
            | "contains"
            | "allOf"
            | "anyOf"
            | "oneOf"
            | "not"
            | "if"
            | "then"
            | "else" => APPLICATOR,
            "type" | "enum" | "const" | "minimum" | "maximum" | "exclusiveMinimum"
            | "exclusiveMaximum" | "multipleOf" | "minLength" | "maxLength" | "pattern"
            | "minItems" | "maxItems" | "uniqueItems" | "minContains" | "maxContains"
            | "minProperties" | "maxProperties" | "required" | "dependentRequired" => VALIDATION,
            "unevaluatedProperties" | "unevaluatedItems" => UNEVALUATED,
            "title" | "description" | "default" | "examples" | "deprecated" | "readOnly"
            | "writeOnly" => METADATA,
            "contentEncoding" | "contentMediaType" | "contentSchema" => CONTENT,
            "format" => FORMAT,
            _ => return false,
        };
        self.vocabularies & vocabulary != 0
    }

    pub fn selector(self, keyword: &str, value: JsonRef<'_>) -> Option<SchemaSelector> {
        if self.custom && !self.active(keyword) {
            return None;
        }
        self.edition.selector(keyword, value)
    }

    pub fn get<'a>(self, schema: JsonRef<'a>, keyword: &str) -> Option<JsonRef<'a>> {
        if !self.active(keyword) || (self.edition.reference_object(schema) && keyword != "$ref") {
            None
        } else {
            schema.get(keyword)
        }
    }

    pub fn each_child<'a>(self, schema: JsonRef<'a>, mut visit: impl FnMut(JsonRef<'a>)) {
        if self.edition.reference_object(schema) {
            return;
        }
        if let Some(fields) = schema.members() {
            for field in fields {
                if let Some(selector) = field
                    .name
                    .as_str()
                    .and_then(|k| self.selector(k, field.value))
                {
                    crate::schema_index::selected_children(field.value, selector, &mut visit);
                }
            }
        }
    }
}

/// Vocabulary maps author normalized absolute URI names. Reject rather than
/// repair non-normalized declarations, including optional unknown names.
/// This applies fluent-uri's RFC 3986 syntax normalization, canonical IPv6
/// spelling and empty-port removal, with scheme-specific default-port removal
/// explicitly disabled. Empty query/fragment presence and reserved escapes
/// remain significant; no schema-resource alias policy is applied.
fn normalized_vocabulary_uri(text: &str) -> bool {
    let Ok(uri) = fluent_uri::Uri::parse(text) else {
        return false;
    };
    fluent_uri::normalize::Normalizer::new()
        .default_port_with(|_| None)
        .normalize(&uri)
        .is_ok_and(|normal| normal.as_str() == text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonValue;
    fn profile(fields: &str) -> Result<Profile, ProfileFailure> {
        let map = JsonValue::parse(format!(
            r#"{{"https://json-schema.org/draft/2020-12/vocab/core":true{fields}}}"#
        ))
        .unwrap();
        Profile::from_vocabulary(Some(map.view()))
    }

    #[test]
    fn optional_known_vocabularies_are_active_but_absent_ones_are_not() {
        let schema =
            JsonValue::parse(r#"{"minimum":10,"properties":{"x":false},"format":"ipv4"}"#).unwrap();
        let optional = profile(r#", "https://json-schema.org/draft/2020-12/vocab/validation":false, "https://json-schema.org/draft/2020-12/vocab/applicator":false, "https://json-schema.org/draft/2020-12/vocab/format-assertion":false"#).unwrap();
        assert!(optional.get(schema.view(), "minimum").is_some());
        let mut children = Vec::new();
        optional.each_child(schema.view(), |v| children.push(v.as_bool()));
        assert_eq!(children, vec![Some(false)]);
        assert!(!optional.asserts_format());
        let absent = profile("").unwrap();
        assert!(absent.get(schema.view(), "minimum").is_none());
        assert!(absent.get(schema.view(), "format").is_none());
        let mut children = 0;
        absent.each_child(schema.view(), |_| children += 1);
        assert_eq!(children, 0);
    }

    #[test]
    fn core_is_required_and_unknown_vocabulary_requirements_are_honest() {
        for (text, expected) in [
            ("{}", ProfileFailure::MissingCore),
            (
                r#"{"https://json-schema.org/draft/2020-12/vocab/core":false}"#,
                ProfileFailure::CoreNotRequired,
            ),
            (
                r#"{"https://json-schema.org/draft/2020-12/vocab/core":1}"#,
                ProfileFailure::InvalidVocabularyMap,
            ),
            (
                r#"{"relative":false}"#,
                ProfileFailure::InvalidVocabularyMap,
            ),
            ("[]", ProfileFailure::InvalidVocabularyMap),
        ] {
            let map = JsonValue::parse(text).unwrap();
            assert_eq!(Profile::from_vocabulary(Some(map.view())), Err(expected));
        }
        assert_eq!(
            Profile::from_vocabulary(None),
            Err(ProfileFailure::MissingVocabularyMap)
        );
        assert_eq!(
            profile(r#", "https://custom.test/vocabulary":true"#),
            Err(ProfileFailure::UnsupportedRequiredVocabulary)
        );
        assert!(profile(r#", "https://custom.test/vocabulary":false"#).is_ok());
    }

    #[test]
    fn vocabulary_names_do_not_inherit_schema_resource_aliases() {
        for name in [
            "https://json-schema.org/draft/2020-12/vocab/validation#",
            "https://json-schema.org/draft/2020-12/vocab/validation#named",
        ] {
            assert_eq!(
                profile(&format!(r#", "{name}":true"#)),
                Err(ProfileFailure::UnsupportedRequiredVocabulary)
            );
            assert!(
                !profile(&format!(r#", "{name}":false"#))
                    .unwrap()
                    .active("minimum")
            );
        }
        let map = JsonValue::parse(r#"{"https://json-schema.org/draft/2020-12/vocab/core#":true}"#)
            .unwrap();
        assert_eq!(
            Profile::from_vocabulary(Some(map.view())),
            Err(ProfileFailure::UnsupportedRequiredVocabulary)
        );
    }

    #[test]
    fn vocabulary_uri_normalization_is_checked_before_optional_handling() {
        for name in [
            "HTTPS://example.test/vocab",
            "https://EXAMPLE.test/vocab",
            "https://example.test/%76ocab",
            "https://example.test/a/../vocab",
            "https://example.test/%2f",
            "https://example.test:/vocab",
            "https://[0:0:0:0:0:0:0:1]/vocab",
        ] {
            for required in [true, false] {
                assert_eq!(
                    profile(&format!(r#", "{name}":{required}"#)),
                    Err(ProfileFailure::InvalidVocabularyMap),
                    "{name}"
                );
            }
        }
        for name in [
            "https://example.test/Vocab",
            "https://example.test/%2F",
            "https://example.test/vocab?",
            "https://example.test/vocab#",
            "https://example.test:443/vocab",
            "https://[::1]/vocab",
            "unknown://example.test:123/vocab",
        ] {
            assert!(profile(&format!(r#", "{name}":false"#)).is_ok(), "{name}");
            assert_eq!(
                profile(&format!(r#", "{name}":true"#)),
                Err(ProfileFailure::UnsupportedRequiredVocabulary),
                "{name}"
            );
        }
    }

    #[test]
    fn custom_inventory_uses_declared_vocabularies_not_standard_deprecated_positions() {
        let schema = JsonValue::parse(r#"{"$defs":{"known":true},"definitions":{"unknown":false},"properties":{"inactive":false},"contentSchema":false}"#).unwrap();
        let core = profile("").unwrap();
        let mut children = Vec::new();
        core.each_child(schema.view(), |v| children.push(v.as_bool()));
        assert_eq!(children, vec![Some(true)]);
        let content =
            profile(r#", "https://json-schema.org/draft/2020-12/vocab/content":false"#).unwrap();
        let mut children = Vec::new();
        content.each_child(schema.view(), |v| children.push(v.as_bool()));
        assert_eq!(children, vec![Some(true), Some(false)]);
        let mut standard = 0;
        Profile::standard(Dialect::Draft202012).each_child(schema.view(), |_| standard += 1);
        assert_eq!(standard, 4);
    }

    #[test]
    fn required_format_assertion_is_separate_from_annotation_membership() {
        let required = profile(r#", "https://json-schema.org/draft/2020-12/vocab/format-assertion":true, "https://json-schema.org/draft/2020-12/vocab/format-annotation":false"#).unwrap();
        assert!(required.asserts_format());
        let annotation =
            profile(r#", "https://json-schema.org/draft/2020-12/vocab/format-annotation":true"#)
                .unwrap();
        assert!(annotation.active("format"));
        assert!(!annotation.asserts_format());
        assert!(!Profile::standard(Dialect::Draft202012).asserts_format());
    }
}

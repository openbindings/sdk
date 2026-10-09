//! Fixed normative schema checks, compiled once without external acquisition.
use crate::JsonValue;
use jsonschema::{Draft, Retrieve, Validator};
use openbindings_internal_json::{
    backend::{FlatJson, view},
    numeric,
};
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

#[derive(Clone)]
struct LocalResources(Arc<BTreeMap<String, Value>>);
impl Retrieve for LocalResources {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("fixed resource unavailable: {uri}").into())
    }
}
pub(crate) struct Problems {
    pub entries: Vec<Problem>,
    pub truncated: bool,
}
pub(crate) struct Problem {
    pub path: String,
    pub message: Cow<'static, str>,
}
const GENERIC_MESSAGE: &str = "value violates the fixed normative schema";
const MAX_MESSAGE_BYTES: usize = 512;
const MAX_REQUIRED_NAME_BYTES: usize = 64;

// Called only for validators compiled from the embedded document/meta schemas.
// In particular, Required.property comes from those trusted schemas, never the
// rejected instance. Do not format a vendor error or descend into union branches.
fn diagnostic_message(kind: &jsonschema::error::ValidationErrorKind) -> Cow<'static, str> {
    use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
    let message = match kind {
        Kind::Type {
            kind: TypeKind::Single(expected),
        } => {
            format!("expected JSON type: {}", expected.as_str())
        }
        Kind::Type {
            kind: TypeKind::Multiple(expected),
        } => {
            // The type set contains at most the seven fixed JSON type names.
            let names: Vec<_> = expected.iter().map(|kind| kind.as_str()).collect();
            if names.is_empty() {
                return GENERIC_MESSAGE.into();
            }
            format!("expected one of these JSON types: {}", names.join(", "))
        }
        Kind::Required { property } => {
            let Some(name) = property
                .as_str()
                .filter(|name| name.len() <= MAX_REQUIRED_NAME_BYTES)
            else {
                return GENERIC_MESSAGE.into();
            };
            // Quoting keeps even a future embedded name's controls unambiguous.
            let Ok(quoted) = serde_json::to_string(name) else {
                return GENERIC_MESSAGE.into();
            };
            format!("object is missing required field {quoted}")
        }
        _ => return GENERIC_MESSAGE.into(),
    };
    if message.len() > MAX_MESSAGE_BYTES {
        GENERIC_MESSAGE.into()
    } else {
        message.into()
    }
}
type Compiled = Result<Validator<FlatJson>, String>;
static DOCUMENT: OnceLock<Compiled> = OnceLock::new();
static META: OnceLock<Compiled> = OnceLock::new();

fn compile(schema: &Value, resources: BTreeMap<String, Value>) -> Compiled {
    numeric::apply(jsonschema::options_for::<FlatJson>())
        .with_draft(Draft::Draft202012)
        .with_retriever(LocalResources(Arc::new(resources)))
        .should_validate_formats(false)
        .build(schema)
        .map_err(|e| e.to_string())
}
fn document() -> &'static Compiled {
    DOCUMENT.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(include_str!("../schemas/openbindings.schema.json"))
                .map_err(|e| e.to_string())?;
        compile(&schema, BTreeMap::new())
    })
}
fn meta() -> &'static Compiled {
    META.get_or_init(|| {
        // Each direct subschema is checked separately by the normative-position walk.
        // Substituting only recursive meta edges with the object/boolean type check
        // preserves all local keyword constraints and avoids repeated ancestor work.
        fn localize(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    if map.get("$dynamicRef").and_then(Value::as_str) == Some("#meta") {
                        map.remove("$dynamicRef");
                        map.insert("type".into(), serde_json::json!(["object", "boolean"]));
                    }
                    for (key, value) in map {
                        if key == "$id" {
                            if let Some(id) = value.as_str() {
                                *value = Value::String(id.replace(
                                    "https://json-schema.org/draft/2020-12/",
                                    "https://openbindings.invalid/internal-meta/",
                                ));
                            }
                        } else {
                            localize(value);
                        }
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        localize(value);
                    }
                }
                _ => {}
            }
        }
        let files = [
            include_str!("../schemas/draft2020-12/schema.json"),
            include_str!("../schemas/draft2020-12/meta/core.json"),
            include_str!("../schemas/draft2020-12/meta/applicator.json"),
            include_str!("../schemas/draft2020-12/meta/unevaluated.json"),
            include_str!("../schemas/draft2020-12/meta/validation.json"),
            include_str!("../schemas/draft2020-12/meta/meta-data.json"),
            include_str!("../schemas/draft2020-12/meta/format-annotation.json"),
            include_str!("../schemas/draft2020-12/meta/content.json"),
        ];
        let mut resources = BTreeMap::new();
        for text in files {
            let mut schema: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            localize(&mut schema);
            let id = schema["$id"]
                .as_str()
                .ok_or("embedded meta-schema has no identifier")?
                .to_owned();
            resources.insert(id, schema);
        }
        let root = resources
            .get("https://openbindings.invalid/internal-meta/schema")
            .ok_or("embedded root meta-schema is missing")?
            .clone();
        compile(&root, resources)
    })
}
pub(crate) fn check(
    value: &JsonValue,
    is_meta: bool,
    max_problems: usize,
) -> Result<Problems, String> {
    let validator = (if is_meta { meta() } else { document() })
        .as_ref()
        .map_err(Clone::clone)?;
    jsonschema::ob_work::bounded(2_000_000, 1024, || {
        jsonschema::ob_ecma::top_level(2_000_000, || {
            let mut entries = Vec::new();
            let mut truncated = false;
            for error in validator.iter_errors(view(value)) {
                if entries.len() >= max_problems.max(1) {
                    truncated = true;
                    break;
                }
                entries.push(Problem {
                    path: error.instance_path().as_str().to_owned(),
                    message: diagnostic_message(error.kind()),
                });
            }
            Problems { entries, truncated }
        })
    })
    .map_err(|e| format!("fixed validation work limit: {e:?}"))?
    .map_err(|_| "fixed pattern work limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_messages_are_bounded_and_do_not_guess_union_branches() {
        use jsonschema::error::{TypeKind, ValidationErrorKind as Kind};
        use jsonschema_value::types::JsonType;
        let types = JsonType::Boolean | JsonType::Object;
        assert_eq!(
            diagnostic_message(&Kind::Type {
                kind: TypeKind::Multiple(types)
            }),
            "expected one of these JSON types: boolean, object"
        );
        for kind in [
            Kind::AnyOf { context: vec![] },
            Kind::OneOfNotValid { context: vec![] },
            Kind::OneOfMultipleValid { context: vec![] },
            Kind::Required {
                property: Value::String("x".repeat(MAX_REQUIRED_NAME_BYTES + 1)),
            },
            Kind::Required {
                property: Value::Null,
            },
        ] {
            assert_eq!(diagnostic_message(&kind), GENERIC_MESSAGE);
        }
        let message = diagnostic_message(&Kind::Required {
            property: Value::String("\n".repeat(MAX_REQUIRED_NAME_BYTES)),
        });
        assert!(!message.contains('\n'));
        assert!(message.len() <= MAX_MESSAGE_BYTES);
        let longest_escaped = diagnostic_message(&Kind::Required {
            property: Value::String("\u{0000}".repeat(MAX_REQUIRED_NAME_BYTES)),
        });
        assert!(longest_escaped.len() <= MAX_MESSAGE_BYTES);
    }

    #[test]
    fn fixed_problem_count_paths_and_truncation_are_preserved() {
        let value = JsonValue::parse(
            r#"{"openbindings":"0.2.0","operations":{"a":{"description":1,"deprecated":2,"aliases":3}}}"#,
        ).unwrap();
        let all = check(&value, false, 100).unwrap();
        assert_eq!(all.entries.len(), 3);
        assert!(!all.truncated);
        for capacity in [0, 1, 2, 3] {
            let bounded = check(&value, false, capacity).unwrap();
            let retained = capacity.max(1);
            assert_eq!(bounded.entries.len(), retained);
            assert_eq!(bounded.truncated, capacity < 3);
            for (actual, expected) in bounded.entries.iter().zip(&all.entries) {
                assert_eq!(actual.path, expected.path);
                assert_eq!(actual.message, expected.message);
            }
        }
    }

    #[test]
    fn localized_meta_matches_unmodified_meta_on_keyword_shapes() {
        let files = [
            include_str!("../schemas/draft2020-12/schema.json"),
            include_str!("../schemas/draft2020-12/meta/core.json"),
            include_str!("../schemas/draft2020-12/meta/applicator.json"),
            include_str!("../schemas/draft2020-12/meta/unevaluated.json"),
            include_str!("../schemas/draft2020-12/meta/validation.json"),
            include_str!("../schemas/draft2020-12/meta/meta-data.json"),
            include_str!("../schemas/draft2020-12/meta/format-annotation.json"),
            include_str!("../schemas/draft2020-12/meta/content.json"),
        ];
        let resources: BTreeMap<_, _> = files
            .iter()
            .map(|text| {
                let v: Value = serde_json::from_str(text).unwrap();
                (v["$id"].as_str().unwrap().to_owned(), v)
            })
            .collect();
        let original =
            compile(&resources[crate::schema_index::DIALECT], resources.clone()).unwrap();
        let keywords = [
            "$schema",
            "$id",
            "$ref",
            "$dynamicRef",
            "$anchor",
            "$dynamicAnchor",
            "$defs",
            "type",
            "enum",
            "const",
            "required",
            "dependentRequired",
            "properties",
            "patternProperties",
            "additionalProperties",
            "dependentSchemas",
            "items",
            "prefixItems",
            "contains",
            "unevaluatedItems",
            "unevaluatedProperties",
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "then",
            "else",
            "contentSchema",
            "definitions",
            "dependencies",
            "minimum",
            "exclusiveMinimum",
            "multipleOf",
            "maxLength",
            "minLength",
            "minItems",
            "uniqueItems",
            "pattern",
            "format",
            "default",
            "examples",
            "unknown",
        ];
        let values = [
            "null",
            "true",
            "false",
            "0",
            "-1",
            "1.0",
            r#""string""#,
            r#""[""#,
            "[]",
            r#"["a","a"]"#,
            "{}",
            r#"{"x":{"type":"invalid"}}"#,
            r#"[{"type":"invalid"}]"#,
            r#"{"$dynamicRef":{"type":"string"}}"#,
        ];
        for keyword in keywords {
            for text in values {
                let raw = format!("{{{}:{text}}}", serde_json::to_string(keyword).unwrap());
                let value = JsonValue::parse(&raw).unwrap();
                let want = original.is_valid(view(&value));
                let mut stack = vec![value.clone()];
                let mut actual = true;
                while let Some(node) = stack.pop() {
                    actual &= check(&node, true, 100).unwrap().entries.is_empty();
                    stack.extend(
                        crate::schema_index::schema_children(node.view())
                            .into_iter()
                            .map(|v| v.to_owned()),
                    );
                }
                assert_eq!(actual, want, "{raw}");
            }
        }
    }
}

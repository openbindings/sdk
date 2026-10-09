//! Fixed normative schema checks, compiled once without external acquisition.
use crate::JsonValue;
use jsonschema::{Draft, Retrieve, Validator};
use openbindings_internal_json::{
    backend::{FlatJson, view},
    numeric,
};
use serde_json::Value;
use std::{
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
    pub paths: Vec<String>,
    pub truncated: bool,
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
            let mut paths = Vec::new();
            let mut truncated = false;
            for error in validator.iter_errors(view(value)) {
                if paths.len() >= max_problems.max(1) {
                    truncated = true;
                    break;
                }
                paths.push(error.instance_path().as_str().to_owned());
            }
            Problems { paths, truncated }
        })
    })
    .map_err(|e| format!("fixed validation work limit: {e:?}"))?
    .map_err(|_| "fixed pattern work limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    actual &= check(&node, true, 100).unwrap().paths.is_empty();
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

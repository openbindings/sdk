//! One closed static resource for value compilation. Original references are
//! already resolved; only private definition addresses are rewritten here.
use serde_json::{Map, Value};
use std::collections::HashMap;
pub(crate) const URI: &str = "urn:openbindings:bundle";
pub(crate) struct Resource {
    pub(crate) definitions: Map<String, Value>,
}
pub(crate) struct Bundle {
    names: Vec<String>,
    schemas: Vec<Value>,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) entry: u32,
}
pub(crate) struct Definition {
    pub(crate) edges: Vec<Edge>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    pub(crate) to: u32,
    pub(crate) weight: u32,
    pub(crate) descends: bool,
}
#[derive(Debug)]
pub(crate) struct Shape;
fn number(name: &str) -> Option<u32> {
    name.strip_prefix('n')?.parse().ok()
}
pub(crate) fn bundle(resources: Vec<Resource>, entry: &str) -> Result<Bundle, Shape> {
    let mut indices = HashMap::new();
    let mut names = Vec::new();
    for resource in &resources {
        for name in resource.definitions.keys() {
            let index = u32::try_from(names.len()).map_err(|_| Shape)?;
            indices.insert(number(name).ok_or(Shape)?, index);
            names.push(name.clone());
        }
    }
    let entry = entry
        .split_once('#')
        .and_then(|(_, fragment)| fragment.strip_prefix("/$defs/"))
        .and_then(number)
        .and_then(|n| indices.get(&n).copied())
        .ok_or(Shape)?;
    let mut schemas = Vec::with_capacity(names.len());
    let mut definitions = Vec::with_capacity(names.len());
    for resource in resources {
        for (_, mut schema) in resource.definitions {
            let mut edges = Vec::new();
            rewrite(&mut schema, 0, false, &indices, &mut edges)?;
            schemas.push(schema);
            definitions.push(Definition { edges });
        }
    }
    Ok(Bundle {
        names,
        schemas,
        definitions,
        entry,
    })
}
fn rewrite(
    value: &mut Value,
    depth: u32,
    descends: bool,
    indices: &HashMap<u32, u32>,
    edges: &mut Vec<Edge>,
) -> Result<(), Shape> {
    let Value::Object(schema) = value else {
        return Ok(());
    };
    // Dynamic reference scope and annotation consumers are selected out before this
    // lowering. Never replace their semantics with a stand-in or approximation.
    if schema.contains_key("$dynamicRef")
        || schema.contains_key("$recursiveRef")
        || schema.contains_key("unevaluatedProperties")
        || schema.contains_key("unevaluatedItems")
    {
        return Err(Shape);
    }
    schema.remove("$dynamicAnchor");
    for (keyword, operand) in schema.iter_mut() {
        let child_depth = depth + 1;
        let child_descends = descends
            || matches!(
                keyword.as_str(),
                "properties"
                    | "patternProperties"
                    | "additionalProperties"
                    | "propertyNames"
                    | "items"
                    | "prefixItems"
                    | "contains"
            );
        match (keyword.as_str(), operand) {
            ("properties" | "patternProperties" | "dependentSchemas", Value::Object(children)) => {
                for child in children.values_mut() {
                    rewrite(child, child_depth, child_descends, indices, edges)?;
                }
            }
            ("allOf" | "anyOf" | "oneOf" | "prefixItems", Value::Array(children)) => {
                for child in children {
                    rewrite(child, child_depth, child_descends, indices, edges)?;
                }
            }
            (
                "additionalProperties"
                | "propertyNames"
                | "items"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else",
                child,
            ) => rewrite(child, child_depth, child_descends, indices, edges)?,
            _ => {}
        }
    }
    if let Some(Value::String(reference)) = schema.get_mut("$ref") {
        let hash = reference.find('#').ok_or(Shape)?;
        let target = reference[hash..]
            .strip_prefix("#/$defs/")
            .and_then(number)
            .and_then(|n| indices.get(&n).copied())
            .ok_or(Shape)?;
        reference.replace_range(..hash, "");
        edges.push(Edge {
            to: target,
            weight: depth + 1,
            descends,
        });
    }
    Ok(())
}
impl Bundle {
    pub(crate) fn document(self) -> Value {
        let mut root = Map::new();
        root.insert("$id".into(), Value::String(URI.into()));
        root.insert(
            "$schema".into(),
            Value::String("https://json-schema.org/draft/2020-12/schema".into()),
        );
        root.insert(
            "$ref".into(),
            Value::String(format!("#/$defs/{}", self.names[self.entry as usize])),
        );
        root.insert(
            "$defs".into(),
            Value::Object(self.names.into_iter().zip(self.schemas).collect()),
        );
        Value::Object(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_edges_keep_same_instance_and_advancing_weights_without_reading_literals() {
        let fixtures = [
            (
                serde_json::json!({"$ref":"urn:test#/$defs/n1"}),
                vec![(1, 1, false)],
            ),
            (
                serde_json::json!({"allOf":[{"$ref":"urn:test#/$defs/n1"}],"dependentSchemas":{"x":{"$ref":"urn:test#/$defs/n1"}}}),
                vec![(1, 2, false), (1, 2, false)],
            ),
            (
                serde_json::json!({"properties":{"x":{"$ref":"urn:test#/$defs/n1"}},"items":{"$ref":"urn:test#/$defs/n1"},"contains":{"$ref":"urn:test#/$defs/n1"}}),
                vec![(1, 2, true), (1, 2, true), (1, 2, true)],
            ),
            (
                serde_json::json!({"const":{"$ref":"urn:inert#/$defs/n9"},"x-annotation":{"$ref":"urn:inert#/$defs/n9"}}),
                vec![],
            ),
        ];
        for (schema, mut expected) in fixtures {
            let original = schema.clone();
            let bundle = bundle(
                vec![Resource {
                    definitions: [("n0".into(), schema), ("n1".into(), Value::Bool(true))]
                        .into_iter()
                        .collect(),
                }],
                "urn:test#/$defs/n0",
            )
            .unwrap();
            let mut actual: Vec<_> = bundle.definitions[0]
                .edges
                .iter()
                .map(|e| (e.to, e.weight, e.descends))
                .collect();
            actual.sort_unstable();
            expected.sort_unstable();
            assert_eq!(actual, expected);
            if expected.is_empty() {
                assert_eq!(bundle.schemas[0], original);
            }
        }
    }
}

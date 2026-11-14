//! One closed static resource for value compilation. Original references are
//! already resolved; only private definition addresses are rewritten here.
use serde_json::{Map, Value};
use std::collections::HashMap;
pub(crate) const URI: &str = "urn:openbindings:bundle";
#[cfg(test)]
thread_local! {static COMPACTION:std::cell::Cell<bool>=const{std::cell::Cell::new(true)};}
#[cfg(test)]
pub(super) fn without_compaction<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            COMPACTION.set(self.0);
        }
    }
    let _restore = Restore(COMPACTION.replace(false));
    run()
}
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
    pub(crate) fn document(
        mut self,
        control: &dyn openbindings::WorkControl,
    ) -> Result<Value, openbindings::Cancelled> {
        #[cfg(not(test))]
        let removed = self.inline_leaves(control)?;
        #[cfg(test)]
        let removed = if COMPACTION.get() {
            self.inline_leaves(control)?
        } else {
            vec![false; self.names.len()]
        };
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
            Value::Object(
                self.names
                    .into_iter()
                    .zip(self.schemas)
                    .zip(removed)
                    .filter_map(|(definition, removed)| (!removed).then_some(definition))
                    .collect(),
            ),
        );
        Ok(Value::Object(root))
    }

    // Projection gives every schema position a definition. Leaving a separate
    // reference for each scalar property makes the backend retain a resolver,
    // placeholder and target for each one. Move shallow, single-use leaves into
    // their sole use instead. Two rounds cover scalar properties and array items
    // without turning long chains into deeply nested compiler input. The original
    // graph remains the conservative evaluation-depth proof, computed beforehand.
    fn inline_leaves(
        &mut self,
        control: &dyn openbindings::WorkControl,
    ) -> Result<Vec<bool>, openbindings::Cancelled> {
        let indices: HashMap<_, _> = self
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index))
            .collect();
        let mut incoming = vec![0usize; self.names.len()];
        incoming[self.entry as usize] = 1;
        for definition in &self.definitions {
            control.check()?;
            for edge in &definition.edges {
                incoming[edge.to as usize] += 1;
            }
        }
        let mut removed = vec![false; self.names.len()];
        for _ in 0..2 {
            let mut leaves = vec![false; self.names.len()];
            for (index, definition) in self.definitions.iter().enumerate() {
                control.check()?;
                leaves[index] = index != self.entry as usize
                    && !removed[index]
                    && incoming[index] == 1
                    && definition
                        .edges
                        .iter()
                        .all(|edge| removed[edge.to as usize])
                    && shallow(&self.schemas[index], 4);
            }
            for index in 0..self.schemas.len() {
                control.check()?;
                if removed[index] || leaves[index] {
                    continue;
                }
                let mut schema = std::mem::take(&mut self.schemas[index]);
                inline_schema(
                    &mut schema,
                    &indices,
                    &leaves,
                    &mut removed,
                    &mut self.schemas,
                );
                self.schemas[index] = schema;
            }
        }
        Ok(removed)
    }
}

fn shallow(value: &Value, remaining: usize) -> bool {
    match value {
        Value::Object(fields) => {
            remaining > 0 && fields.values().all(|v| shallow(v, remaining - 1))
        }
        Value::Array(items) => remaining > 0 && items.iter().all(|v| shallow(v, remaining - 1)),
        _ => true,
    }
}

fn inline_schema(
    value: &mut Value,
    indices: &HashMap<&str, usize>,
    leaves: &[bool],
    removed: &mut [bool],
    definitions: &mut [Value],
) {
    let Value::Object(schema) = value else { return };
    let target = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| indices.get(name))
        .copied();
    if schema.len() == 1
        && let Some(target) = target.filter(|&target| leaves[target])
    {
        *value = std::mem::take(&mut definitions[target]);
        removed[target] = true;
        return;
    }
    // Visit schema positions only; a reference-looking object in a literal or
    // annotation is data, and must retain its exact value.
    for (keyword, operand) in schema {
        match (keyword.as_str(), operand) {
            ("properties" | "patternProperties" | "dependentSchemas", Value::Object(children)) => {
                for child in children.values_mut() {
                    inline_schema(child, indices, leaves, removed, definitions);
                }
            }
            ("allOf" | "anyOf" | "oneOf" | "prefixItems", Value::Array(children)) => {
                for child in children {
                    inline_schema(child, indices, leaves, removed, definitions);
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
            ) => {
                inline_schema(child, indices, leaves, removed, definitions);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inlining_keeps_long_chains_flat_and_observes_cancellation() {
        let definitions: Map<String, Value> = (0..128)
            .map(|index| {
                let schema = if index == 127 {
                    Value::Bool(true)
                } else {
                    serde_json::json!({"$ref":format!("urn:test#/$defs/n{}", index + 1)})
                };
                (format!("n{index}"), schema)
            })
            .collect();
        let make = || {
            bundle(
                vec![Resource {
                    definitions: definitions.clone(),
                }],
                "urn:test#/$defs/n0",
            )
            .unwrap()
        };
        assert!(
            make()
                .document(&std::sync::atomic::AtomicBool::new(true))
                .is_err()
        );
        let document = make()
            .document(&std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        assert_eq!(document["$defs"].as_object().unwrap().len(), 126);
        assert!(
            jsonschema::validator_for(&document)
                .unwrap()
                .is_valid(&Value::Null)
        );
    }

    #[test]
    fn inlining_preserves_shared_targets_cycles_ref_siblings_and_literals() {
        let definitions = serde_json::json!({
            "n0":{"type":"object","properties":{
                "single":{"$ref":"urn:test#/$defs/n1"},
                "sharedA":{"$ref":"urn:test#/$defs/n2"},
                "sharedB":{"$ref":"urn:test#/$defs/n2"},
                "recursive":{"$ref":"urn:test#/$defs/n3"},
                "withSibling":{"$ref":"urn:test#/$defs/n4","maxLength":3},
                "literal":{"const":{"$ref":"#/$defs/n1"}}
            }},
            "n1":{"type":"string","minLength":2},
            "n2":{"type":"boolean"},
            "n3":{"type":"object","properties":{"next":{"$ref":"urn:test#/$defs/n3"}}},
            "n4":{"type":"string"}
        })
        .as_object()
        .unwrap()
        .clone();
        let bundle = bundle(vec![Resource { definitions }], "urn:test#/$defs/n0").unwrap();
        let document = bundle
            .document(&std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        let defs = document["$defs"].as_object().unwrap();
        assert!(!defs.contains_key("n1"));
        for retained in ["n0", "n2", "n3", "n4"] {
            assert!(defs.contains_key(retained));
        }
        let properties = &defs["n0"]["properties"];
        assert_eq!(
            properties["single"],
            serde_json::json!({"type":"string","minLength":2})
        );
        assert_eq!(properties["literal"]["const"]["$ref"], "#/$defs/n1");
        let validator = jsonschema::validator_for(&document).unwrap();
        assert!(
            validator.is_valid(
                &serde_json::json!({"single":"ok","sharedA":true,"recursive":{"next":{}}})
            )
        );
        assert!(!validator.is_valid(&serde_json::json!({"single":"x"})));
        assert!(!validator.is_valid(&serde_json::json!({"withSibling":"long"})));
    }
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

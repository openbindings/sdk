//! Source-position guidance, separate from the authoritative fixed-schema verdict.
//! Only documented validator-map, lookup and validity APIs are used. No rejected
//! instance is converted into a library diagnostic or an owned JSON tree.
use crate::{JsonKind, JsonRef};
use jsonschema::{Validator, ValidatorMap};
use openbindings_internal_json::backend::{self, FlatJson};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

struct Shape {
    pointer: String,
    container_pointer: String,
    properties: BTreeMap<String, usize>,
    patterns: Vec<(Validator<FlatJson>, usize)>,
    names: Option<usize>,
    additional: Option<usize>,
    prefix: Vec<usize>,
    items: Option<usize>,
    reference: Option<usize>,
    forbidden: bool,
}

/// Internal compiled location plan; compilation depends only on the pinned schema.
pub struct LocationMap {
    validators: ValidatorMap<FlatJson>,
    containers: ValidatorMap<FlatJson>,
    shapes: Vec<Shape>,
    root: usize,
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn local_pointer(reference: &str) -> Option<String> {
    let fragment = reference.strip_prefix('#')?;
    let mut decoded = Vec::with_capacity(fragment.len());
    let mut bytes = fragment.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' {
            let hi = (bytes.next()? as char).to_digit(16)?;
            let lo = (bytes.next()? as char).to_digit(16)?;
            (hi * 16 + lo) as u8
        } else {
            byte
        });
    }
    let pointer = String::from_utf8(decoded).ok()?;
    (pointer.is_empty() || pointer.starts_with('/')).then(|| format!("#{pointer}"))
}

impl LocationMap {
    pub fn compile(schema: &Value) -> Result<Self, String> {
        let validators = super::fixed_schema::options(BTreeMap::new())
            .build_map(schema)
            .map_err(|e| e.to_string())?;
        let mut keys: Vec<_> = validators.keys().map(str::to_owned).collect();
        keys.sort();
        let ids: HashMap<_, _> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        // Keep the original root intact: references inside non-descending
        // applicators (for example anyOf) must still see original constraints.
        // Container projections live in a collision-free definition beside it.
        let mut container_root = schema.clone();
        if !container_root.is_object() {
            container_root = serde_json::json!({});
        }
        let definitions = container_root
            .as_object_mut()
            .expect("object root")
            .entry("$defs")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or("invalid root definitions")?;
        let mut slot = "__location_containers".to_owned();
        while definitions.contains_key(&slot) {
            slot.push('_');
        }
        let container_prefix = format!("#/$defs/{}/$defs", escape(&slot));
        let mut container_schemas = serde_json::Map::new();
        let mut shapes = Vec::with_capacity(keys.len());
        for pointer in &keys {
            let schema = schema
                .pointer(&pointer[1..])
                .ok_or("missing mapped schema")?;
            let child = |suffix: &str| {
                ids.get(format!("{pointer}/{suffix}").as_str())
                    .copied()
                    .ok_or_else(|| format!("unmapped location constraint: {pointer}/{suffix}"))
            };
            let optional = |keyword: &str| schema.get(keyword).map(|_| child(keyword)).transpose();
            let mut properties = BTreeMap::new();
            if let Some(map) = schema.get("properties").and_then(Value::as_object) {
                for key in map.keys() {
                    properties.insert(key.clone(), child(&format!("properties/{}", escape(key)))?);
                }
            }
            let mut patterns = Vec::new();
            if let Some(map) = schema.get("patternProperties").and_then(Value::as_object) {
                for pattern in map.keys() {
                    let matcher = super::fixed_schema::compile(
                        &serde_json::json!({"pattern": pattern}),
                        BTreeMap::new(),
                    )?;
                    patterns.push((
                        matcher,
                        child(&format!("patternProperties/{}", escape(pattern)))?,
                    ));
                }
            }
            let prefix = schema
                .get("prefixItems")
                .and_then(Value::as_array)
                .map(|items| {
                    (0..items.len())
                        .map(|i| child(&format!("prefixItems/{i}")))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            // Non-pointer references are deliberate stops. The pinned-schema
            // inventory test rejects adding anchors or resource scopes unnoticed.
            let reference = schema
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(local_pointer)
                .and_then(|key| ids.get(key.as_str()).copied());
            let mut container = schema.clone();
            if let Some(object) = container.as_object_mut() {
                // Retain selectors and annotation coverage, but judge their
                // constraints only at the selected children. In particular do
                // not erase properties before additional/unevaluatedProperties,
                // or prefixItems before items/unevaluatedItems.
                for keyword in ["properties", "patternProperties"] {
                    if let Some(map) = object.get_mut(keyword).and_then(Value::as_object_mut) {
                        for value in map.values_mut() {
                            *value = Value::Bool(true);
                        }
                    }
                }
                if let Some(items) = object.get_mut("prefixItems").and_then(Value::as_array_mut) {
                    items.fill(Value::Bool(true));
                }
                for keyword in ["additionalProperties", "propertyNames", "items"] {
                    if let Some(value) = object.get_mut(keyword) {
                        *value = Value::Bool(true);
                    }
                }
                if let Some(target) = reference {
                    object.insert(
                        "$ref".into(),
                        Value::String(format!("{container_prefix}/{target}")),
                    );
                }
                // These projections inherit the original root's resource scope;
                // copying its identifier would register a competing resource.
                object.remove("$id");
            }
            let container_id = shapes.len();
            container_schemas.insert(container_id.to_string(), container);
            shapes.push(Shape {
                pointer: pointer.clone(),
                container_pointer: format!("{container_prefix}/{container_id}"),
                properties,
                patterns,
                names: optional("propertyNames")?,
                additional: optional("additionalProperties")?,
                prefix,
                items: optional("items")?,
                reference,
                forbidden: schema == &Value::Bool(false),
            });
        }
        let root = *ids.get("#").ok_or("missing root location validator")?;
        // build_map also exposes object-shaped annotations and property maps.
        // Only actual schemas reached by the location walk become projections.
        let mut reachable = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            let shape = &shapes[id];
            pending.extend(shape.properties.values().copied());
            pending.extend(shape.patterns.iter().map(|(_, target)| *target));
            pending.extend(shape.names);
            pending.extend(shape.additional);
            pending.extend(shape.prefix.iter().copied());
            pending.extend(shape.items);
            pending.extend(shape.reference);
        }
        container_schemas
            .retain(|key, _| reachable.contains(&key.parse::<usize>().expect("shape index")));
        definitions.insert(slot, serde_json::json!({"$defs":container_schemas}));
        let containers = super::fixed_schema::options(BTreeMap::new())
            .build_map(&container_root)
            .map_err(|e| e.to_string())?;
        for id in reachable {
            let shape = &shapes[id];
            if containers.get(&shape.container_pointer).is_none() {
                return Err(format!("unmapped container constraint: {}", shape.pointer));
            }
        }
        Ok(Self {
            validators,
            containers,
            shapes,
            root,
        })
    }

    fn valid(&self, id: usize, at: JsonRef<'_>) -> Result<bool, ()> {
        let validator = self.validators.get(&self.shapes[id].pointer).ok_or(())?;
        super::fixed_schema::bounded_valid(validator, at).map_err(|_| ())
    }

    fn frame<'a>(&self, at: JsonRef<'a>, seeds: Vec<usize>) -> Result<Frame<'a>, ()> {
        let mut shapes = Vec::new();
        for mut id in seeds {
            loop {
                if !jsonschema::ob_work::charge(1) {
                    return Err(());
                }
                if shapes.contains(&id) {
                    break;
                }
                shapes.push(id);
                match self.shapes[id].reference {
                    Some(target) => id = target,
                    None => break,
                }
            }
        }
        let mut container_failed = false;
        for &id in &shapes {
            let validator = self
                .containers
                .get(&self.shapes[id].container_pointer)
                .ok_or(())?;
            if !super::fixed_schema::bounded_valid(validator, at).map_err(|_| ())? {
                container_failed = true;
                break;
            }
        }
        Ok(Frame {
            at,
            shapes,
            index: 0,
            name: true,
            found: false,
            container_failed,
        })
    }

    // At most one member/element's candidates are materialized at a time. The
    // frame stack scales with depth, not the number of children in the instance.
    fn next<'a>(&self, frame: &mut Frame<'a>) -> Result<Option<(JsonRef<'a>, Vec<usize>)>, ()> {
        if frame.shapes.iter().all(|&id| {
            let s = &self.shapes[id];
            match frame.at.kind() {
                JsonKind::Object => {
                    s.properties.is_empty()
                        && s.patterns.is_empty()
                        && s.names.is_none()
                        && s.additional.is_none()
                }
                JsonKind::Array => s.prefix.is_empty() && s.items.is_none(),
                _ => true,
            }
        }) {
            return Ok(None);
        }
        loop {
            if frame.index >= frame.at.len().unwrap_or(0) {
                return Ok(None);
            }
            if !jsonschema::ob_work::charge(1) {
                return Err(());
            }
            let mut candidates = Vec::new();
            let at = if frame.at.kind() == JsonKind::Object {
                let Some(member) = backend::member_at(frame.at, frame.index) else {
                    return Ok(None);
                };
                for &id in &frame.shapes {
                    let shape = &self.shapes[id];
                    if frame.name {
                        if let Some(names) = shape.names {
                            candidates.push(names);
                        }
                    } else {
                        let property = member
                            .name
                            .as_str()
                            .and_then(|name| shape.properties.get(name));
                        let mut matched = property.is_some();
                        if let Some(&property) = property {
                            candidates.push(property);
                        }
                        for (matcher, target) in &shape.patterns {
                            if super::fixed_schema::bounded_valid(matcher, member.name)
                                .map_err(|_| ())?
                            {
                                matched = true;
                                candidates.push(*target);
                            }
                        }
                        if !matched && let Some(additional) = shape.additional {
                            candidates.push(additional);
                        }
                    }
                }
                if frame.name {
                    frame.name = false;
                    member.name
                } else {
                    frame.name = true;
                    frame.index += 1;
                    member.value
                }
            } else if frame.at.kind() == JsonKind::Array {
                let Some(element) = frame.at.element(frame.index) else {
                    return Ok(None);
                };
                for &id in &frame.shapes {
                    let shape = &self.shapes[id];
                    if let Some(&prefix) = shape.prefix.get(frame.index) {
                        candidates.push(prefix);
                    } else if let Some(items) = shape.items {
                        candidates.push(items);
                    }
                }
                frame.index += 1;
                element
            } else {
                return Ok(None);
            };
            let mut failed = Vec::new();
            for candidate in candidates {
                if !jsonschema::ob_work::charge(1) {
                    return Err(());
                }
                if !failed.contains(&candidate) && !self.valid(candidate, at)? {
                    failed.push(candidate);
                }
            }
            if !failed.is_empty() {
                // A forbidden member has no admissible value. Keep its original
                // name token, as for propertyNames, instead of inventing a span.
                let at = if frame.at.kind() == JsonKind::Object
                    && frame.name
                    && failed.iter().all(|&id| self.shapes[id].forbidden)
                {
                    backend::member_at(frame.at, frame.index - 1)
                        .ok_or(())?
                        .name
                } else {
                    at
                };
                return Ok(Some((at, failed)));
            }
        }
    }

    /// Locate known failure without changing that verdict. `false` from `offer`
    /// closes the findings allowance. Return whether work/depth limited guidance.
    pub fn locate<'a>(
        &self,
        root: JsonRef<'a>,
        mut offer: impl FnMut(JsonRef<'a>) -> bool,
    ) -> bool {
        let Ok(frame) = self.frame(root, vec![self.root]) else {
            offer(root);
            return true;
        };
        let mut stack = vec![frame];
        while let Some(frame) = stack.last_mut() {
            match self.next(frame) {
                Ok(Some((at, failed))) => {
                    frame.found = true;
                    if stack.len() >= 1024 {
                        offer(at);
                        return true;
                    }
                    match self.frame(at, failed) {
                        Ok(child) => stack.push(child),
                        Err(()) => {
                            offer(at);
                            return true;
                        }
                    }
                }
                Ok(None) => {
                    let frame = stack.pop().expect("live frame");
                    if (!frame.found || frame.container_failed) && !offer(frame.at) {
                        return false;
                    }
                }
                Err(()) => {
                    offer(frame.at);
                    return true;
                }
            }
        }
        false
    }
}
struct Frame<'a> {
    at: JsonRef<'a>,
    shapes: Vec<usize>,
    index: usize,
    name: bool,
    found: bool,
    container_failed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonValue;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn locate(schema: Value, text: &str, work: usize) -> (Vec<String>, bool) {
        let map = LocationMap::compile(&schema).unwrap();
        let value = JsonValue::parse(text).unwrap();
        let mut paths = Vec::new();
        let mut limited = false;
        backend::fixed_strings(|| {
            let _ = jsonschema::ob_work::bounded(work, 1024, || {
                limited = map.locate(value.view(), |at| {
                    paths.push(at.location().pointer.unwrap());
                    true
                });
            });
        });
        (paths, limited)
    }
    #[test]
    fn overlapping_applicators_refs_names_and_array_elements_keep_deepest_positions() {
        let schema = json!({
            "$defs":{"a/b~c":{"type":"array", "prefixItems":[{"type":"string"}], "items":{"type":"integer"}}},
            "type":"object", "propertyNames":{"pattern":"^[a-z/]+$"},
            "properties":{"arr":{"$ref":"#/$defs/a~1b~0c"}, "x":{"type":"array","items":{"type":"string"}}},
            "patternProperties":{"^x":{"items":{"minLength":2}}, "x$":{"items":{"pattern":"^z"}}},
            "additionalProperties":false
        });
        let (paths, limited) = locate(
            schema,
            r#"{"arr":[false,"bad",3],"x":["a",null],"/bad name":0,"extra":true}"#,
            2_000_000,
        );
        assert!(!limited);
        assert_eq!(
            paths,
            [
                "/arr/0",
                "/arr/1",
                "/x/0",
                "/x/1",
                "/~1bad name",
                "/~1bad name",
                "/extra"
            ]
        );
    }
    #[test]
    fn missing_members_container_constraints_and_unions_stop_at_container() {
        let schema = json!({"properties":{
            "a":{"required":["missing"]},
            "b":{"uniqueItems":true,"items":{"type":"integer"}},
            "c":{"anyOf":[{"items":{"type":"string"}},{"type":"boolean"}]}
        }});
        assert_eq!(
            locate(schema, r#"{"a":{},"b":[1,1],"c":[0]}"#, 10000).0,
            ["/a", "/b", "/c"]
        );
    }
    #[test]
    fn independent_container_assertions_survive_failing_children() {
        let schema = json!({"properties": {
            "required": {"required":["missing"],"properties":{"x":{"type":"string"}}},
            "unique": {"uniqueItems":true,"items":{"type":"string"}},
            "size": {"maxItems":1,"items":{"type":"string"}},
            "union": {"anyOf":[{"$ref":"#/$defs/required"},{"type":"boolean"}],"properties":{"x":{"type":"string"}}}
        },"$defs":{"required":{"required":["missing"]}}});
        let (paths, limited) = locate(
            schema,
            r#"{"required":{"x":0},"unique":[0,0],"size":[0,0],"union":{"x":0}}"#,
            10000,
        );
        assert!(!limited);
        assert_eq!(
            paths,
            [
                "/required/x",
                "/required",
                "/unique/0",
                "/unique/1",
                "/unique",
                "/size/0",
                "/size/1",
                "/size",
                "/union/x",
                "/union"
            ]
        );
    }
    #[test]
    fn container_projections_keep_reference_scope_and_evaluated_positions() {
        let schema = json!({"$defs":{"inner":{"properties":{"a":{"type":"boolean"}}}},
            "$ref":"#/$defs/inner", "unevaluatedProperties":false});
        assert_eq!(locate(schema, r#"{"a":0}"#, 10000).0, ["/a"]);
        let schema = json!({"prefixItems":[{"type":"boolean"}],"items":{"type":"integer"},"unevaluatedItems":false});
        assert_eq!(locate(schema, "[0,false]", 10000).0, ["/0", "/1"]);
        let schema = json!({"properties":{"x":{"type":"boolean"}},"anyOf":[{"$ref":"#/$defs/branch"}],"$defs":{"branch":{"properties":{"x":{"type":"boolean"}}}}});
        assert_eq!(locate(schema, r#"{"x":0}"#, 10000).0, ["/x", ""]);
    }
    #[test]
    fn local_ref_siblings_and_percent_escaped_pointer_are_conjunctive() {
        let schema = json!({"$defs":{"a b":{"properties":{"x":{"type":"string"}}}},
            "$ref":"#/$defs/a%20b", "properties":{"y":{"type":"boolean"}}});
        assert_eq!(locate(schema, r#"{"x":0,"y":0}"#, 10000).0, ["/x", "/y"]);
    }
    #[test]
    fn work_limit_retains_deepest_established_failure() {
        let schema = json!({"properties":{"a":{"properties":{"b":{"type":"string"}}}}});
        let mut saw_child_stop = false;
        for work in 0..40 {
            let (paths, limited) = locate(schema.clone(), r#"{"a":{"b":0}}"#, work);
            assert_eq!(paths.len(), 1);
            assert!(["", "/a", "/a/b"].contains(&paths[0].as_str()));
            saw_child_stop |= limited && paths[0] == "/a";
        }
        assert!(saw_child_stop);
    }
    #[test]
    fn pinned_schema_keywords_have_explicit_location_policy() {
        // Only visit schema positions, not property names, const data or annotations.
        // New keywords must be classified here after reviewing their location policy.
        let schema: Value =
            serde_json::from_str(include_str!("../schemas/openbindings.schema.json")).unwrap();
        let descend = [
            "$ref",
            "properties",
            "patternProperties",
            "additionalProperties",
            "propertyNames",
            "prefixItems",
            "items",
        ];
        let stop = [
            "$schema",
            "$id",
            "title",
            "description",
            "type",
            "required",
            "pattern",
            "minLength",
            "minItems",
            "uniqueItems",
            "enum",
            "minimum",
            "maximum",
        ];
        let mut seen = BTreeSet::new();
        let mut stack = vec![&schema];
        while let Some(schema) = stack.pop() {
            let Some(map) = schema.as_object() else {
                continue;
            };
            for (key, value) in map {
                seen.insert(key.as_str());
                match key.as_str() {
                    "$defs" | "properties" | "patternProperties" => {
                        stack.extend(value.as_object().unwrap().values())
                    }
                    "anyOf" | "prefixItems" => stack.extend(value.as_array().unwrap()),
                    "additionalProperties" | "propertyNames" | "items" => stack.push(value),
                    "$ref" => assert!(
                        value.as_str().and_then(local_pointer).is_some(),
                        "review non-pointer reference"
                    ),
                    key if descend.contains(&key) || stop.contains(&key) => {}
                    _ => panic!("unclassified fixed-schema keyword: {key}"),
                }
            }
        }
        // anyOf deliberately stops at its container; $defs is reached by $ref.
        assert!(seen.contains("anyOf") && seen.contains("$defs"));
        LocationMap::compile(&schema).unwrap();
    }
}

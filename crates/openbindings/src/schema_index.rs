//! The normative schema-position walk. Unknown keywords remain opaque.
use crate::{JsonKind, JsonRef, JsonValue, uri};
use openbindings_internal_json::backend::node_id;
use std::collections::{BTreeMap, HashMap};

pub(crate) const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
pub(crate) struct SchemaNode {
    pub value: JsonValue,
    pub depth: usize,
    pub resource: usize,
    pub parent_resource: usize,
    pub obi_position: bool,
    pub base: Option<String>,
}
#[derive(Default)]
pub(crate) struct SchemaIndex {
    pub nodes: Vec<SchemaNode>,
    pub positions: HashMap<usize, usize>,
    pub anchors: BTreeMap<(usize, String), Vec<(JsonValue, &'static str)>>,
    pub identifiers: BTreeMap<String, Vec<JsonValue>>,
}
pub(crate) fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic()
                || c == b'_'
                || (i > 0 && (c.is_ascii_digit() || c == b'-' || c == b'.'))
        })
}
pub(crate) fn schema_children(value: JsonRef<'_>) -> Vec<JsonRef<'_>> {
    let mut out = Vec::new();
    if let Some(members) = value.members() {
        for member in members {
            let child = member.value;
            match member.name.as_str().unwrap_or("") {
                "$defs" | "properties" | "patternProperties" | "dependentSchemas"
                | "definitions" | "dependencies" => {
                    if let Some(entries) = child.members() {
                        out.extend(
                            entries.map(|m| m.value).filter(|v| {
                                matches!(v.kind(), JsonKind::Object | JsonKind::Boolean)
                            }),
                        );
                    }
                }
                "additionalProperties"
                | "propertyNames"
                | "unevaluatedProperties"
                | "items"
                | "contains"
                | "unevaluatedItems"
                | "not"
                | "if"
                | "then"
                | "else"
                | "contentSchema" => {
                    if matches!(child.kind(), JsonKind::Object | JsonKind::Boolean) {
                        out.push(child);
                    }
                }
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                    if let Some(entries) = child.elements() {
                        out.extend(
                            entries.filter(|v| {
                                matches!(v.kind(), JsonKind::Object | JsonKind::Boolean)
                            }),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    out
}
impl SchemaIndex {
    pub fn build(document: &JsonValue) -> Self {
        let mut roots = Vec::new();
        if let Some(schemas) = document.get("schemas").and_then(|v| v.members()) {
            roots.extend(schemas.map(|m| m.value));
        }
        if let Some(operations) = document.get("operations").and_then(|v| v.members()) {
            for op in operations {
                for side in ["input", "output"] {
                    if let Some(v) = op.value.get(side) {
                        roots.push(v);
                    }
                }
            }
        }
        Self::build_roots(roots, None)
    }
    pub fn supplied(value: &JsonValue, retrieval_uri: &str) -> Self {
        Self::build_roots(vec![value.view()], Some(retrieval_uri.to_owned()))
    }
    fn build_roots(roots: Vec<JsonRef<'_>>, base: Option<String>) -> Self {
        let mut stack: Vec<_> = roots
            .into_iter()
            .rev()
            .map(|v| (v, 0, 0, base.clone()))
            .collect();
        let mut out = Self::default();
        while let Some((value, depth, parent_resource, parent_base)) = stack.pop() {
            let obi_position = parent_resource == 0
                && matches!(value.kind(), JsonKind::Object | JsonKind::Boolean);
            let id = value.get("$id");
            let resource = if id.is_some() {
                out.nodes.len() + 1
            } else {
                parent_resource
            };
            let base = if let Some(id) = id {
                id.as_str()
                    .and_then(|s| uri::compared_id(parent_base.as_deref(), s))
            } else {
                parent_base
            };
            if id.is_some()
                && let Some(base) = &base
            {
                out.identifiers
                    .entry(base.clone())
                    .or_default()
                    .push(value.to_owned());
            }
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if let Some(name) = value
                    .get(keyword)
                    .and_then(|v| v.as_str())
                    .filter(|s| plain_name(s))
                {
                    out.anchors
                        .entry((resource, name.into()))
                        .or_default()
                        .push((value.to_owned(), keyword));
                }
            }
            for child in schema_children(value).into_iter().rev() {
                stack.push((child, depth + 1, resource, base.clone()));
            }
            let owned = value.to_owned();
            out.positions.insert(node_id(&owned), out.nodes.len());
            out.nodes.push(SchemaNode {
                value: owned,
                depth,
                resource,
                parent_resource,
                obi_position,
                base,
            });
        }
        out
    }
    pub fn same_document(&self, document: &JsonValue, reference: &str) -> SameDocument {
        let Some(fragment) = uri::decode_fragment(reference.strip_prefix('#').unwrap_or(reference))
        else {
            return SameDocument::Missing;
        };
        if fragment.is_empty() {
            return SameDocument::Missing;
        }
        if fragment.starts_with('/') {
            let Some(value) = document.at(&fragment) else {
                return SameDocument::Missing;
            };
            return match self
                .positions
                .get(&node_id(&value.to_owned()))
                .map(|&i| &self.nodes[i])
            {
                Some(node) if node.obi_position => SameDocument::Found(node.value.clone()),
                _ => SameDocument::Missing,
            };
        }
        match self
            .anchors
            .get(&(0, fragment))
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            [] => SameDocument::Missing,
            [(value, _)] => SameDocument::Found(value.clone()),
            _ => SameDocument::Ambiguous,
        }
    }
}
pub(crate) enum SameDocument {
    Found(JsonValue),
    Missing,
    Ambiguous,
}

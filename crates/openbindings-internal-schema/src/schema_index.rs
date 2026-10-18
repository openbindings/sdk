//! The normative schema-position walk. Unknown keywords remain opaque.
use crate::{JsonKind, JsonRef, JsonValue, uri};
use openbindings_internal_json::backend::{node_id, ref_id};
use std::collections::{BTreeMap, HashMap};

pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
pub struct SchemaNode {
    pub value: JsonValue,
    pub depth: usize,
    pub resource: usize,
    pub parent_resource: usize,
    pub obi_position: bool,
    pub base: Option<String>,
}
impl std::fmt::Debug for SchemaNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaNode").finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct SchemaIndex {
    pub nodes: Vec<SchemaNode>,
    pub positions: HashMap<usize, usize>,
    pub anchors: BTreeMap<(usize, String), Vec<(JsonValue, &'static str)>>,
    pub identifiers: BTreeMap<String, Vec<JsonValue>>,
}
impl std::fmt::Debug for SchemaIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaIndex").finish_non_exhaustive()
    }
}

pub fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic()
                || c == b'_'
                || (i > 0 && (c.is_ascii_digit() || c == b'-' || c == b'.'))
        })
}
// the test derives these selectors from the pinned originals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaSelector {
    Direct,
    Map,
    Array,
    MapOrStringArray,
}
pub const SCHEMA_SELECTORS: &[(&str, SchemaSelector)] = &[
    ("$defs", SchemaSelector::Map),
    ("properties", SchemaSelector::Map),
    ("patternProperties", SchemaSelector::Map),
    ("dependentSchemas", SchemaSelector::Map),
    ("definitions", SchemaSelector::Map),
    ("dependencies", SchemaSelector::MapOrStringArray),
    ("additionalProperties", SchemaSelector::Direct),
    ("propertyNames", SchemaSelector::Direct),
    ("unevaluatedProperties", SchemaSelector::Direct),
    ("items", SchemaSelector::Direct),
    ("contains", SchemaSelector::Direct),
    ("unevaluatedItems", SchemaSelector::Direct),
    ("not", SchemaSelector::Direct),
    ("if", SchemaSelector::Direct),
    ("then", SchemaSelector::Direct),
    ("else", SchemaSelector::Direct),
    ("contentSchema", SchemaSelector::Direct),
    ("allOf", SchemaSelector::Array),
    ("anyOf", SchemaSelector::Array),
    ("oneOf", SchemaSelector::Array),
    ("prefixItems", SchemaSelector::Array),
];
pub fn schema_children(value: JsonRef<'_>) -> Vec<JsonRef<'_>> {
    let mut out = Vec::new();
    if let Some(members) = value.members() {
        for member in members {
            let Some((_, selector)) = SCHEMA_SELECTORS
                .iter()
                .find(|(name, _)| Some(*name) == member.name.as_str())
            else {
                continue;
            };
            let is_schema =
                |v: &JsonRef<'_>| matches!(v.kind(), JsonKind::Object | JsonKind::Boolean);
            match selector {
                SchemaSelector::Map | SchemaSelector::MapOrStringArray => {
                    if let Some(entries) = member.value.members() {
                        out.extend(entries.map(|m| m.value).filter(is_schema));
                    }
                }
                SchemaSelector::Array => {
                    if let Some(entries) = member.value.elements() {
                        out.extend(entries.filter(is_schema));
                    }
                }
                SchemaSelector::Direct => {
                    if is_schema(&member.value) {
                        out.push(member.value);
                    }
                }
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
                for direction in ["input", "output"] {
                    if let Some(v) = op.value.get(direction) {
                        roots.push(v);
                    }
                }
            }
        }
        Self::build_roots(roots, None, false, true)
    }
    pub fn supplied(value: &JsonValue, retrieval_uri: &str) -> Self {
        Self::build_roots(
            vec![value.view()],
            Some(retrieval_uri.to_owned()),
            true,
            true,
        )
    }
    /// A supplied resource's schema nodes alone: the same walk, order, depths
    /// and resource numbering as [`Self::supplied`], without bases, positions,
    /// identifiers or anchors. A schema space needs only these.
    pub fn supplied_nodes(value: &JsonValue) -> Self {
        Self::build_roots(vec![value.view()], None, true, false)
    }
    fn build_roots(
        roots: Vec<JsonRef<'_>>,
        base: Option<String>,
        ordinary: bool,
        full: bool,
    ) -> Self {
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
            let base = if !full {
                None
            } else if let Some(id) = id {
                id.as_str().and_then(|s| {
                    if ordinary {
                        crate::input_policy::resolve_identifier(parent_base.as_deref(), s)
                    } else {
                        uri::compared_id(parent_base.as_deref(), s)
                    }
                })
            } else {
                parent_base
            };
            if full
                && id.is_some()
                && let Some(base) = &base
            {
                out.identifiers
                    .entry(base.clone())
                    .or_default()
                    .push(value.to_owned());
            }
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if full
                    && let Some(name) = value
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
            if full {
                out.positions.insert(node_id(&owned), out.nodes.len());
            }
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
            return match self.positions.get(&ref_id(value)).map(|&i| &self.nodes[i]) {
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
#[derive(Debug)]
pub enum SameDocument {
    Found(JsonValue),
    Missing,
    Ambiguous,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn many_same_document_references_keep_the_target() {
        // exercise the normative OBI-12 route at all work-count sizes.
        // The arena tests count member probes in the shared pointer evaluator.
        for n in [10_000, 20_000, 40_000] {
            let mut text = String::from("{\"schemas\":{");
            for i in 0..n {
                if i > 0 {
                    text.push(',');
                }
                write!(text, "\"s{i}\":{{\"$ref\":\"#/schemas/s{}\"}}", n - 1).unwrap();
            }
            text.push_str("}}");
            let document = JsonValue::parse(text).unwrap();
            let index = SchemaIndex::build(&document);
            assert_eq!(index.nodes.len(), n);
            let reference = format!("#/schemas/s{}", n - 1);
            let target = document.at(&reference[1..]).unwrap().to_owned();
            for node in &index.nodes {
                assert_eq!(
                    node.value.get("$ref").unwrap().as_str(),
                    Some(reference.as_str())
                );
                let SameDocument::Found(found) = index.same_document(&document, &reference) else {
                    panic!("same-document reference lost its target");
                };
                assert_eq!(node_id(&found), node_id(&target));
            }
            assert!(matches!(
                index.same_document(&document, "#/schemas/absent"),
                SameDocument::Missing
            ));
        }
    }

    #[test]
    fn supplied_nodes_walk_the_supplied_positions() {
        let nested = JsonValue::parse(
            r##"{"$defs":{"a":{"$id":"nested","$anchor":"x","items":[{"$id":"#bad"}],"properties":{"p":{"$id":"https://other.test/p","not":true}}}},"allOf":[{"$ref":"#x"},false]}"##,
        )
        .unwrap();
        let resources = crate::space::embedded_resources()
            .iter()
            .map(|r| (r.document.clone(), r.uri.clone()))
            .chain([(nested, "https://supplied.test/r".to_owned())]);
        for (value, uri) in resources {
            let (full, nodes) = (
                SchemaIndex::supplied(&value, &uri),
                SchemaIndex::supplied_nodes(&value),
            );
            assert_eq!(full.nodes.len(), nodes.nodes.len(), "{uri}");
            for (a, b) in full.nodes.iter().zip(&nodes.nodes) {
                assert_eq!(node_id(&a.value), node_id(&b.value));
                assert_eq!(
                    (a.depth, a.resource, a.parent_resource, a.obi_position),
                    (b.depth, b.resource, b.parent_resource, b.obi_position)
                );
                assert!(b.base.is_none());
            }
            assert!(
                nodes.positions.is_empty()
                    && nodes.anchors.is_empty()
                    && nodes.identifiers.is_empty()
            );
        }
    }

    #[test]
    fn pointer_escapes_arrays_and_resource_boundaries() {
        // indexing must leave RFC 6901 and OBI-position semantics intact.
        let document = JsonValue::parse(
            r##"{"schemas":{
            "a/~":{"allOf":[true,{"properties":{"~1":false}}]},
            "a\u002f~":false,
            "resource":{"$id":"https://example.test/r","properties":{"child":true}},
            "space name":true
        },"x-opaque":{"schema":true}}"##,
        )
        .unwrap();
        let index = SchemaIndex::build(&document);
        for reference in [
            "#/schemas/a~1~0/allOf/0",
            "#%2Fschemas%2Fa~1~0%2FallOf%2F1%2Fproperties%2F~01",
            "#/schemas/resource",
            "#/schemas/space%20name",
        ] {
            assert!(
                matches!(
                    index.same_document(&document, reference),
                    SameDocument::Found(_)
                ),
                "{reference}"
            );
        }
        let SameDocument::Found(first) = index.same_document(&document, "#/schemas/a~1~0") else {
            panic!()
        };
        assert_eq!(first.kind(), JsonKind::Object);
        for reference in [
            "#/schemas/a~1~0/allOf/01",
            "#/schemas/a~1~0/allOf/-",
            "#/schemas/a~2",
            "#/schemas/missing",
            "#/schemas",
            "#/schemas/resource/properties/child",
            "#/x-opaque/schema",
            "#%FF",
        ] {
            assert!(
                matches!(
                    index.same_document(&document, reference),
                    SameDocument::Missing
                ),
                "{reference}"
            );
        }
    }
}

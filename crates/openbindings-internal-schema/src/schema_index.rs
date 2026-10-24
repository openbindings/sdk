//! The normative schema-position walk. Unknown keywords remain opaque.
use crate::{JsonKind, JsonRef, JsonValue, identifiers::Identifiers};
use openbindings_internal_json::backend::{node, ref_id};
use std::sync::{Arc, OnceLock};

pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
/// One schema position. Its value is [`SchemaIndex::value`]; a schema whose
/// resource it begins has [`SchemaIndex::parent_resource`].
#[derive(Clone, Copy)]
pub struct SchemaNode {
    /// The schema's node in the indexed source.
    pub id: u32,
    pub depth: u32,
    pub resource: u32,
}
impl std::fmt::Debug for SchemaNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaNode").finish_non_exhaustive()
    }
}

/// One plain-name declaration: an `$anchor` or `$dynamicAnchor` value.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub resource: u32,
    /// The declaring schema, as an index into [`SchemaIndex::nodes`].
    pub node: u32,
    /// The name's string token in the indexed source.
    pub name: u32,
    pub dynamic: bool,
}

#[derive(Default)]
pub struct SchemaIndex {
    /// The indexed value; every node is in its source.
    source: Option<JsonValue>,
    pub nodes: Vec<SchemaNode>,
    /// Each resource after the first and the resource enclosing it, by
    /// resource number.
    parent_resources: Vec<(u32, u32)>,
    /// Indices into `nodes`, by source node; built when first asked for.
    positions: OnceLock<Vec<u32>>,
    /// Whether positions are kept: not for a supplied resource's nodes alone.
    located: bool,
    /// Plain names by resource, then name; one name's declarations in walk order.
    pub anchors: Vec<Anchor>,
    /// Each `$id` declaration whose compared identifier another declaration
    /// shares: the identifier's rank and the declaring schema, in the order of
    /// the identifier strings, one identifier's in walk order. A document's
    /// index only.
    identifiers: Vec<(u32, u32)>,
    /// Each `$id` declaration with a base and that base, in walk order. A
    /// supplied resource's index only.
    bases: Vec<(u32, Arc<str>)>,
}
impl std::fmt::Debug for SchemaIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaIndex").finish_non_exhaustive()
    }
}
// How the walk assigns bases: none, a supplied resource's resolved strings, or
// a document's compared identifiers. A supplied resource's base is listed when
// the walk meets its `$id`, so by increasing resource number; a document's
// resource has the handle of its compared identifier, if any, which the walk
// lists with the declaring schema.
enum Bases<'a> {
    None,
    Ordinary(Vec<(u32, Option<Arc<str>>)>, Arc<str>),
    Compared(Box<Identifiers<'a>>),
}
fn base_of<T>(bases: &[(u32, Option<T>)], resource: u32) -> Option<&T> {
    let at = bases
        .binary_search_by_key(&resource, |&(r, _)| r)
        .expect("an enclosing resource is listed");
    bases[at].1.as_ref()
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
/// Each schema a schema position holds, in document order, for tests.
#[cfg(test)]
pub fn schema_children(value: JsonRef<'_>) -> Vec<JsonRef<'_>> {
    let mut out = Vec::new();
    each_schema_child(value, |child| out.push(child));
    out
}
/// Visit each schema a schema position holds, in document order.
pub fn each_schema_child<'a>(value: JsonRef<'a>, mut visit: impl FnMut(JsonRef<'a>)) {
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
                        entries
                            .map(|m| m.value)
                            .filter(is_schema)
                            .for_each(&mut visit);
                    }
                }
                SchemaSelector::Array => {
                    if let Some(entries) = member.value.elements() {
                        entries.filter(is_schema).for_each(&mut visit);
                    }
                }
                SchemaSelector::Direct => {
                    if is_schema(&member.value) {
                        visit(member.value);
                    }
                }
            }
        }
    }
}
impl SchemaIndex {
    pub fn build(document: &JsonValue) -> Self {
        let schemas = document
            .get("schemas")
            .and_then(|v| v.members())
            .into_iter()
            .flatten()
            .map(|m| m.value);
        let operations = document
            .get("operations")
            .and_then(|v| v.members())
            .into_iter()
            .flatten()
            .flat_map(|op| {
                ["input", "output"]
                    .into_iter()
                    .filter_map(move |d| op.value.get(d))
            });
        Self::build_roots(
            document,
            schemas.chain(operations),
            // A document's identifiers are texts of it, so its length bounds them.
            Bases::Compared(Box::new(Identifiers::new(document.original_source().len()))),
        )
    }
    pub fn supplied(value: &JsonValue, retrieval_uri: &str) -> Self {
        Self::build_roots(
            value,
            [value.view()],
            Bases::Ordinary(Vec::new(), Arc::from(retrieval_uri)),
        )
    }
    /// A supplied resource's schema nodes alone: the same walk, order, depths
    /// and resource numbering as [`Self::supplied`], without bases, positions,
    /// identifiers or anchors. A schema space needs only these.
    pub fn supplied_nodes(value: &JsonValue) -> Self {
        Self::build_roots(value, [value.view()], Bases::None)
    }
    // Roots are walked one at a time, each depth first in document order. A
    // pending schema is its node and its parent's index, from which its depth,
    // resource and base follow.
    fn build_roots<'a>(
        source: &'a JsonValue,
        roots: impl IntoIterator<Item = JsonRef<'a>>,
        mut bases: Bases<'a>,
    ) -> Self {
        let full = !matches!(bases, Bases::None);
        let mut out = Self {
            source: Some(source.clone()),
            ..Self::default()
        };
        let mut compared = Vec::new();
        let mut stack: Vec<(u32, u32)> = Vec::new();
        for root in roots {
            stack.push((ref_id(root) as u32, u32::MAX));
            while let Some((id, parent)) = stack.pop() {
                let value = node(source, id as usize).expect("a node of the source");
                let (depth, parent_resource) = match out.nodes.get(parent as usize) {
                    Some(parent) => (parent.depth + 1, parent.resource),
                    None => (0, 0),
                };
                let index =
                    u32::try_from(out.nodes.len()).expect("schemas are bounded by the node count");
                let declared = value.get("$id");
                let resource = if declared.is_some() {
                    out.parent_resources.push((index + 1, parent_resource));
                    index + 1
                } else {
                    parent_resource
                };
                if let Some(declared) = declared {
                    let text = declared.as_str();
                    match &mut bases {
                        Bases::None => {}
                        Bases::Ordinary(of, initial) => {
                            let parent_base = match parent_resource {
                                0 => Some(&**initial),
                                r => base_of(of, r).map(|b| &**b),
                            };
                            let base = text
                                .and_then(|s| {
                                    crate::input_policy::resolve_identifier(parent_base, s)
                                })
                                .map(Arc::<str>::from);
                            if let Some(base) = &base {
                                out.bases.push((index, base.clone()));
                            }
                            of.push((resource, base));
                        }
                        Bases::Compared(ids) => {
                            // Resource `r` begins at schema `r - 1`; the handles
                            // of compared declarations follow the walk.
                            let parent_base = match parent_resource {
                                0 => None,
                                r => compared.binary_search(&(r - 1)).ok().map(|h| h as u32),
                            };
                            // Each compared declaration's handle is the next.
                            if text.and_then(|s| ids.compared(parent_base, s)).is_some() {
                                compared.push(index);
                            }
                        }
                    }
                }
                for (keyword, dynamic) in [("$anchor", false), ("$dynamicAnchor", true)] {
                    if full
                        && let Some(name) = value.get(keyword)
                        && name.as_str().is_some_and(plain_name)
                    {
                        out.anchors.push(Anchor {
                            resource,
                            node: index,
                            name: ref_id(name) as u32,
                            dynamic,
                        });
                    }
                }
                // Push children in reverse so they are visited in document order.
                let pending = stack.len();
                each_schema_child(value, |child| stack.push((ref_id(child) as u32, index)));
                stack[pending..].reverse();
                out.nodes.push(SchemaNode {
                    id,
                    depth,
                    resource,
                });
            }
        }
        drop(stack);
        out.located = full;
        let mut anchors = std::mem::take(&mut out.anchors);
        anchors.sort_by(|a, b| {
            a.resource
                .cmp(&b.resource)
                .then_with(|| out.anchor_name(a).cmp(out.anchor_name(b)))
        });
        out.anchors = anchors;
        if let Bases::Compared(ids) = bases {
            out.identifiers = ids
                .duplicates()
                .into_iter()
                .map(|(rank, handle)| (rank, compared[handle as usize]))
                .collect();
        }
        out
    }
    /// A schema's value.
    pub fn value(&self, node: &SchemaNode) -> JsonRef<'_> {
        node_of(self.source.as_ref(), node.id)
    }
    /// A schema's value, retained.
    pub fn owned(&self, node: &SchemaNode) -> JsonValue {
        self.value(node).to_owned()
    }
    /// The resource enclosing the schema at `index`: for a schema that
    /// begins a resource, the one around it.
    pub fn parent_resource(&self, index: usize) -> u32 {
        let resource = self.nodes[index].resource;
        if resource as usize != index + 1 {
            return resource;
        }
        let at = self
            .parent_resources
            .binary_search_by_key(&resource, |&(r, _)| r)
            .expect("a resource begun by a schema is listed");
        self.parent_resources[at].1
    }
    /// Whether the schema at `index` is at an OBI position: in the document
    /// resource and an object or boolean.
    pub fn obi_position(&self, index: usize) -> bool {
        self.parent_resource(index) == 0
            && matches!(
                self.value(&self.nodes[index]).kind(),
                JsonKind::Object | JsonKind::Boolean
            )
    }
    /// The index in [`Self::nodes`] of the schema at a source node, if indexed.
    pub fn position(&self, source: usize) -> Option<usize> {
        if !self.located {
            return None;
        }
        let source = u32::try_from(source).ok()?;
        let positions = self.positions.get_or_init(|| {
            let mut positions: Vec<u32> = (0..self.nodes.len() as u32).collect();
            positions.sort_unstable_by_key(|&index| self.nodes[index as usize].id);
            positions
        });
        positions
            .binary_search_by_key(&source, |&index| self.nodes[index as usize].id)
            .ok()
            .map(|at| positions[at] as usize)
    }
    /// A plain name's text.
    pub fn anchor_name(&self, anchor: &Anchor) -> &str {
        self.anchor_token(anchor).as_str().expect("an indexed name")
    }
    /// The declaration token of a plain name.
    pub fn anchor_token(&self, anchor: &Anchor) -> JsonRef<'_> {
        node_of(self.source.as_ref(), anchor.name)
    }
    /// The declarations of one name, by resource, then name.
    pub fn anchor_declarations(&self) -> impl Iterator<Item = &[Anchor]> {
        self.anchors
            .chunk_by(|a, b| a.resource == b.resource && self.anchor_name(a) == self.anchor_name(b))
    }
    /// The declaring schemas of one compared identifier declared more than
    /// once, as indices into [`Self::nodes`], by identifier.
    pub fn repeated_identifiers(&self) -> impl Iterator<Item = &[(u32, u32)]> {
        self.identifiers.chunk_by(|a, b| a.0 == b.0)
    }
    /// A supplied resource's `$id` declarations that have a base, with it.
    pub fn declared_bases(&self) -> &[(u32, Arc<str>)] {
        &self.bases
    }
    fn named(&self, resource: u32, name: &str) -> &[Anchor] {
        let key = |a: &Anchor| (a.resource, self.anchor_name(a));
        let start = self.anchors.partition_point(|a| key(a) < (resource, name));
        let end = start + self.anchors[start..].partition_point(|a| key(a) <= (resource, name));
        &self.anchors[start..end]
    }
    pub fn same_document(&self, document: &JsonValue, reference: &str) -> SameDocument {
        let Some(fragment) =
            crate::uri::decode_fragment(reference.strip_prefix('#').unwrap_or(reference))
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
            return match self.position(ref_id(value)) {
                Some(index) if self.obi_position(index) => {
                    SameDocument::Found(self.owned(&self.nodes[index]))
                }
                _ => SameDocument::Missing,
            };
        }
        match self.named(0, &fragment) {
            [] => SameDocument::Missing,
            [anchor] => SameDocument::Found(self.owned(&self.nodes[anchor.node as usize])),
            _ => SameDocument::Ambiguous,
        }
    }
}
fn node_of(source: Option<&JsonValue>, id: u32) -> JsonRef<'_> {
    node(source.expect("an index of a source"), id as usize).expect("a node of the source")
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
    use openbindings_internal_json::backend::node_id;
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
                    index.value(node).get("$ref").unwrap().as_str(),
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
            for (index, (a, b)) in full.nodes.iter().zip(&nodes.nodes).enumerate() {
                assert_eq!(a.id, b.id);
                assert_eq!(
                    (
                        a.depth,
                        a.resource,
                        full.parent_resource(index),
                        full.obi_position(index)
                    ),
                    (
                        b.depth,
                        b.resource,
                        nodes.parent_resource(index),
                        nodes.obi_position(index)
                    )
                );
            }
            assert!(
                nodes.position(nodes.nodes[0].id as usize).is_none()
                    && nodes.anchors.is_empty()
                    && nodes.identifiers.is_empty()
                    && nodes.bases.is_empty()
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

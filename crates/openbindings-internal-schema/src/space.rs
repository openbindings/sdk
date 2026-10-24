//! Original-context resource graph. No URI here acquires bytes.
mod library;
use crate::{
    schema_index::{DIALECT, SchemaIndex},
    uri, *,
};
use openbindings_internal_json::backend::{
    has_unpaired, location_pointer_size, node_id, pointer_into, push_pointer, ref_id,
    relative_pointer,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub struct SpaceNode {
    pub value: JsonValue,
    pub resource: usize,
    pub source: usize,
    pub depth: usize,
}
impl std::fmt::Debug for SpaceNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpaceNode").finish_non_exhaustive()
    }
}

pub struct Resource {
    pub root: Option<usize>,
    pub base: Option<String>,
    pub anonymous: bool,
    pub dialect: Option<String>,
    pub anchors: BTreeMap<String, Vec<(usize, bool)>>,
    pub origin: ResourceOrigin,
    // Read through `SchemaSpace::eligible`, which accounts for local claims.
    eligible: bool,
}
impl std::fmt::Debug for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resource").finish_non_exhaustive()
    }
}

// Priority applies to name associations, not bodies or source provenance. A
// supplied retrieval alias competes equally with every supplied declared ID.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ResourceOrigin {
    Contained,
    Supplied,
    Standard,
}
#[derive(Clone, Copy)]
struct NameSelection {
    origin: ResourceOrigin,
    // None denotes multiple carriers at the winning priority, never absence.
    carrier: Option<usize>,
}
pub struct Source {
    pub uri: Option<String>,
    pub value: JsonValue,
    pub positions: HashMap<usize, usize>,
}
impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source").finish_non_exhaustive()
    }
}

const PROGRAM_NAMESPACE: &str = "https://sdk-program.openbindings.invalid/";
// A projection is not a document: it holds the reached schemas of the document,
// its supplied resources and the packaged meta-schemas, with its own references
// and wrappers. It keeps fixed bounds, independent of any admission limits: its
// text stops at 64 MiB, and each resource it writes is parsed within 64 MiB,
// 1,000,000 nodes and 10,000 levels.
const PROJECTION_LIMITS: openbindings_internal_json::JsonLimits =
    openbindings_internal_json::JsonLimits {
        max_bytes: 64 * 1024 * 1024,
        max_depth: 10_000,
        max_nodes: 1_000_000,
    };
/// A projection without its location map, with the node each definition of
/// each projected resource came from, in `$defs` order.
pub struct UnmappedProgram {
    pub program: Program,
    pub definitions: Vec<Vec<usize>>,
    /// Each `const` and `enum` value, by the index the program writes in its
    /// place, all in one arena holding only them ([`SchemaSpace::program_unmapped`]).
    pub literals: Vec<JsonValue>,
}
impl std::fmt::Debug for UnmappedProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnmappedProgram").finish_non_exhaustive()
    }
}
// One empty map shared by every unmapped program, so none allocates one.
fn unmapped() -> std::sync::Arc<BTreeMap<String, crate::ProgramOrigin>> {
    static EMPTY: std::sync::OnceLock<std::sync::Arc<BTreeMap<String, crate::ProgramOrigin>>> =
        std::sync::OnceLock::new();
    EMPTY.get_or_init(Default::default).clone()
}
/// One keyword of a projected schema object, as its program writes it.
pub enum Field<'s> {
    /// `$ref` or `$dynamicRef`: the generated URI it names.
    Reference(String),
    /// One subschema: its definition's address, which the program writes as
    /// `{"$ref": <address>}`.
    Single(String),
    /// Subschemas by name, in member order; the program's text holds them
    /// in name order, each name once, the last of a repeated one.
    Map(Vec<(&'s str, String)>),
    /// Subschemas in order.
    List(Vec<String>),
    /// A `const` or `enum` value: its index in the literal arena.
    Literal(usize),
    /// Any other keyword's value, unchanged.
    Copied(JsonRef<'s>),
}
/// A projected definition's schema.
pub enum Schema<'s> {
    Boolean(bool),
    /// Its keywords in member order: the program's text holds each keyword
    /// once, in name order, the last of a repeated one.
    Object(Vec<(&'s str, Field<'s>)>),
}
/// One projected definition: its name in `$defs`, its node and its schema.
pub struct Definition<'s> {
    pub name: String,
    pub node: usize,
    pub schema: Schema<'s>,
}
/// One projected resource, as [`Program::resources`] holds it.
pub struct DefinedResource<'s> {
    pub uri: String,
    /// In the entry's resource, the entry's address, which the resource's
    /// root references.
    pub entry: Option<String>,
    /// In `$defs` order.
    pub definitions: Vec<Definition<'s>>,
}
/// A projection as definitions, with its separated literals.
pub struct Definitions<'s> {
    pub entry_uri: String,
    pub resources: Vec<DefinedResource<'s>>,
    pub literals: Vec<JsonValue>,
}
// Debug is structural: no projected name, address or value is written.
macro_rules! structural_debug {
    ($($name:ident),*) => {$(
        impl std::fmt::Debug for $name<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    )*};
}
structural_debug!(Field, Schema, Definition, DefinedResource, Definitions);
/// How a projected schema keyword holds its subschemas. A projection writes each
/// child position as `{"$ref": <generated child address>}`; every other keyword
/// value is copied from the original source unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildShape {
    /// An object whose member values are subschemas.
    Map,
    /// An array of subschemas.
    List,
    /// One subschema.
    Single,
}
/// The child shape of a 2020-12 applicator keyword, or `None` for keywords whose
/// values a projection copies unchanged. `then` and `else` are projected only
/// beside `if`.
pub fn child_shape(keyword: &str) -> Option<ChildShape> {
    Some(match keyword {
        "properties" | "patternProperties" | "dependentSchemas" => ChildShape::Map,
        "allOf" | "anyOf" | "oneOf" | "prefixItems" => ChildShape::List,
        "additionalProperties"
        | "propertyNames"
        | "unevaluatedProperties"
        | "items"
        | "contains"
        | "unevaluatedItems"
        | "not"
        | "if"
        | "then"
        | "else" => ChildShape::Single,
        _ => return None,
    })
}

pub struct SchemaSpace {
    pub document: RawDocument,
    pub supplied: Supplied,
    // The document and the supplied resources, in that order. The packaged
    // meta-schemas follow them as one immutable layer that every space in the
    // process shares.
    local: Layer,
    packaged: &'static Layer,
    inspection: std::sync::OnceLock<library::Inspection>,
}
// The packaged layer numbers its nodes, resources and sources from here, so
// its entries hold their final identifiers in every space. Local identifiers
// stay below it; [`SchemaSpace::node_number`] gives the dense numbering.
const PACKAGED: usize = 1 << (usize::BITS - 1);
// The nodes, resources and sources of one layer, with the names they declare.
#[derive(Default)]
struct Layer {
    base: usize,
    nodes: Vec<SpaceNode>,
    resources: Vec<Resource>,
    sources: Vec<Source>,
    names: BTreeMap<String, NameSelection>,
    // How inspection reaches each reference's carrier, for the packaged layer.
    lookups: std::sync::OnceLock<Vec<library::PackagedLookup>>,
    // For the packaged layer: every name its references look up and every
    // base of its resources, sorted.
    claimable: std::sync::OnceLock<Vec<String>>,
}
impl std::fmt::Debug for SchemaSpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaSpace").finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct Target {
    pub node: usize,
    pub dynamic_name: Option<String>,
}
impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
/// Static reference inspection outcome. Located dynamic references still require evaluation-time scope resolution.
pub enum ReferenceResolution {
    /// A static target was located in the supplied original context.
    Located {
        /// Original target resource and pointer, not a fetched or generated address.
        target: Location,
        /// True when evaluation must additionally apply dynamic-anchor scope; the static target alone is not the final runtime target.
        dynamic_lookup: bool,
    },
    /// No sound target could be established within supported capabilities and supplied resources.
    Unresolved {
        /// Structured unresolved-reference cause at the original reference location when available.
        detail: Failure,
    },
}

#[derive(Clone, Serialize)]
/// One `$ref` or `$dynamicRef` encountered in supported schema positions; opaque JSON content is not traversed as schema.
pub struct Reference {
    /// Original location of the reference keyword.
    pub location: Location,
    /// Reference keyword spelling, such as `$ref` or `$dynamicRef`.
    pub keyword: String,
    /// Original scalar reference string, or `None` when its value is not representable as one.
    pub spelling: Option<String>,
    /// Static location result or a structured refusal; never triggers I/O.
    pub resolution: ReferenceResolution,
}
impl std::fmt::Debug for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reference")
            .field("location", &self.location)
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Serialize)]
/// Bounded original-context reference inspection, not conformance proof or successful schema preparation.
pub struct ReferenceReport {
    /// Retained reference observations in traversal order.
    pub references: Vec<Reference>,
    /// Whether traversal completed; individual unresolved references may still occur in a complete report.
    pub complete: bool,
    /// Traversal-level cause when inspection is incomplete, distinct from per-reference refusals.
    pub limitation: Option<Failure>,
}
use serde::Serialize;

impl Layer {
    fn resource(&self, id: usize) -> &Resource {
        &self.resources[id - self.base]
    }
    fn resource_mut(&mut self, id: usize) -> &mut Resource {
        &mut self.resources[id - self.base]
    }
    fn nodes(&self) -> impl Iterator<Item = (usize, &SpaceNode)> {
        (self.base..).zip(&self.nodes)
    }
    fn resources(&self) -> impl Iterator<Item = (usize, &Resource)> {
        (self.base..).zip(&self.resources)
    }
    fn add_source(
        &mut self,
        retrieval: Option<String>,
        value: JsonValue,
        index: &SchemaIndex,
        origin: ResourceOrigin,
        protected: &HashSet<String>,
    ) {
        let source = self.base + self.sources.len();
        let start = self.nodes.len();
        let mut resources = HashMap::new();
        let initial = self.base + self.resources.len();
        resources.insert(0, initial);
        self.resources.push(Resource {
            root: None,
            base: retrieval
                .as_deref()
                .and_then(crate::input_policy::canonical_uri),
            anonymous: retrieval.is_none(),
            dialect: Some(DIALECT.into()),
            anchors: BTreeMap::new(),
            origin,
            eligible: true,
        });
        if retrieval.is_some() {
            self.resource_mut(initial).dialect = declared_dialect(value.view(), Some(DIALECT));
        }
        // Every indexed node becomes one space node with one position.
        self.nodes.reserve(index.nodes.len());
        let mut positions = HashMap::with_capacity(index.nodes.len());
        for (at, node) in index.nodes.iter().enumerate() {
            let value = index.value(node);
            let resource = if let Some(&id) = resources.get(&(node.resource as usize)) {
                id
            } else {
                let parent = *resources
                    .get(&(index.parent_resource(at) as usize))
                    .unwrap_or(&initial);
                let id = self.base + self.resources.len();
                resources.insert(node.resource as usize, id);
                let dialect = declared_dialect(value, self.resource(parent).dialect.as_deref());
                let base = value.get("$id").and_then(|v| v.as_str()).and_then(|id| {
                    crate::input_policy::resolve_identifier(
                        self.resource(parent).base.as_deref(),
                        id,
                    )
                });
                self.resources.push(Resource {
                    root: Some(self.base + self.nodes.len()),
                    base,
                    anonymous: false,
                    dialect,
                    anchors: BTreeMap::new(),
                    origin,
                    eligible: true,
                });
                id
            };
            let id = self.base + self.nodes.len();
            positions.insert(node.id as usize, id);
            self.nodes.push(SpaceNode {
                value: value.to_owned(),
                resource,
                source,
                depth: node.depth as usize,
            });
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if let Some(name) = value
                    .get(keyword)
                    .and_then(|v| v.as_str())
                    .filter(|s| crate::schema_index::plain_name(s))
                {
                    self.resource_mut(resource)
                        .anchors
                        .entry(name.into())
                        .or_default()
                        .push((id, keyword == "$dynamicAnchor"));
                }
            }
        }
        if retrieval.is_some() && start < self.nodes.len() {
            let root_resource = self.nodes[start].resource;
            self.resource_mut(root_resource).root = Some(self.base + start);
            if origin != ResourceOrigin::Supplied
                && let Some(retrieval) = &retrieval
            {
                self.add_name(retrieval.clone(), root_resource, protected);
            }
        }
        for &id in resources.values() {
            if origin != ResourceOrigin::Supplied
                && let Some(base) = self.resource(id).base.clone()
                && self.resource(id).root.is_some()
            {
                self.add_name(base, id, protected);
            }
        }
        self.sources.push(Source {
            uri: retrieval,
            value,
            positions,
        });
    }
    fn add_name(&mut self, name: String, resource: usize, protected: &HashSet<String>) {
        let Some(name) = crate::input_policy::canonical_uri(&name) else {
            return;
        };
        let origin = self.resource(resource).origin;
        if origin == ResourceOrigin::Supplied
            && protected.contains(&name)
            && self
                .names
                .get(&name)
                .is_none_or(|selected| selected.origin != ResourceOrigin::Contained)
        {
            self.names.insert(
                name,
                NameSelection {
                    origin,
                    carrier: None,
                },
            );
            return;
        }

        self.names
            .entry(name)
            .and_modify(|selected| {
                // Supplied resources (including nested IDs and retrieval aliases)
                // cannot replace packaged standards. Contained IDs retain priority.
                if matches!(
                    (origin, selected.origin),
                    (ResourceOrigin::Standard, ResourceOrigin::Supplied)
                        | (ResourceOrigin::Supplied, ResourceOrigin::Standard)
                ) {
                    selected.origin = ResourceOrigin::Supplied;
                    selected.carrier = None;
                } else if origin < selected.origin {
                    *selected = NameSelection {
                        origin,
                        carrier: Some(resource),
                    };
                } else if origin == selected.origin && selected.carrier != Some(resource) {
                    selected.carrier = None;
                }
            })
            .or_insert(NameSelection {
                origin,
                carrier: Some(resource),
            });
    }
    // Finalize once, after every alias and canonical association is known.
    // An independently named child does not inherit its parent's eligibility.
    fn finalize(
        &mut self,
        select: impl Fn(&BTreeMap<String, NameSelection>, &str) -> Option<NameSelection>,
    ) {
        let Self {
            base,
            resources,
            names,
            ..
        } = self;
        for (id, resource) in (*base..).zip(resources.iter_mut()) {
            if !resource.anonymous
                && let Some(name) = &resource.base
            {
                resource.eligible = select(names, name).and_then(|name| name.carrier) == Some(id);
            }
        }
    }
}
// The packaged meta-schemas as one layer, built once per process from their
// pinned, immutable documents and URIs. It holds no caller URI or result.
// Resource eligibility here is among the packaged resources alone; a space
// withdraws it from a packaged resource whose identifier a local one claims.
fn packaged() -> &'static Layer {
    static LAYER: std::sync::OnceLock<Layer> = std::sync::OnceLock::new();
    LAYER.get_or_init(|| {
        let mut layer = Layer {
            base: PACKAGED,
            ..Layer::default()
        };
        let unprotected = HashSet::new();
        for resource in embedded_resources() {
            let index = SchemaIndex::supplied_nodes(&resource.document);
            layer.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &index,
                ResourceOrigin::Standard,
                &unprotected,
            );
        }
        layer.finalize(|names, name| names.get(name).copied());
        layer
    })
}
// A name's selection in a space. The packaged names count as added after
// every local name, under `Layer::add_name`'s rules: a local contained
// carrier keeps the name, and a supplied claim to a packaged name leaves it
// without a single carrier.
fn select(
    local: &BTreeMap<String, NameSelection>,
    packaged: &Layer,
    name: &str,
) -> Option<NameSelection> {
    match (local.get(name), packaged.names.get(name)) {
        (Some(selected), Some(_)) if selected.origin == ResourceOrigin::Supplied => {
            Some(NameSelection {
                origin: ResourceOrigin::Supplied,
                carrier: None,
            })
        }
        (Some(selected), _) => Some(*selected),
        (None, packaged) => packaged.copied(),
    }
}

impl SchemaSpace {
    pub fn new(document: RawDocument, supplied: Supplied) -> Self {
        let packaged = packaged();
        let mut local = Layer::default();
        let protected = &supplied.policy.protected;
        local.add_source(
            None,
            document.value().clone(),
            document.schema_index(),
            ResourceOrigin::Contained,
            protected,
        );
        for (resource, index) in supplied.iter().zip(supplied.nodes()) {
            local.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                index,
                ResourceOrigin::Supplied,
                protected,
            );
        }
        // Canonical/retrieval associations and protected standard identities
        // originate in the immutable supplied-resource input policy.
        for (name, holders) in &supplied.policy.names {
            for &(source, node) in holders {
                if let Some(&id) = local.sources[source + 1].positions.get(&node) {
                    let resource = local.nodes[id].resource;
                    local.add_name(name.clone(), resource, protected);
                }
            }
        }
        local.finalize(|names, name| select(names, packaged, name));
        Self {
            document,
            supplied,
            local,
            packaged,
            inspection: std::sync::OnceLock::new(),
        }
    }
    // Local identifiers index the local layer directly; any other identifier
    // is the packaged layer's.
    /// The schema node with this identifier.
    #[inline]
    pub fn node(&self, id: usize) -> &SpaceNode {
        match self.local.nodes.get(id) {
            Some(node) => node,
            None => &self.packaged.nodes[id - PACKAGED],
        }
    }
    /// The resource with this identifier.
    #[inline]
    pub fn resource(&self, id: usize) -> &Resource {
        match self.local.resources.get(id) {
            Some(resource) => resource,
            None => &self.packaged.resources[id - PACKAGED],
        }
    }
    /// The source with this identifier.
    #[inline]
    pub fn source(&self, id: usize) -> &Source {
        match self.local.sources.get(id) {
            Some(source) => source,
            None => &self.packaged.sources[id - PACKAGED],
        }
    }
    /// Every schema node with its identifier: the document's, each supplied
    /// resource's, then the packaged meta-schemas', in identifier order.
    pub fn nodes(&self) -> impl Iterator<Item = (usize, &SpaceNode)> {
        self.local.nodes().chain(self.packaged.nodes())
    }
    /// The document's and each supplied resource's schema nodes, as
    /// [`Self::nodes`] begins. A packaged meta-schema's reference always names
    /// a resource the packaged layer holds, so only these can name a resource
    /// that no resource holds.
    pub fn local_nodes(&self) -> impl Iterator<Item = (usize, &SpaceNode)> {
        self.local.nodes()
    }
    /// Every resource with its identifier, in identifier order.
    pub fn resources(&self) -> impl Iterator<Item = (usize, &Resource)> {
        self.local.resources().chain(self.packaged.resources())
    }
    /// A node's position in [`Self::nodes`], as generated names number it.
    pub fn node_number(&self, id: usize) -> usize {
        if id < PACKAGED {
            id
        } else {
            self.local.nodes.len() + (id - PACKAGED)
        }
    }
    /// A resource's position in [`Self::resources`], as generated names number it.
    pub fn resource_number(&self, id: usize) -> usize {
        if id < PACKAGED {
            id
        } else {
            self.local.resources.len() + (id - PACKAGED)
        }
    }
    /// Whether a resource is the selected carrier of its canonical identifier.
    pub fn eligible(&self, id: usize) -> bool {
        let resource = self.resource(id);
        if id < PACKAGED || !resource.eligible || resource.anonymous {
            return resource.eligible;
        }
        resource
            .base
            .as_ref()
            .is_none_or(|name| !self.local.names.contains_key(name))
    }
    fn name(&self, name: &str) -> Option<NameSelection> {
        select(&self.local.names, self.packaged, name)
    }
    fn has_name(&self, name: &str) -> bool {
        self.local.names.contains_key(name) || self.packaged.names.contains_key(name)
    }
    pub fn check_resource(&self, resource: usize, at: usize) -> Result<(), Failure> {
        if self.eligible(resource) {
            return Ok(());
        }
        let mut detail = Failure::new(
            FailureReason::ConservativePreparation,
            "resource-identity-conflict",
            "the reached resource is not the selected carrier of its canonical identifier",
        );
        // Refusal is independent of optional diagnostic text. Measure original
        // pointers and retrieval provenance before any location allocation.
        const LOCATION_BYTES: usize = 64 * 1024;
        let node = self.node(at);
        let source = self.source(node.source);
        let uri_bytes = source.uri.as_ref().map_or(0, String::len);
        if let Some(remaining) = LOCATION_BYTES.checked_sub(uri_bytes)
            && location_pointer_size(node.value.view(), remaining).is_some()
        {
            detail.location = Some(self.location(at));
        }
        Err(detail)
    }
    pub fn document_node(&self, value: &JsonValue) -> Option<usize> {
        self.local.sources[0]
            .positions
            .get(&node_id(value))
            .copied()
    }
    /// Append a node's original pointer, exactly as [`Self::location`] gives it,
    /// to `out`, using `scratch` for its source's own pointer. `false`,
    /// appending nothing, when the pointer passes through a name holding
    /// unpaired units, which only [`Self::location`] can hold.
    pub fn location_into(&self, node: usize, out: &mut Vec<u8>, scratch: &mut Vec<u8>) -> bool {
        let node = self.node(node);
        let source = self.source(node.source);
        scratch.clear();
        if !pointer_into(&source.value, scratch) {
            scratch.clear();
        }
        let start = out.len();
        if !pointer_into(&node.value, out) {
            return false;
        }
        if !scratch.is_empty() && out[start..].starts_with(scratch) {
            out.drain(start..start + scratch.len());
        }
        true
    }
    /// The URI of a node's source, as [`Self::location`] gives it.
    pub fn source_uri(&self, node: usize) -> Option<&str> {
        self.source(self.node(node).source).uri.as_deref()
    }
    pub fn location(&self, node: usize) -> Location {
        let node = self.node(node);
        let source = self.source(node.source);
        // Exact from the source's own root, including names with unpaired units.
        Location {
            resource: source.uri.clone(),
            pointer: relative_pointer(node.value.view(), source.value.view()),
        }
    }
    pub fn failure(
        &self,
        node: usize,
        reason: FailureReason,
        code: &str,
        message: impl Into<String>,
    ) -> Failure {
        Failure::new(reason, code, message).located(self.location(node))
    }
    pub fn resolve(&self, holder: usize, reference: &str) -> Result<Target, Failure> {
        self.resolve_with_missing(holder, reference, &mut |_| {})
    }
    fn malformed_reference_scope(&self, holder: usize) -> bool {
        let resource = self.resource(self.node(holder).resource);
        resource.base.is_some()
            && resource
                .root
                .is_some_and(|root| self.node(root).value.get("$id").is_some())
    }
    pub fn resolve_with_missing(
        &self,
        holder: usize,
        reference: &str,
        missing: &mut impl FnMut(&str),
    ) -> Result<Target, Failure> {
        let at = self.node(holder);
        let resource = self.resource(at.resource);
        let fail = |code: &str, message: String| {
            self.failure(
                holder,
                FailureReason::ConservativePreparation,
                code,
                message,
            )
        };
        if !uri::valid(reference) {
            if self.malformed_reference_scope(holder) {
                // The repaired inspection carrier must still prepare: an
                // unsupported sibling identifier cannot become a semantic hole.
                return self.library_target(holder, reference, missing);
            }
            return Err(fail(
                "invalid-reference",
                "reference is not a well-formed URI reference".into(),
            ));
        }
        if resource.anonymous && !uri::absolute(reference) {
            if !reference.is_empty() && !reference.starts_with('#') {
                return Err(fail(
                    "anonymous-relative-reference",
                    "the anonymous document resource has no named base".into(),
                ));
            }
            return match self
                .document
                .schema_index()
                .same_document(self.document.value(), reference)
            {
                crate::schema_index::SameDocument::Found(value) => {
                    let node = self.document_node(&value).ok_or_else(|| {
                        fail(
                            "unindexed-target",
                            "target is outside the schema index".into(),
                        )
                    })?;
                    self.check_resource(self.node(node).resource, holder)?;
                    let name =
                        uri::decode_fragment(reference.strip_prefix('#').unwrap_or(reference))
                            .filter(|s| !s.is_empty() && !s.starts_with('/'));
                    let dynamic_name = name.filter(|s| {
                        value.get("$dynamicAnchor").and_then(|v| v.as_str()) == Some(s.as_str())
                    });
                    Ok(Target { node, dynamic_name })
                }
                crate::schema_index::SameDocument::Ambiguous => Err(fail(
                    "ambiguous-anchor",
                    "the document resource declares this name more than once".into(),
                )),
                crate::schema_index::SameDocument::Missing => Err(fail(
                    "non-schema-target",
                    "the same-document reference identifies no schema at an OBI position".into(),
                )),
            };
        }
        self.library_target(holder, reference, missing)
    }

    pub fn references(&self, control: &dyn Control) -> ReferenceReport {
        let mut report = ReferenceReport {
            references: Vec::new(),
            complete: true,
            limitation: None,
        };
        for (id, node) in self.local.nodes().filter(|(_, n)| n.source == 0) {
            if let Err(reason) = control.check() {
                report.complete = false;
                report.limitation = Some(reason);
                break;
            }
            if node.depth > 256 {
                report.complete = false;
                report.limitation.get_or_insert_with(|| {
                    self.failure(
                        id,
                        FailureReason::LimitExceeded,
                        "schema-depth",
                        "reference inspection stops below 256 schema levels",
                    )
                });
                continue;
            }
            for keyword in ["$ref", "$dynamicRef"] {
                if let Some(value) = node.value.get(keyword) {
                    let spelling = value.as_str().map(str::to_owned);
                    let target = spelling
                        .as_deref()
                        .ok_or_else(|| {
                            self.failure(
                                id,
                                FailureReason::ConservativePreparation,
                                "reference-type",
                                "reference is not a string",
                            )
                        })
                        .and_then(|s| self.resolve(id, s));
                    let resolution = match target {
                        Ok(target) => ReferenceResolution::Located {
                            target: self.location(target.node),
                            dynamic_lookup: keyword == "$dynamicRef"
                                && target.dynamic_name.is_some(),
                        },
                        Err(detail) => ReferenceResolution::Unresolved { detail },
                    };
                    let mut location = self.location(id);
                    // Reference keywords need no escape.
                    push_pointer(&mut location.pointer, &format!("/{keyword}"));
                    report.references.push(Reference {
                        location,
                        keyword: keyword.into(),
                        spelling,
                        resolution,
                    });
                }
            }
        }
        report
    }
    pub fn program(&self, entry: usize, control: &dyn Control) -> Result<Program, Failure> {
        control.check()?;
        let reach = self.reach(entry, control)?;
        self.project(entry, &reach, control)
    }
    /// The same projection as [`Self::program`] without its location map, for a
    /// caller that writes origins itself: each definition's node, per projected
    /// resource in `$defs` order, whose origin [`Self::location_into`] gives.
    ///
    /// Each `const` and `enum` value is copied into one arena that holds only
    /// those values, and the program holds its index in the value's place. The
    /// copy is parsed instead of the text the program would hold, so a caller
    /// can keep literals without keeping the program.
    pub fn program_unmapped(
        &self,
        entry: usize,
        control: &dyn Control,
    ) -> Result<UnmappedProgram, Failure> {
        control.check()?;
        let reach = self.reach(entry, control)?;
        let mut literals = Vec::new();
        let (program, definitions) = self.projection(
            entry,
            &reach,
            control,
            PROGRAM_NAMESPACE,
            false,
            Some(&mut literals),
        )?;
        Ok(UnmappedProgram {
            program,
            definitions,
            literals,
        })
    }
    pub fn project(
        &self,
        entry: usize,
        reach: &Reach,
        control: &dyn Control,
    ) -> Result<Program, Failure> {
        self.project_with_namespace(entry, reach, control, PROGRAM_NAMESPACE)
    }
    pub fn project_with_namespace(
        &self,
        entry: usize,
        reach: &Reach,
        control: &dyn Control,
        namespace: &str,
    ) -> Result<Program, Failure> {
        self.projection(entry, reach, control, namespace, true, None)
            .map(|(program, _)| program)
    }
    // With `mapped`, the program maps every definition to its origin; without,
    // it maps nothing and the nodes of each resource's definitions are returned.
    // With `literals`, each `const` and `enum` value is copied there instead,
    // into one arena holding only those values, and the program holds its
    // index; the copy counts toward the byte limit as the value's text would.
    fn projection(
        &self,
        entry: usize,
        reach: &Reach,
        control: &dyn Control,
        namespace: &str,
        mapped: bool,
        literals: Option<&mut Vec<JsonValue>>,
    ) -> Result<(Program, Vec<Vec<usize>>), Failure> {
        // Generated names number nodes and resources densely (`node_number`).
        let resource_uri =
            |resource: usize| format!("{namespace}r{}", self.resource_number(resource));
        let address = |node: usize| {
            format!(
                "{}#/$defs/n{}",
                resource_uri(self.node(node).resource),
                self.node_number(node)
            )
        };
        let reference = |address: &str| format!("{{\"$ref\":{}}}", quoted(address));
        // Reach admits every node through check_known_node before projection.
        // Its meta-schema and scalar-name checks establish the shapes below.
        let mut definitions: BTreeMap<usize, BTreeMap<String, (String, usize)>> = BTreeMap::new();
        let mut locations = BTreeMap::new();
        let mut byte_count = 0usize;
        // Separated literals, copied together once every node is admitted, and
        // the literal text the current node separates, counted with its own.
        let separating = literals.is_some();
        let mut values = Vec::new();
        let mut separated = 0usize;
        let mut nodes: Vec<_> = reach.nodes.iter().copied().collect();
        nodes.sort_unstable();
        for id in nodes {
            control.check()?;
            let node = self.node(id);
            let transformed = if node.value.kind() == JsonKind::Boolean {
                node.value.text().to_owned()
            } else {
                let fields = self.fields(
                    id,
                    reach,
                    namespace,
                    separating,
                    &mut values,
                    &mut separated,
                )?;
                let mut text = BTreeMap::new();
                for (keyword, field) in fields {
                    let value = match field {
                        Field::Reference(uri) => quoted(&uri),
                        Field::Single(address) => reference(&address),
                        Field::Map(children) => object(
                            children
                                .into_iter()
                                .map(|(name, address)| (name.to_owned(), reference(&address)))
                                .collect::<BTreeMap<_, _>>(),
                        ),
                        Field::List(children) => format!(
                            "[{}]",
                            children
                                .iter()
                                .map(|address| reference(address))
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                        Field::Literal(index) => index.to_string(),
                        Field::Copied(value) => value.text().to_owned(),
                    };
                    text.insert(keyword.to_owned(), value);
                }
                object(text)
            };
            byte_count = byte_count
                .saturating_add(transformed.len())
                .saturating_add(std::mem::take(&mut separated));
            if byte_count > PROJECTION_LIMITS.max_bytes {
                return Err(self.failure(
                    id,
                    FailureReason::LimitExceeded,
                    "program-byte-limit",
                    "schema projection exceeds 64 MiB",
                ));
            }
            if mapped {
                locations.insert(address(id), crate::ProgramOrigin::Prefix(self.location(id)));
            }
            definitions
                .entry(node.resource)
                .or_default()
                .insert(format!("n{}", self.node_number(id)), (transformed, id));
        }
        if let Some(literals) = literals {
            *literals = openbindings_internal_json::backend::standalone_list(&values);
        }
        let entry_resource = self.node(entry).resource;
        let entry_uri = resource_uri(entry_resource);
        let mut nodes = Vec::new();
        if !mapped {
            nodes.reserve_exact(definitions.len());
        }
        let resources = definitions
            .into_iter()
            .map(|(id, defs)| {
                let uri = resource_uri(id);
                if !mapped {
                    nodes.push(defs.values().map(|&(_, node)| node).collect());
                }
                let defs = defs.into_iter().map(|(name, (text, _))| (name, text));
                let mut fields = BTreeMap::from([
                    ("$id".into(), quoted(&uri)),
                    ("$schema".into(), quoted(DIALECT)),
                    ("$defs".into(), object(defs)),
                ]);
                // Only the entry resource's root applies the entry. Other roots are
                // containers; every original reference names its exact projected node.
                if id == entry_resource {
                    fields.insert("$ref".into(), quoted(&address(entry)));
                }
                JsonValue::parse_with_limits(object(fields), PROJECTION_LIMITS)
                    .map(|document| ProjectedResource { uri, document })
                    .map_err(|e| {
                        // A fixed bound: no admission setting raises it.
                        let what =
                            openbindings_internal_json::backend::input_error_description(e.code)
                                .unwrap_or(e.code);
                        Failure::new(
                            FailureReason::LimitExceeded,
                            "program-json-limit",
                            format!("{what} at byte {}", e.byte_offset),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let locations = if mapped {
            std::sync::Arc::new(locations)
        } else {
            unmapped()
        };
        let program = Program {
            entry_uri,
            resources,
            locations,
        };
        Ok((program, nodes))
    }
    // The generated URI of a projected resource.
    fn resource_uri(&self, namespace: &str, resource: usize) -> String {
        format!("{namespace}r{}", self.resource_number(resource))
    }
    // The generated address of a node's definition.
    fn address(&self, namespace: &str, node: usize) -> String {
        format!(
            "{namespace}r{}#/$defs/n{}",
            self.resource_number(self.node(node).resource),
            self.node_number(node)
        )
    }
    // The keywords a reached schema object's definition holds, in member
    // order, as a projection writes them. With `separating`, each `const` and
    // `enum` value is pushed to `values` and its text counted in `separated`.
    fn fields<'s>(
        &'s self,
        id: usize,
        reach: &Reach,
        namespace: &str,
        separating: bool,
        values: &mut Vec<JsonRef<'s>>,
        separated: &mut usize,
    ) -> Result<Vec<(&'s str, Field<'s>)>, Failure> {
        let node = self.node(id);
        let members = node.value.view().members().expect("checked schema object");
        let mut fields = Vec::with_capacity(members.len());
        for member in members {
            let keyword = member.name.as_str().expect("checked schema member");
            if matches!(
                keyword,
                "$id"
                    | "$schema"
                    | "$defs"
                    | "definitions"
                    | "dependencies"
                    | "contentSchema"
                    | "$anchor"
            ) {
                continue;
            }
            if keyword == "$dynamicAnchor"
                && !member
                    .value
                    .as_str()
                    .is_some_and(|name| reach.dynamic.contains_key(name))
            {
                continue;
            }
            if matches!(keyword, "then" | "else") && node.value.get("if").is_none() {
                continue;
            }
            if matches!(keyword, "$ref" | "$dynamicRef") {
                let target = &reach.targets[&(id, keyword == "$dynamicRef")];
                let uri = if keyword == "$dynamicRef" {
                    if let Some(name) = &target.dynamic_name {
                        format!(
                            "{}#{name}",
                            self.resource_uri(namespace, self.node(target.node).resource)
                        )
                    } else {
                        self.address(namespace, target.node)
                    }
                } else {
                    self.address(namespace, target.node)
                };
                fields.push((keyword, Field::Reference(uri)));
                continue;
            }
            let child = |value: JsonRef<'_>| -> Result<String, Failure> {
                let target = self
                    .source(node.source)
                    .positions
                    .get(&ref_id(value))
                    .copied()
                    .filter(|n| reach.nodes.contains(n))
                    .ok_or_else(|| {
                        self.failure(
                            id,
                            FailureReason::ConservativePreparation,
                            "projection-target",
                            "an evaluated child is not in the prepared closure",
                        )
                    })?;
                Ok(self.address(namespace, target))
            };
            let field = match child_shape(keyword) {
                Some(ChildShape::Map) => {
                    let entries = member.value.members().expect("checked schema map");
                    let mut children = Vec::with_capacity(entries.len());
                    for m in entries {
                        children.push((m.name.as_str().unwrap(), child(m.value)?));
                    }
                    Field::Map(children)
                }
                Some(ChildShape::List) => Field::List(
                    member
                        .value
                        .elements()
                        .expect("checked schema list")
                        .map(child)
                        .collect::<Result<_, _>>()?,
                ),
                Some(ChildShape::Single) => Field::Single(child(member.value)?),
                None if separating && matches!(keyword, "const" | "enum") => {
                    *separated = separated.saturating_add(member.value.text().len());
                    values.push(member.value);
                    Field::Literal(values.len() - 1)
                }
                None => Field::Copied(member.value),
            };
            fields.push((keyword, field));
        }
        Ok(fields)
    }
    /// The same projection as [`Self::program_unmapped`], as the definitions
    /// themselves rather than as text parsed again: each projected resource
    /// with its definitions in `$defs` order, each schema object's keywords in
    /// the order the program's text holds them, and each `const` and `enum`
    /// value copied into the same literal arena. It refuses exactly as
    /// [`Self::program_unmapped`] does.
    pub fn program_definitions(
        &self,
        entry: usize,
        control: &dyn Control,
    ) -> Result<Definitions<'_>, Failure> {
        control.check()?;
        let reach = self.reach(entry, control)?;
        let namespace = PROGRAM_NAMESPACE;
        // The text's fixed bounds refuse with byte offsets in the text. The
        // definitions carry bounds on the text each definition and resource
        // would take, which also bound their nodes (each holds at least a
        // byte of its own) and their nesting (each level at least two). Once
        // a bound could reach a fixed limit, the projection is written as text
        // too, which refuses as it does there or admits the definitions.
        let mut written = false;
        let write = |written: &mut bool| -> Result<(), Failure> {
            if !std::mem::replace(written, true) {
                self.projection(
                    entry,
                    &reach,
                    control,
                    namespace,
                    false,
                    Some(&mut Vec::new()),
                )?;
            }
            Ok(())
        };
        const DEEP: usize = 9_990;
        let mut values = Vec::new();
        let mut separated = 0usize;
        let mut total = 0usize;
        let mut resources: BTreeMap<usize, (usize, Vec<Definition<'_>>)> = BTreeMap::new();
        let mut nodes: Vec<_> = reach.nodes.iter().copied().collect();
        nodes.sort_unstable();
        for id in nodes {
            control.check()?;
            let node = self.node(id);
            let (schema, bound) = match node.value.view().as_bool() {
                Some(value) => (Schema::Boolean(value), node.value.text().len()),
                None => {
                    let fields =
                        self.fields(id, &reach, namespace, true, &mut values, &mut separated)?;
                    let mut bound = 2usize;
                    for (keyword, field) in &fields {
                        let value = match field {
                            Field::Reference(uri) => quoted_bound(uri),
                            Field::Single(address) => quoted_bound(address) + 9,
                            Field::Map(children) => {
                                2 + children
                                    .iter()
                                    .map(|(name, address)| {
                                        quoted_bound(name) + quoted_bound(address) + 11
                                    })
                                    .sum::<usize>()
                            }
                            Field::List(children) => {
                                2 + children
                                    .iter()
                                    .map(|address| quoted_bound(address) + 10)
                                    .sum::<usize>()
                            }
                            Field::Literal(_) => 20,
                            Field::Copied(value) => {
                                let length = value.text().len();
                                if !written
                                    && length >= 2 * DEEP
                                    && !openbindings_internal_json::backend::depth_at_most(
                                        &JsonRef::to_owned(*value),
                                        DEEP,
                                    )
                                {
                                    write(&mut written)?;
                                }
                                length
                            }
                        };
                        bound = bound
                            .saturating_add(quoted_bound(keyword))
                            .saturating_add(value)
                            .saturating_add(2);
                    }
                    (Schema::Object(fields), bound)
                }
            };
            total = total
                .saturating_add(bound)
                .saturating_add(std::mem::take(&mut separated));
            if total > PROJECTION_LIMITS.max_bytes {
                write(&mut written)?;
            }
            let name = format!("n{}", self.node_number(id));
            let (size, definitions) = resources.entry(node.resource).or_default();
            *size = size.saturating_add(bound).saturating_add(name.len() + 4);
            definitions.push(Definition {
                name,
                node: id,
                schema,
            });
        }
        let entry_resource = self.node(entry).resource;
        let entry_uri = self.resource_uri(namespace, entry_resource);
        let mut projected = Vec::with_capacity(resources.len());
        for (id, (size, mut definitions)) in resources {
            let uri = self.resource_uri(namespace, id);
            let entry = (id == entry_resource).then(|| self.address(namespace, entry));
            let size = size
                .saturating_add(64 + DIALECT.len() + uri.len())
                .saturating_add(entry.as_ref().map_or(0, String::len));
            // The node bound; the nesting bound is the copies' checked above.
            if size > PROJECTION_LIMITS.max_nodes {
                write(&mut written)?;
            }
            definitions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            projected.push(DefinedResource {
                uri,
                entry,
                definitions,
            });
        }
        Ok(Definitions {
            entry_uri,
            resources: projected,
            literals: openbindings_internal_json::backend::standalone_list(&values),
        })
    }
    pub fn check_known_node(&self, id: usize) -> Result<(), Failure> {
        let node = self.node(id);
        let resource = self.resource(node.resource);
        if node.depth > 256 {
            return Err(self.failure(
                id,
                FailureReason::LimitExceeded,
                "schema-depth-limit",
                "preparation reached a schema deeper than 256 levels",
            ));
        }
        self.check_resource(node.resource, id)?;
        if !matches!(
            resource.dialect.as_deref(),
            Some(DIALECT) | Some("https://json-schema.org/draft/2020-12/schema#")
        ) {
            return Err(self.failure(
                id,
                FailureReason::UnsupportedCapability,
                "schema-dialect",
                "the resource does not use the supported 2020-12 dialect",
            ));
        }
        // annotations, literal values and child schemas are not interpreted
        // by this node. Children receive their own reached-node checks.
        let interprets_unpaired = node.value.view().members().is_some_and(|mut members| {
            members.any(|member| {
                let Some(keyword) = member.name.as_str() else {
                    return true;
                };
                match keyword {
                    "$id" | "$schema" | "$ref" | "$dynamicRef" | "$anchor" | "$dynamicAnchor"
                    | "type" | "pattern" | "required" | "dependentRequired" => {
                        has_unpaired(&member.value.to_owned())
                    }
                    "properties" | "patternProperties" | "dependentSchemas" => {
                        member.value.members().is_some_and(|mut entries| {
                            entries.any(|entry| entry.name.as_str().is_none())
                        })
                    }
                    _ => false,
                }
            })
        });
        if interprets_unpaired {
            return Err(self.failure(
                id,
                FailureReason::UnsupportedCapability,
                "lone-surrogate-schema",
                "schema interpretation requires Unicode scalar strings",
            ));
        }
        // Only the verdict matters here, so no finding is located. A
        // conformant document's schemas already passed this check.
        let checked = node.source == 0 && self.document.schemas_checked();
        let valid = checked
            || crate::fixed_schema::validity(&node.value, true).map_err(|e| {
                self.failure(id, FailureReason::LimitExceeded, "meta-schema-check", e)
            })?;
        if !valid {
            return Err(self.failure(
                id,
                FailureReason::ConservativePreparation,
                "invalid-schema",
                "the reached schema is not well formed under the 2020-12 meta-schemas",
            ));
        }
        Ok(())
    }
    pub fn reach(&self, entry: usize, control: &dyn Control) -> Result<Reach, Failure> {
        let mut out = Reach::default();
        let mut queue = VecDeque::from([entry]);
        while let Some(id) = queue.pop_front() {
            control.check()?;
            if !out.nodes.insert(id) {
                continue;
            }
            if out.nodes.len() > 100_000 {
                return Err(self.failure(
                    id,
                    FailureReason::LimitExceeded,
                    "schema-node-limit",
                    "preparation reached more than 100,000 schema nodes",
                ));
            }
            self.check_known_node(id)?;
            let node = self.node(id);
            let resource = self.resource(node.resource);
            if out.resources.insert(node.resource) {
                for (name, holders) in &out.dynamic {
                    if let Some(declarations) = resource.anchors.get(name) {
                        if declarations.len() > 1 {
                            return Err(self.failure(id,FailureReason::ConservativePreparation,"ambiguous-dynamic-anchor","a reached resource declares a dynamically looked-up name more than once"));
                        }
                        for &(target, dynamic) in declarations {
                            if dynamic {
                                for &holder in holders {
                                    out.edges.entry(holder).or_default().push(target);
                                }
                                queue.push_back(target);
                            }
                        }
                    }
                }
            }
            let has_if = node.value.get("if").is_some();
            if let Some(members) = node.value.view().members() {
                for member in members {
                    let keyword = member.name.as_str().unwrap_or("");
                    if matches!(keyword, "$ref" | "$dynamicRef") {
                        let reference = member.value.as_str().ok_or_else(|| {
                            self.failure(
                                id,
                                FailureReason::ConservativePreparation,
                                "reference-type",
                                "reference is not a string",
                            )
                        })?;
                        let target = self.resolve(id, reference)?;
                        out.targets
                            .insert((id, keyword == "$dynamicRef"), target.clone());
                        out.edges.entry(id).or_default().push(target.node);
                        queue.push_back(target.node);
                        if keyword == "$dynamicRef"
                            && let Some(name) = target.dynamic_name
                        {
                            out.dynamic.entry(name.clone()).or_default().push(id);
                            for &resource_id in &out.resources {
                                if let Some(declarations) =
                                    self.resource(resource_id).anchors.get(&name)
                                {
                                    if declarations.len() > 1 {
                                        return Err(self.failure(id,FailureReason::ConservativePreparation,"ambiguous-dynamic-anchor","a dynamically looked-up name has multiple declarations"));
                                    }
                                    for &(target, dynamic) in declarations {
                                        if dynamic {
                                            out.edges.entry(id).or_default().push(target);
                                            queue.push_back(target);
                                        }
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    let inplace = matches!(
                        keyword,
                        "allOf"
                            | "anyOf"
                            | "oneOf"
                            | "not"
                            | "if"
                            | "then"
                            | "else"
                            | "dependentSchemas"
                    );
                    let advancing = matches!(
                        keyword,
                        "properties"
                            | "patternProperties"
                            | "additionalProperties"
                            | "propertyNames"
                            | "items"
                            | "prefixItems"
                            | "contains"
                            | "unevaluatedItems"
                            | "unevaluatedProperties"
                    );
                    if (!inplace && !advancing) || (matches!(keyword, "then" | "else") && !has_if) {
                        continue;
                    }
                    let children = keyword_children(keyword, member.value);
                    for child in children {
                        if let Some(&target) =
                            self.source(node.source).positions.get(&ref_id(child))
                        {
                            queue.push_back(target);
                            if inplace {
                                out.edges.entry(id).or_default().push(target);
                            }
                        }
                    }
                }
            }
        }
        self.check_cycles(&out, control)?;
        Ok(out)
    }
    pub fn check_cycles(&self, reach: &Reach, control: &dyn Control) -> Result<(), Failure> {
        // A cycle in potentially reachable in-place applicators is conservative
        // preparation failure. Static reach does not prove semantic undefinedness.
        let mut colors = HashMap::new();
        let mut roots: Vec<_> = reach.nodes.iter().copied().collect();
        roots.sort_unstable();
        for root in roots {
            if colors.get(&root) == Some(&2) {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((id, leaving)) = stack.pop() {
                control.check()?;
                if leaving {
                    colors.insert(id, 2);
                    continue;
                }
                match colors.get(&id) {
                    Some(1) => {
                        return Err(self.failure(
                            id,
                            FailureReason::ConservativePreparation,
                            "in-place-cycle",
                            "static preparation found a potentially evaluated in-place cycle",
                        ));
                    }
                    Some(2) => continue,
                    _ => {}
                }
                colors.insert(id, 1);
                stack.push((id, true));
                for &next in reach.edges.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
                    stack.push((next, false));
                }
            }
        }
        Ok(())
    }
}
pub fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a Rust string serializes as JSON")
}
// At least the length of `quoted(text)`: JSON escapes a control character in
// at most six bytes and a quote or backslash in two.
fn quoted_bound(text: &str) -> usize {
    2 + text
        .bytes()
        .map(|byte| match byte {
            0..0x20 => 6,
            b'"' | b'\\' => 2,
            _ => 1,
        })
        .sum::<usize>()
}
pub fn object(fields: impl IntoIterator<Item = (String, String)>) -> String {
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| format!("{}:{value}", quoted(&key)))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn declared_dialect(value: JsonRef<'_>, inherited: Option<&str>) -> Option<String> {
    match value.get("$schema") {
        Some(v) => v.as_str().map(str::to_owned),
        None => inherited.map(str::to_owned),
    }
}
pub fn keyword_children<'a>(
    keyword: &str,
    value: JsonRef<'a>,
) -> impl Iterator<Item = JsonRef<'a>> {
    let map = matches!(
        keyword,
        "properties" | "patternProperties" | "dependentSchemas"
    );
    let list = matches!(keyword, "allOf" | "anyOf" | "oneOf" | "prefixItems");
    let members = if map { value.members() } else { None };
    let elements = if list { value.elements() } else { None };
    members
        .into_iter()
        .flatten()
        .map(|member| member.value)
        .chain(elements.into_iter().flatten())
        .chain((!map && !list).then_some(value))
}

#[derive(Default)]
pub struct Reach {
    pub nodes: HashSet<usize>,
    pub resources: HashSet<usize>,
    pub edges: HashMap<usize, Vec<usize>>,
    /// Each reference's target, by its holder and whether it is a
    /// `$dynamicRef`.
    pub targets: HashMap<(usize, bool), Target>,
    pub dynamic: BTreeMap<String, Vec<usize>>,
}
impl std::fmt::Debug for Reach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reach").finish_non_exhaustive()
    }
}

/// Borrow pinned original meta-schema snapshots for source-location lookup.
pub fn embedded_resources() -> &'static [ProjectedResource] {
    static RESOURCES: std::sync::OnceLock<Vec<ProjectedResource>> = std::sync::OnceLock::new();
    RESOURCES.get_or_init(|| {
        let files = crate::fixed_schema::META_FILES;
        files
            .into_iter()
            .map(|text| {
                let document = JsonValue::parse(text).expect("pinned meta-schema is JSON");
                let uri = document
                    .get("$id")
                    .and_then(|v| v.as_str())
                    .expect("pinned meta-schema identifier")
                    .into();
                ProjectedResource { uri, document }
            })
            .collect()
    })
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use openbindings_internal_json::backend::{self, FlatJson};
    use std::collections::BTreeSet;
    #[test]
    fn malformed_reached_shapes_refuse_before_projection() {
        let control = std::sync::atomic::AtomicBool::new(false);
        for schema in [
            "17",
            r#"{"properties":[]}"#,
            r#"{"patternProperties":null}"#,
            r#"{"dependentSchemas":false}"#,
            r#"{"allOf":{}}"#,
            r#"{"anyOf":false}"#,
            r#"{"oneOf":{}}"#,
            r#"{"prefixItems":{}}"#,
            r#"{"\ud800":true}"#,
            r#"{"properties":{"\ud800":true}}"#,
        ] {
            for reference in [None, Some("$ref"), Some("$dynamicRef")] {
                let entry_schema = reference.map_or_else(
                    || schema.to_owned(),
                    |keyword| format!(r#"{{"{keyword}":"https://example.invalid/supplied"}}"#),
                );
                let document = RawDocument::parse(format!(
                    r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{entry_schema}}}}}}}"#
                ))
                .unwrap();
                let supplied = if reference.is_some() {
                    Supplied::new([ProjectedResource {
                        uri: "https://example.invalid/supplied".into(),
                        document: JsonValue::parse(schema).unwrap(),
                    }])
                    .unwrap()
                } else {
                    Supplied::default()
                };
                let space = SchemaSpace::new(document, supplied);
                let entry = space
                    .document_node(
                        &space
                            .document
                            .value()
                            .at("/operations/op/input")
                            .unwrap()
                            .to_owned(),
                    )
                    .unwrap();
                let refusal = space.reach(entry, &control).expect_err("malformed reach");
                assert!(
                    matches!(
                        refusal.code.as_str(),
                        "invalid-schema" | "lone-surrogate-schema"
                    ),
                    "{schema} {reference:?}: {refusal:?}"
                );
                let projected = space
                    .program(entry, &control)
                    .expect_err("malformed program");
                assert_eq!(
                    serde_json::to_value(refusal).unwrap(),
                    serde_json::to_value(projected).unwrap()
                );
            }
        }
    }

    fn verdict(program: &Program, value: &JsonValue) -> (bool, Vec<Location>) {
        let mut registry = jsonschema::Registry::new();
        let mut root = None;
        for resource in &program.resources {
            let v: serde_json::Value = serde_json::from_str(resource.document.text()).unwrap();
            if resource.uri == program.entry_uri {
                root = Some(v.clone());
            }
            registry = registry.add(resource.uri.clone(), v).unwrap();
        }
        let registry = registry.prepare().unwrap();
        // The compared projections hold no numeric keyword.
        let validator = jsonschema::options_for::<FlatJson>()
            .with_draft(jsonschema::Draft::Draft202012)
            .with_registry(&registry)
            .build(&root.unwrap())
            .unwrap();
        let valid = validator.is_valid(backend::view(value));
        let paths = validator
            .iter_errors(backend::view(value))
            .map(|e| {
                program
                    .original_location(e.absolute_keyword_location().unwrap().as_str())
                    .unwrap()
            })
            .collect();
        (valid, paths)
    }
    impl SchemaSpace {
        // The space as assembled before the packaged layer was shared: the
        // meta-schemas added to every space as sources after the supplied
        // resources' names, with eligibility finalized over all of them.
        fn flat(document: RawDocument, supplied: Supplied) -> Self {
            static NONE: std::sync::OnceLock<Layer> = std::sync::OnceLock::new();
            let packaged = NONE.get_or_init(|| Layer {
                base: PACKAGED,
                ..Layer::default()
            });
            let mut local = Layer::default();
            let protected = &supplied.policy.protected;
            local.add_source(
                None,
                document.value().clone(),
                document.schema_index(),
                ResourceOrigin::Contained,
                protected,
            );
            for resource in supplied.iter() {
                let index = SchemaIndex::supplied(&resource.document, &resource.uri);
                local.add_source(
                    Some(resource.uri.clone()),
                    resource.document.clone(),
                    &index,
                    ResourceOrigin::Supplied,
                    protected,
                );
            }
            for (name, holders) in &supplied.policy.names {
                for &(source, node) in holders {
                    if let Some(&id) = local.sources[source + 1].positions.get(&node) {
                        let resource = local.nodes[id].resource;
                        local.add_name(name.clone(), resource, protected);
                    }
                }
            }
            for resource in embedded_resources() {
                let index = SchemaIndex::supplied(&resource.document, &resource.uri);
                local.add_source(
                    Some(resource.uri.clone()),
                    resource.document.clone(),
                    &index,
                    ResourceOrigin::Standard,
                    protected,
                );
            }
            local.finalize(|names, name| names.get(name).copied());
            Self {
                document,
                supplied,
                local,
                packaged,
                inspection: std::sync::OnceLock::new(),
            }
        }
    }
    fn program_text(result: Result<Program, Failure>) -> String {
        match result {
            Ok(program) => {
                let mut text = format!("entry {}\n", program.entry_uri);
                for resource in &program.resources {
                    text.push_str(&format!("{} {}\n", resource.uri, resource.document.text()));
                }
                for (uri, origin) in program.locations.iter() {
                    text.push_str(&format!("{uri} {origin:?}\n"));
                }
                text
            }
            Err(failure) => format!(
                "refused {}",
                serde_json::to_string(&failure).expect("failure serializes")
            ),
        }
    }
    #[test]
    fn the_shared_packaged_layer_assembles_the_flat_space() {
        const META: &str = "https://json-schema.org/draft/2020-12/schema";
        const CORE: &str = "https://json-schema.org/draft/2020-12/meta/core";
        let resource = |uri: &str, text: &str| ProjectedResource {
            uri: uri.into(),
            document: JsonValue::parse(text).unwrap(),
        };
        let document = |input: &str, schemas: &str| {
            format!(
                r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{input}}}}},"schemas":{{{schemas}}}}}"#
            )
        };
        let cases: Vec<(String, Vec<ProjectedResource>)> = vec![
            (document(r#"{"type":"string"}"#, ""), vec![]),
            // The meta-schema as a value's schema, and a dynamic reference into it.
            (document(&format!(r#"{{"$ref":"{META}"}}"#), ""), vec![]),
            (
                document(
                    &format!(
                        r##"{{"$dynamicRef":"{META}#meta","properties":{{"t":{{"$ref":"https://json-schema.org/draft/2020-12/meta/validation#/$defs/simpleTypes"}}}}}}"##
                    ),
                    "",
                ),
                vec![],
            ),
            // A contained resource claiming a packaged identifier keeps it.
            (
                document(
                    &format!(r#"{{"allOf":[{{"$ref":"{CORE}"}},{{"$ref":"{META}"}}]}}"#),
                    &format!(r#""core":{{"$id":"{CORE}","type":"object"}}"#),
                ),
                vec![],
            ),
            // A supplied resource claiming a packaged identifier, by retrieval
            // URI and by a nested identifier, leaves it without one carrier.
            (
                document(&format!(r#"{{"$ref":"{META}"}}"#), ""),
                vec![resource(META, r#"{"type":"integer"}"#)],
            ),
            (
                document(
                    &format!(
                        r#"{{"allOf":[{{"$ref":"https://supplied.test/a"}},{{"$ref":"{CORE}"}}]}}"#
                    ),
                    "",
                ),
                vec![resource(
                    "https://supplied.test/a",
                    &format!(r#"{{"$defs":{{"c":{{"$id":"{CORE}"}}}},"$ref":"{META}"}}"#),
                )],
            ),
            // Supplied resources before the packaged ones, with anchors and a
            // dynamic anchor the meta-schemas also declare.
            (
                document(
                    &format!(
                        r##"{{"anyOf":[{{"$ref":"https://supplied.test/b#x"}},{{"$ref":"https://supplied.test/c"}},{{"$ref":"{META}"}}]}}"##
                    ),
                    r#""s":{"type":"string"}"#,
                ),
                vec![
                    resource(
                        "https://supplied.test/b",
                        r##"{"$defs":{"x":{"$anchor":"x","$dynamicAnchor":"meta","type":"null"}},"$ref":"#x"}"##,
                    ),
                    resource(
                        "https://supplied.test/c",
                        &format!(
                            r##"{{"$dynamicRef":"{META}#meta","$defs":{{"d":{{"$id":"https://supplied.test/d"}}}}}}"##
                        ),
                    ),
                ],
            ),
            // Two supplied claims to one identifier, a missing resource and a cycle.
            (
                document(r#"{"$ref":"https://supplied.test/b"}"#, ""),
                vec![
                    resource("https://supplied.test/b", r#"{"type":"null"}"#),
                    resource(
                        "https://supplied.test/other",
                        r#"{"$id":"https://supplied.test/b"}"#,
                    ),
                ],
            ),
            (document(r#"{"$ref":"https://absent.test/c"}"#, ""), vec![]),
            (
                document(
                    r##"{"$ref":"#/schemas/loop"}"##,
                    r##""loop":{"allOf":[{"$ref":"#/schemas/loop"}]}"##,
                ),
                vec![],
            ),
        ];
        let control = std::sync::atomic::AtomicBool::new(false);
        let (mut programs, mut projected_packaged) = (0, 0);
        for (text, resources) in cases {
            let space = |flat: bool| {
                let document = RawDocument::parse(&text).unwrap();
                let supplied = Supplied::new(resources.clone()).unwrap();
                if flat {
                    SchemaSpace::flat(document, supplied)
                } else {
                    SchemaSpace::new(document, supplied)
                }
            };
            let (layered, flat) = (space(false), space(true));
            // Identifiers correspond through the dense numbering.
            assert_eq!(layered.nodes().count(), flat.nodes().count(), "{text}");
            for ((id, node), (number, original)) in layered.nodes().zip(flat.nodes()) {
                assert_eq!(layered.node_number(id), number);
                assert_eq!(node_id(&node.value), node_id(&original.value));
                assert_eq!(layered.resource_number(node.resource), original.resource);
                assert_eq!(node.depth, original.depth);
                let (source, other) = (layered.source(node.source), flat.source(original.source));
                assert_eq!(source.uri, other.uri);
                assert_eq!(node_id(&source.value), node_id(&other.value));
                assert_eq!(
                    source.positions.get(&node_id(&node.value)).copied(),
                    Some(id)
                );
            }
            assert_eq!(layered.resources().count(), flat.resources().count());
            for ((id, resource), (number, original)) in layered.resources().zip(flat.resources()) {
                assert_eq!(layered.resource_number(id), number);
                assert_eq!(resource.root.map(|n| layered.node_number(n)), original.root);
                assert_eq!(resource.base, original.base);
                assert_eq!(resource.anonymous, original.anonymous);
                assert_eq!(resource.dialect, original.dialect);
                assert_eq!(resource.origin, original.origin);
                let anchors = |space: &SchemaSpace, r: &Resource| {
                    r.anchors
                        .iter()
                        .map(|(name, at)| {
                            let at: Vec<_> =
                                at.iter().map(|&(n, d)| (space.node_number(n), d)).collect();
                            (name.clone(), at)
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(anchors(&layered, resource), anchors(&flat, original));
                assert_eq!(
                    layered.eligible(id),
                    flat.eligible(number),
                    "{text} {:?}",
                    resource.base
                );
            }
            let names: BTreeSet<_> = layered
                .local
                .names
                .keys()
                .chain(layered.packaged.names.keys())
                .collect();
            assert!(names.iter().copied().eq(flat.local.names.keys()));
            for name in flat.local.names.keys() {
                let (selected, original) = (layered.name(name).unwrap(), flat.name(name).unwrap());
                assert_eq!(selected.origin, original.origin, "{name}");
                assert_eq!(
                    selected.carrier.map(|r| layered.resource_number(r)),
                    original.carrier,
                    "{name}"
                );
                assert!(layered.has_name(name));
            }
            // Every document schema projects identically, refusals included.
            assert_eq!(
                serde_json::to_string(&layered.references(&control)).unwrap(),
                serde_json::to_string(&flat.references(&control)).unwrap()
            );
            for (id, _) in layered.local.nodes().filter(|(_, n)| n.source == 0) {
                let projected = program_text(layered.program(id, &control));
                assert_eq!(
                    projected,
                    program_text(flat.program(id, &control)),
                    "{text} {id}"
                );
                programs += 1;
                projected_packaged += usize::from(projected.contains("json-schema.org"));
            }
        }
        assert!(
            programs >= 20 && projected_packaged >= 4,
            "{programs} {projected_packaged}"
        );
    }
    #[test]
    fn unmapped_programs_are_the_same_projection_and_name_every_origin() {
        // A document schema, a supplied resource, an embedded resource and
        // names needing pointer escapes.
        let document = RawDocument::parse(
            r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"allOf":[{"$ref":"#/schemas/a~1b"},{"$ref":"https://example.invalid/tree"}]}}},"schemas":{"a/b":{"properties":{"~x":{"$id":"https://example.invalid/inner","minimum":1}}}}}"##,
        )
        .unwrap();
        let resources = Supplied::new([ProjectedResource {
            uri: "https://example.invalid/tree".into(),
            document: JsonValue::parse(r#"{"items":{"type":"string"},"prefixItems":[true]}"#)
                .unwrap(),
        }])
        .unwrap();
        let space = SchemaSpace::new(document, resources);
        let entry = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        let mapped = space.program(entry, &control).unwrap();
        let unmapped = space.program_unmapped(entry, &control).unwrap();
        assert!(unmapped.program.locations.is_empty());
        assert_eq!(mapped.entry_uri, unmapped.program.entry_uri);
        assert_eq!(mapped.resources.len(), unmapped.definitions.len());
        let (mut out, mut scratch) = (Vec::new(), Vec::new());
        let mut checked = 0;
        for ((resource, other), nodes) in mapped
            .resources
            .iter()
            .zip(&unmapped.program.resources)
            .zip(&unmapped.definitions)
        {
            assert_eq!(resource.uri, other.uri);
            assert_eq!(resource.document.text(), other.document.text());
            let names = resource.document.get("$defs").unwrap().members().unwrap();
            assert_eq!(names.len(), nodes.len());
            for (definition, &node) in names.zip(nodes) {
                let key = format!(
                    "{}#/$defs/{}",
                    resource.uri,
                    definition.name.as_str().unwrap()
                );
                let ProgramOrigin::Prefix(at) = &mapped.locations[&key] else {
                    panic!("strict projections map prefix origins")
                };
                out.clear();
                assert!(space.location_into(node, &mut out, &mut scratch), "{key}");
                assert_eq!(space.source_uri(node), at.resource.as_deref(), "{key}");
                assert_eq!(at.pointer, std::str::from_utf8(&out).unwrap(), "{key}");
                checked += 1;
            }
        }
        assert_eq!(checked, mapped.locations.len());
    }
    #[test]
    fn origins_through_unpaired_names_are_left_to_the_exact_location() {
        // An anchor reaches a schema beneath a name no reference can spell.
        let document = RawDocument::parse(
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://loc.test/r#an"}}}}"#,
        )
        .unwrap();
        let resources = Supplied::new([ProjectedResource {
            uri: "https://loc.test/r".into(),
            document: JsonValue::parse(
                r#"{"minimum":0,"$defs":{"\ud800/":{"$anchor":"an","minimum":3}}}"#,
            )
            .unwrap(),
        }])
        .unwrap();
        let space = SchemaSpace::new(document, resources);
        let entry = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        let mapped = space.program(entry, &control).unwrap();
        let unmapped = space.program_unmapped(entry, &control).unwrap();
        let (mut out, mut scratch) = (b"held".to_vec(), Vec::new());
        let mut declined = 0;
        for &node in unmapped.definitions.iter().flatten() {
            let at = space.location(node);
            if space.location_into(node, &mut out, &mut scratch) {
                assert_eq!(at.pointer, std::str::from_utf8(&out[4..]).unwrap());
            } else {
                // Nothing is appended, and the location keeps every unit, as the
                // mapped projection's origin does.
                assert_eq!(out, b"held");
                assert!(at.pointer.as_str().is_none());
                assert_eq!(at.resource.as_deref(), Some("https://loc.test/r"));
                assert_eq!(at.pointer.to_string(), "/$defs/\\ud800~1");
                assert!(mapped.locations.values().any(
                    |origin| matches!(origin, ProgramOrigin::Prefix(mapped) if *mapped == at)
                ));
                declined += 1;
            }
            out.truncate(4);
        }
        assert_eq!(declined, 1);
    }
    #[test]
    fn generated_name_changes_preserve_dynamic_scope_annotations_opaque_strings_and_locations() {
        let document=RawDocument::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/schemas/strict"}}},"schemas":{"strict":{"$dynamicAnchor":"node","$ref":"https://example.invalid/tree","unevaluatedProperties":false}}}"##).unwrap();
        let resources=Supplied::new([ProjectedResource{uri:"https://example.invalid/tree".into(),document:JsonValue::parse(r##"{"$dynamicAnchor":"node","type":"object","properties":{"children":{"type":"array","items":{"$dynamicRef":"#node"}},"literal":{"const":"https://sdk-program.openbindings.invalid/r0#/$defs/n0"}}}"##).unwrap()}]).unwrap();
        let space = SchemaSpace::new(document, resources);
        let entry = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        let control = std::sync::atomic::AtomicBool::new(false);
        let reach = space.reach(entry, &control).unwrap();
        let standard = space.project(entry, &reach, &control).unwrap();
        for namespace in [
            "https://different.invalid/longer/path/",
            "urn:example:generated:",
        ] {
            let renamed = space
                .project_with_namespace(entry, &reach, &control, namespace)
                .unwrap();
            assert_ne!(standard.entry_uri, renamed.entry_uri);
            for text in [
                r#"{"children":[]}"#,
                r#"{"children":[{"unexpected":1}]}"#,
                r#"{"literal":"https://sdk-program.openbindings.invalid/r0#/$defs/n0"}"#,
                r#"{"literal":"wrong"}"#,
            ] {
                let value = JsonValue::parse(text).unwrap();
                assert_eq!(
                    verdict(&standard, &value),
                    verdict(&renamed, &value),
                    "{namespace} {text}"
                );
            }
        }
    }
}

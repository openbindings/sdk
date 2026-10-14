//! Original-context resource graph. No URI here acquires bytes.
mod library;
use crate::{
    schema_index::{DIALECT, SchemaIndex},
    uri, *,
};
use openbindings_internal_json::backend::{has_unpaired, node_id, pointer, pointer_size};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub struct SpaceNode {
    pub value: JsonValue,
    pub resource: usize,
    pub source: usize,
    pub depth: usize,
}
pub struct Resource {
    pub root: Option<usize>,
    pub base: Option<String>,
    pub anonymous: bool,
    pub dialect: Option<String>,
    pub anchors: BTreeMap<String, Vec<(usize, bool)>>,
    pub origin: ResourceOrigin,
    pub eligible: bool,
}
// Priority applies to name associations, not bodies or source provenance. A
// supplied retrieval alias competes equally with every supplied declared ID.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceOrigin {
    Contained,
    Supplied,
    Standard,
}
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
pub struct SchemaSpace {
    pub document: RawDocument,
    pub supplied: Supplied,
    pub nodes: Vec<SpaceNode>,
    pub resources: Vec<Resource>,
    pub sources: Vec<Source>,
    names: BTreeMap<String, NameSelection>,
    library: std::sync::OnceLock<library::Targets>,
}
#[derive(Clone)]
pub struct Target {
    pub node: usize,
    pub dynamic_name: Option<String>,
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
#[derive(Clone, Debug, Serialize)]
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

impl SchemaSpace {
    pub fn new(document: RawDocument, supplied: Supplied) -> Self {
        let mut out = Self {
            document: document.clone(),
            supplied: supplied.clone(),
            nodes: Vec::new(),
            resources: Vec::new(),
            sources: Vec::new(),
            names: BTreeMap::new(),
            library: std::sync::OnceLock::new(),
        };
        out.add_source(
            None,
            document.value().clone(),
            document.schema_index(),
            ResourceOrigin::Contained,
        );
        for resource in supplied.iter() {
            let index = SchemaIndex::supplied(&resource.document, &resource.uri);
            out.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &index,
                ResourceOrigin::Supplied,
            );
        }
        // Canonical/retrieval associations and protected standard identities
        // originate in the immutable supplied-resource input policy.
        for (name, holders) in &supplied.policy.names {
            for &(source, node) in holders {
                if let Some(id) = out.sources[source + 1].positions.get(&node) {
                    let resource = out.nodes[*id].resource;
                    out.add_name(name.clone(), resource);
                }
            }
        }
        for resource in embedded_resources() {
            let index = SchemaIndex::supplied(&resource.document, &resource.uri);
            out.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &index,
                ResourceOrigin::Standard,
            );
        }
        // Finalize once, after every alias and canonical association is known.
        // An independently named child does not inherit its parent's eligibility.
        for (id, resource) in out.resources.iter_mut().enumerate() {
            if !resource.anonymous
                && let Some(base) = &resource.base
            {
                resource.eligible = out.names.get(base).and_then(|name| name.carrier) == Some(id);
            }
        }
        out
    }
    pub fn add_source(
        &mut self,
        retrieval: Option<String>,
        value: JsonValue,
        index: &SchemaIndex,
        origin: ResourceOrigin,
    ) {
        let source = self.sources.len();
        let start = self.nodes.len();
        let mut resources = HashMap::new();
        let initial = self.resources.len();
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
            self.resources[initial].dialect = declared_dialect(value.view(), Some(DIALECT));
        }
        let mut positions = HashMap::new();
        for node in &index.nodes {
            let resource = if let Some(&id) = resources.get(&node.resource) {
                id
            } else {
                let parent = *resources.get(&node.parent_resource).unwrap_or(&initial);
                let id = self.resources.len();
                resources.insert(node.resource, id);
                let dialect =
                    declared_dialect(node.value.view(), self.resources[parent].dialect.as_deref());
                self.resources.push(Resource {
                    root: Some(self.nodes.len()),
                    base: node
                        .value
                        .get("$id")
                        .and_then(|v| v.as_str())
                        .and_then(|id| {
                            crate::input_policy::resolve_identifier(
                                self.resources[parent].base.as_deref(),
                                id,
                            )
                        }),
                    anonymous: false,
                    dialect,
                    anchors: BTreeMap::new(),
                    origin,
                    eligible: true,
                });
                id
            };
            let id = self.nodes.len();
            positions.insert(node_id(&node.value), id);
            self.nodes.push(SpaceNode {
                value: node.value.clone(),
                resource,
                source,
                depth: node.depth,
            });
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if let Some(name) = node
                    .value
                    .get(keyword)
                    .and_then(|v| v.as_str())
                    .filter(|s| crate::schema_index::plain_name(s))
                {
                    self.resources[resource]
                        .anchors
                        .entry(name.into())
                        .or_default()
                        .push((id, keyword == "$dynamicAnchor"));
                }
            }
        }
        if retrieval.is_some() && start < self.nodes.len() {
            let root_resource = self.nodes[start].resource;
            self.resources[root_resource].root = Some(start);
            if origin != ResourceOrigin::Supplied
                && let Some(retrieval) = &retrieval
            {
                self.add_name(retrieval.clone(), root_resource);
            }
        }
        for &id in resources.values() {
            if origin != ResourceOrigin::Supplied
                && let Some(base) = self.resources[id].base.clone()
                && self.resources[id].root.is_some()
            {
                self.add_name(base, id);
            }
        }
        self.sources.push(Source {
            uri: retrieval,
            value,
            positions,
        });
    }
    fn add_name(&mut self, name: String, resource: usize) {
        let Some(name) = crate::input_policy::canonical_uri(&name) else {
            return;
        };
        let origin = self.resources[resource].origin;
        if origin == ResourceOrigin::Supplied
            && self.supplied.policy.protected.contains(&name)
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
    pub fn check_resource(&self, resource: usize, at: usize) -> Result<(), Failure> {
        if self.resources[resource].eligible {
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
        let node = &self.nodes[at];
        let source = &self.sources[node.source];
        let uri_bytes = source.uri.as_ref().map_or(0, String::len);
        if let Some(remaining) = LOCATION_BYTES.checked_sub(uri_bytes)
            && pointer_size(node.value.view(), remaining).is_some()
            && pointer_size(source.value.view(), remaining).is_some()
        {
            detail.location = Some(self.location(at));
        }
        Err(detail)
    }
    pub fn document_node(&self, value: &JsonValue) -> Option<usize> {
        self.sources[0].positions.get(&node_id(value)).copied()
    }
    pub fn location(&self, node: usize) -> Location {
        let node = &self.nodes[node];
        let source = &self.sources[node.source];
        let prefix = pointer(&source.value).unwrap_or_default();
        let full = pointer(&node.value).unwrap_or_default();
        Location {
            resource: source.uri.clone(),
            pointer: full.strip_prefix(&prefix).unwrap_or(&full).into(),
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
    pub fn resolve_with_missing(
        &self,
        holder: usize,
        reference: &str,
        missing: &mut impl FnMut(&str),
    ) -> Result<Target, Failure> {
        let at = &self.nodes[holder];
        let resource = &self.resources[at.resource];
        let fail = |code: &str, message: String| {
            self.failure(
                holder,
                FailureReason::ConservativePreparation,
                code,
                message,
            )
        };
        if !uri::valid(reference) {
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
                    self.check_resource(self.nodes[node].resource, holder)?;
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
        for (id, node) in self.nodes.iter().enumerate().filter(|(_, n)| n.source == 0) {
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
                    location.pointer.push('/');
                    location.pointer.push_str(keyword);
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
    pub fn project(
        &self,
        entry: usize,
        reach: &Reach,
        control: &dyn Control,
    ) -> Result<Program, Failure> {
        self.project_with_namespace(
            entry,
            reach,
            control,
            "https://sdk-program.openbindings.invalid/",
        )
    }
    pub fn project_with_namespace(
        &self,
        entry: usize,
        reach: &Reach,
        control: &dyn Control,
        namespace: &str,
    ) -> Result<Program, Failure> {
        let resource_uri = |resource: usize| format!("{namespace}r{resource}");
        let address =
            |node: usize| format!("{}#/$defs/n{node}", resource_uri(self.nodes[node].resource));
        let reference = |node: usize| format!("{{\"$ref\":{}}}", quoted(&address(node)));
        let mut definitions: BTreeMap<usize, BTreeMap<String, String>> = BTreeMap::new();
        let mut locations = BTreeMap::new();
        let mut byte_count = 0usize;
        let mut nodes: Vec<_> = reach.nodes.iter().copied().collect();
        nodes.sort_unstable();
        for id in nodes {
            control.check()?;
            let node = &self.nodes[id];
            let transformed = if node.value.kind() == JsonKind::Boolean {
                node.value.text().to_owned()
            } else {
                let mut fields = BTreeMap::new();
                for member in node.value.view().members().ok_or_else(|| {
                    self.failure(
                        id,
                        FailureReason::ConservativePreparation,
                        "schema-type",
                        "a schema is an object or boolean",
                    )
                })? {
                    let keyword = member.name.as_str().ok_or_else(|| {
                        self.failure(
                            id,
                            FailureReason::UnsupportedCapability,
                            "schema-string",
                            "schema member is not a Unicode scalar string",
                        )
                    })?;
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
                        let target = &reach.targets[&(id, keyword.into())];
                        let uri = if keyword == "$dynamicRef" {
                            if let Some(name) = &target.dynamic_name {
                                format!("{}#{name}", resource_uri(self.nodes[target.node].resource))
                            } else {
                                address(target.node)
                            }
                        } else {
                            address(target.node)
                        };
                        fields.insert(keyword.to_owned(), quoted(&uri));
                        continue;
                    }
                    let child_reference = |value: JsonRef<'_>| -> Result<String, Failure> {
                        let target = self.sources[node.source]
                            .positions
                            .get(&node_id(&value.to_owned()))
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
                        Ok(reference(target))
                    };
                    let value = match keyword {
                        "properties" | "patternProperties" | "dependentSchemas" => {
                            let mut children = BTreeMap::new();
                            for m in member.value.members().ok_or_else(|| {
                                self.failure(
                                    id,
                                    FailureReason::ConservativePreparation,
                                    "schema-shape",
                                    "schema map is not an object",
                                )
                            })? {
                                children.insert(
                                    m.name.as_str().unwrap().to_owned(),
                                    child_reference(m.value)?,
                                );
                            }
                            object(children)
                        }
                        "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                            let children: Result<Vec<_>, _> = member
                                .value
                                .elements()
                                .ok_or_else(|| {
                                    self.failure(
                                        id,
                                        FailureReason::ConservativePreparation,
                                        "schema-shape",
                                        "schema list is not an array",
                                    )
                                })?
                                .map(child_reference)
                                .collect();
                            format!("[{}]", children?.join(","))
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
                        | "else" => child_reference(member.value)?,
                        _ => member.value.text().to_owned(),
                    };
                    fields.insert(keyword.to_owned(), value);
                }
                object(fields)
            };
            byte_count = byte_count.saturating_add(transformed.len());
            if byte_count > 64 * 1024 * 1024 {
                return Err(self.failure(
                    id,
                    FailureReason::LimitExceeded,
                    "program-byte-limit",
                    "schema projection exceeds 64 MiB",
                ));
            }
            locations.insert(address(id), crate::ProgramOrigin::Prefix(self.location(id)));
            definitions
                .entry(node.resource)
                .or_default()
                .insert(format!("n{id}"), transformed);
        }
        let entry_resource = self.nodes[entry].resource;
        let entry_uri = resource_uri(entry_resource);
        let resources = definitions
            .into_iter()
            .map(|(id, defs)| {
                let uri = resource_uri(id);
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
                JsonValue::parse(object(fields))
                    .map(|document| ProjectedResource { uri, document })
                    .map_err(|e| {
                        Failure::new(
                            FailureReason::LimitExceeded,
                            "program-json-limit",
                            e.to_string(),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Program {
            entry_uri,
            resources,
            locations: std::sync::Arc::new(locations),
        })
    }
    pub fn check_known_node(&self, id: usize) -> Result<(), Failure> {
        let node = &self.nodes[id];
        let resource = &self.resources[node.resource];
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
        // EV-3: annotations, literal values and child schemas are not interpreted
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
        let checks = crate::fixed_schema::check(&node.value, true, 1)
            .map_err(|e| self.failure(id, FailureReason::LimitExceeded, "meta-schema-check", e))?;
        if checks.violated {
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
            let node = &self.nodes[id];
            let resource = &self.resources[node.resource];
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
                        out.targets.insert((id, keyword.into()), target.clone());
                        out.edges.entry(id).or_default().push(target.node);
                        queue.push_back(target.node);
                        if keyword == "$dynamicRef"
                            && let Some(name) = target.dynamic_name
                        {
                            out.dynamic.entry(name.clone()).or_default().push(id);
                            for &resource_id in &out.resources {
                                if let Some(declarations) =
                                    self.resources[resource_id].anchors.get(&name)
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
                        if let Some(&target) = self.sources[node.source]
                            .positions
                            .get(&node_id(&child.to_owned()))
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
pub fn object(fields: BTreeMap<String, String>) -> String {
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
    pub targets: HashMap<(usize, String), Target>,
    pub dynamic: BTreeMap<String, Vec<usize>>,
}
/// Borrow pinned original meta-schema snapshots for source-location lookup.
pub fn embedded_resources() -> &'static [ProjectedResource] {
    static RESOURCES: std::sync::OnceLock<Vec<ProjectedResource>> = std::sync::OnceLock::new();
    RESOURCES.get_or_init(|| {
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
    use openbindings_internal_json::{
        backend::{self, FlatJson},
        numeric,
    };
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
        let validator = numeric::apply(jsonschema::options_for::<FlatJson>())
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

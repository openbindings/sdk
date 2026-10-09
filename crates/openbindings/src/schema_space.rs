//! Original-context resource graph. No URI here acquires bytes.
use crate::{
    schema_index::{DIALECT, SchemaIndex},
    uri, *,
};
use openbindings_internal_json::backend::{has_unpaired, node_id, pointer};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub(crate) struct SpaceNode {
    pub value: JsonValue,
    pub resource: usize,
    pub source: usize,
    pub depth: usize,
}
struct Resource {
    root: Option<usize>,
    base: Option<String>,
    anonymous: bool,
    dialect: Option<String>,
    anchors: BTreeMap<String, Vec<(usize, bool)>>,
}
struct Source {
    uri: Option<String>,
    value: JsonValue,
    positions: HashMap<usize, usize>,
}
pub(crate) struct SchemaSpace {
    pub document: ParsedDocument,
    pub supplied: ResourceSet,
    pub nodes: Vec<SpaceNode>,
    resources: Vec<Resource>,
    sources: Vec<Source>,
    names: BTreeMap<String, Vec<usize>>,
}
#[derive(Clone)]
struct Target {
    node: usize,
    dynamic_name: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ReferenceResolution {
    Located {
        target: SchemaLocation,
        dynamic_lookup: bool,
    },
    Unresolved {
        detail: NoVerdict,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub location: SchemaLocation,
    pub keyword: String,
    pub spelling: Option<String>,
    pub resolution: ReferenceResolution,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReferenceReport {
    pub references: Vec<Reference>,
    pub complete: bool,
    pub limitation: Option<NoVerdict>,
}
use serde::Serialize;

impl SchemaSpace {
    pub fn new(document: ParsedDocument, supplied: ResourceSet) -> Self {
        let mut out = Self {
            document: document.clone(),
            supplied: supplied.clone(),
            nodes: Vec::new(),
            resources: Vec::new(),
            sources: Vec::new(),
            names: BTreeMap::new(),
        };
        out.add_source(None, document.value().clone(), document.schemas(), false);
        for resource in supplied.iter() {
            let index = SchemaIndex::supplied(&resource.document, &resource.uri);
            out.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &index,
                false,
            );
        }
        for resource in embedded_resources() {
            let index = SchemaIndex::supplied(&resource.document, &resource.uri);
            out.add_source(
                Some(resource.uri.clone()),
                resource.document.clone(),
                &index,
                true,
            );
        }
        out
    }
    fn add_source(
        &mut self,
        retrieval: Option<String>,
        value: JsonValue,
        index: &SchemaIndex,
        fallback: bool,
    ) {
        let source = self.sources.len();
        let start = self.nodes.len();
        let mut resources = HashMap::new();
        let initial = self.resources.len();
        resources.insert(0, initial);
        self.resources.push(Resource {
            root: None,
            base: retrieval.clone(),
            anonymous: retrieval.is_none(),
            dialect: Some(DIALECT.into()),
            anchors: BTreeMap::new(),
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
                    base: node.base.clone(),
                    anonymous: false,
                    dialect,
                    anchors: BTreeMap::new(),
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
            if let Some(retrieval) = &retrieval {
                self.add_name(retrieval.clone(), root_resource, fallback);
            }
        }
        for &id in resources.values() {
            if let Some(base) = self.resources[id].base.clone()
                && self.resources[id].root.is_some()
            {
                self.add_name(base, id, fallback);
            }
        }
        self.sources.push(Source {
            uri: retrieval,
            value,
            positions,
        });
    }
    fn add_name(&mut self, name: String, resource: usize, fallback: bool) {
        if fallback && self.names.contains_key(&name) {
            return;
        }
        let list = self.names.entry(name).or_default();
        if !list.contains(&resource) {
            list.push(resource);
        }
    }
    pub fn document_node(&self, value: &JsonValue) -> Option<usize> {
        self.sources[0].positions.get(&node_id(value)).copied()
    }
    pub fn location(&self, node: usize) -> SchemaLocation {
        let node = &self.nodes[node];
        let source = &self.sources[node.source];
        let prefix = pointer(&source.value).unwrap_or_default();
        let full = pointer(&node.value).unwrap_or_default();
        SchemaLocation {
            resource: source.uri.clone(),
            pointer: full.strip_prefix(&prefix).unwrap_or(&full).into(),
        }
    }
    fn failure(
        &self,
        node: usize,
        reason: NoVerdictReason,
        code: &str,
        message: impl Into<String>,
    ) -> NoVerdict {
        NoVerdict::new(reason, code, message).located(self.location(node))
    }
    fn resolve(&self, holder: usize, reference: &str) -> Result<Target, NoVerdict> {
        let at = &self.nodes[holder];
        let resource = &self.resources[at.resource];
        let fail = |code: &str, message: String| {
            self.failure(
                holder,
                NoVerdictReason::ConservativePreparation,
                code,
                message,
            )
        };
        if !uri::valid(reference) {
            return Err(fail(
                "invalid-reference",
                format!("not a well-formed URI reference: {reference}"),
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
                .schemas()
                .same_document(self.document.value(), reference)
            {
                crate::schema_index::SameDocument::Found(value) => {
                    let node = self.document_node(&value).ok_or_else(|| {
                        fail(
                            "unindexed-target",
                            "target is outside the schema index".into(),
                        )
                    })?;
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
        let resolved = if uri::absolute(reference) {
            uri::resolve(reference, reference)
        } else {
            resource
                .base
                .as_deref()
                .and_then(|base| uri::resolve(base, reference))
        }
        .ok_or_else(|| {
            fail(
                "missing-base",
                "the resource has no usable identifier for relative resolution".into(),
            )
        })?;
        let (name, fragment) = resolved.split_once('#').unwrap_or((&resolved, ""));
        let fragment = uri::decode_fragment(fragment).ok_or_else(|| {
            fail(
                "fragment-encoding",
                "the reference fragment does not decode to UTF-8".into(),
            )
        })?;
        let carriers = self.names.get(name).map(Vec::as_slice).unwrap_or(&[]);
        let resource = match carriers {
            [] => {
                return Err(self.failure(
                    holder,
                    NoVerdictReason::ResourceUnavailable,
                    "resource-unavailable",
                    format!("static preparation requires an unsupplied resource: {name}"),
                ));
            }
            [id] => &self.resources[*id],
            _ => {
                return Err(fail(
                    "ambiguous-resource",
                    format!("more than one resource carries the exact identifier {name}"),
                ));
            }
        };
        if fragment.is_empty() {
            return resource
                .root
                .map(|node| Target {
                    node,
                    dynamic_name: None,
                })
                .ok_or_else(|| {
                    fail(
                        "resource-root",
                        "resource has no indexed schema root".into(),
                    )
                });
        }
        if fragment.starts_with('/') {
            let root = resource
                .root
                .ok_or_else(|| fail("resource-root", "resource has no indexed root".into()))?;
            let root = &self.nodes[root];
            let node = root
                .value
                .at(&fragment)
                .and_then(|value| {
                    self.sources[root.source]
                        .positions
                        .get(&node_id(&value.to_owned()))
                        .copied()
                })
                .ok_or_else(|| {
                    fail(
                        "non-schema-target",
                        "resource pointer does not identify an indexed schema".into(),
                    )
                })?;
            return Ok(Target {
                node,
                dynamic_name: None,
            });
        }
        match resource
            .anchors
            .get(&fragment)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            [(node, dynamic)] => Ok(Target {
                node: *node,
                dynamic_name: dynamic.then_some(fragment),
            }),
            [] => Err(fail(
                "anchor-missing",
                "resource declares no such plain name".into(),
            )),
            _ => Err(fail(
                "ambiguous-anchor",
                "resource declares this plain name more than once".into(),
            )),
        }
    }
    fn references(&self, control: &WorkControl) -> ReferenceReport {
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
                        NoVerdictReason::LimitExceeded,
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
                                NoVerdictReason::ConservativePreparation,
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
    pub fn program(
        &self,
        entry: usize,
        control: &WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        control.check()?;
        let reach = self.reach(entry, control)?;
        self.project(entry, &reach, control)
    }
    fn project(
        &self,
        entry: usize,
        reach: &Reach,
        control: &WorkControl,
    ) -> Result<EvaluationProgram, NoVerdict> {
        self.project_with_namespace(
            entry,
            reach,
            control,
            "https://sdk-program.openbindings.invalid/",
        )
    }
    fn project_with_namespace(
        &self,
        entry: usize,
        reach: &Reach,
        control: &WorkControl,
        namespace: &str,
    ) -> Result<EvaluationProgram, NoVerdict> {
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
                        NoVerdictReason::ConservativePreparation,
                        "schema-type",
                        "a schema is an object or boolean",
                    )
                })? {
                    let keyword = member.name.as_str().ok_or_else(|| {
                        self.failure(
                            id,
                            NoVerdictReason::UnsupportedCapability,
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
                    let child_reference = |value: JsonRef<'_>| -> Result<String, NoVerdict> {
                        let target = self.sources[node.source]
                            .positions
                            .get(&node_id(&value.to_owned()))
                            .copied()
                            .filter(|n| reach.nodes.contains(n))
                            .ok_or_else(|| {
                                self.failure(
                                    id,
                                    NoVerdictReason::ConservativePreparation,
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
                                    NoVerdictReason::ConservativePreparation,
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
                                        NoVerdictReason::ConservativePreparation,
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
                    NoVerdictReason::LimitExceeded,
                    "program-byte-limit",
                    "schema projection exceeds 64 MiB",
                ));
            }
            locations.insert(address(id), self.location(id));
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
                    .map(|document| SchemaResource { uri, document })
                    .map_err(|e| {
                        NoVerdict::new(
                            NoVerdictReason::LimitExceeded,
                            "program-json-limit",
                            e.to_string(),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EvaluationProgram {
            entry_uri,
            resources,
            locations,
        })
    }
    fn reach(&self, entry: usize, control: &WorkControl) -> Result<Reach, NoVerdict> {
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
                    NoVerdictReason::LimitExceeded,
                    "schema-node-limit",
                    "preparation reached more than 100,000 schema nodes",
                ));
            }
            let node = &self.nodes[id];
            let resource = &self.resources[node.resource];
            if node.depth > 256 {
                return Err(self.failure(
                    id,
                    NoVerdictReason::LimitExceeded,
                    "schema-depth-limit",
                    "preparation reached a schema deeper than 256 levels",
                ));
            }
            if !matches!(
                resource.dialect.as_deref(),
                Some(DIALECT) | Some("https://json-schema.org/draft/2020-12/schema#")
            ) {
                return Err(self.failure(
                    id,
                    NoVerdictReason::UnsupportedCapability,
                    "schema-dialect",
                    "the resource does not use the supported 2020-12 dialect",
                ));
            }
            if has_unpaired(&node.value) {
                return Err(self.failure(
                    id,
                    NoVerdictReason::UnsupportedCapability,
                    "lone-surrogate-schema",
                    "schema interpretation requires Unicode scalar strings",
                ));
            }
            let checks = crate::fixed_schema::check(&node.value, true, 1).map_err(|e| {
                self.failure(id, NoVerdictReason::LimitExceeded, "meta-schema-check", e)
            })?;
            if !checks.entries.is_empty() {
                return Err(self.failure(
                    id,
                    NoVerdictReason::ConservativePreparation,
                    "invalid-schema",
                    "the reached schema is not well formed under the 2020-12 meta-schemas",
                ));
            }
            if out.resources.insert(node.resource) {
                for (name, holders) in &out.dynamic {
                    if let Some(declarations) = resource.anchors.get(name) {
                        if declarations.len() > 1 {
                            return Err(self.failure(id,NoVerdictReason::ConservativePreparation,"ambiguous-dynamic-anchor","a reached resource declares a dynamically looked-up name more than once"));
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
                                NoVerdictReason::ConservativePreparation,
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
                                        return Err(self.failure(id,NoVerdictReason::ConservativePreparation,"ambiguous-dynamic-anchor","a dynamically looked-up name has multiple declarations"));
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
        // A cycle in potentially reachable in-place applicators is conservative
        // preparation failure. Static reach does not prove semantic undefinedness.
        let mut colors = HashMap::new();
        for &root in &out.nodes {
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
                            NoVerdictReason::ConservativePreparation,
                            "in-place-cycle",
                            "static preparation found a potentially evaluated in-place cycle",
                        ));
                    }
                    Some(2) => continue,
                    _ => {}
                }
                colors.insert(id, 1);
                stack.push((id, true));
                for &next in out.edges.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
                    stack.push((next, false));
                }
            }
        }
        Ok(out)
    }
}
fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("a Rust string serializes as JSON")
}
fn object(fields: BTreeMap<String, String>) -> String {
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
fn keyword_children<'a>(keyword: &str, value: JsonRef<'a>) -> Vec<JsonRef<'a>> {
    match keyword {
        "properties" | "patternProperties" | "dependentSchemas" => value
            .members()
            .map(|m| m.map(|v| v.value).collect())
            .unwrap_or_default(),
        "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
            value.elements().map(Iterator::collect).unwrap_or_default()
        }
        _ => vec![value],
    }
}
#[derive(Default)]
struct Reach {
    nodes: HashSet<usize>,
    resources: HashSet<usize>,
    edges: HashMap<usize, Vec<usize>>,
    targets: HashMap<(usize, String), Target>,
    dynamic: BTreeMap<String, Vec<usize>>,
}
impl ParsedDocument {
    pub fn references(&self) -> Result<ReferenceReport, InterpretationError> {
        self.references_with_resources(ResourceSet::default(), &WorkControl::new())
    }
    pub fn references_with_resources(
        &self,
        resources: ResourceSet,
        control: &WorkControl,
    ) -> Result<ReferenceReport, InterpretationError> {
        self.interpretable()?;
        Ok(SchemaSpace::new(self.clone(), resources).references(control))
    }
}
fn embedded_resources() -> &'static [SchemaResource] {
    static RESOURCES: std::sync::OnceLock<Vec<SchemaResource>> = std::sync::OnceLock::new();
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
                SchemaResource { uri, document }
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
    fn verdict(program: &EvaluationProgram, value: &JsonValue) -> (bool, Vec<SchemaLocation>) {
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
        let document=ParsedDocument::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/schemas/strict"}}},"schemas":{"strict":{"$dynamicAnchor":"node","$ref":"https://example.invalid/tree","unevaluatedProperties":false}}}"##).unwrap();
        let resources=ResourceSet::new([SchemaResource{uri:"https://example.invalid/tree".into(),document:JsonValue::parse(r##"{"$dynamicAnchor":"node","type":"object","properties":{"children":{"type":"array","items":{"$dynamicRef":"#node"}},"literal":{"const":"https://sdk-program.openbindings.invalid/r0#/$defs/n0"}}}"##).unwrap()}]).unwrap();
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
        let control = WorkControl::new();
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

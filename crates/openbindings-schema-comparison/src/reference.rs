//! Lazy resource resolution. Schema ancestry never interprets carrier members.
use crate::{ComparisonContext, ComparisonFailure as Failure, SchemaInput, budget::Attempt};
use openbindings::{JsonKind, JsonRef, JsonValue};
use openbindings_internal_json::backend;
use openbindings_internal_schema::{
    schema_index::{SCHEMA_SELECTORS, SchemaSelector, each_schema_child},
    uri,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Resource {
    base: Option<String>,
    document: JsonValue,
    origin: usize,
}
#[derive(Default)]
pub(crate) struct Index {
    entries: Vec<Resource>,
    uris: BTreeMap<String, Option<usize>>,
    scopes: BTreeMap<((usize, usize), usize), usize>,
    catalog: bool,
    cataloged: std::collections::BTreeSet<((usize, usize), usize)>,
    local_root: Option<JsonValue>,
    parent: Option<Arc<Mutex<Index>>>,
    imports: BTreeMap<usize, usize>,
    anchors: BTreeMap<(usize, String), Option<JsonValue>>,
    anchored: std::collections::BTreeSet<usize>,
}
#[derive(Clone)]
pub(crate) struct Resources {
    index: Arc<Mutex<Index>>,
}
fn identity(text: &str, attempt: &mut Attempt<'_>) -> Result<String, Failure> {
    attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
    attempt.allocate(text.len().saturating_mul(8).saturating_add(128))?;
    if !uri::absolute(text) || uri::fragment(text).is_some_and(|f| !f.is_empty()) {
        return Err(Failure::InvalidReference);
    }
    let mut resolved = uri::resolve(text, text).ok_or(Failure::InvalidReference)?;
    if resolved.ends_with('#') {
        resolved.pop();
    }
    Ok(resolved)
}
fn same(
    a: &ComparisonContext,
    b: &ComparisonContext,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.work(1)?;
    if Arc::ptr_eq(&a.data, &b.data) {
        return Ok(true);
    }
    if backend::occurrence(a.data.document.view()) != backend::occurrence(b.data.document.view()) {
        return Ok(false);
    }
    attempt.work(
        a.data
            .base
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(b.data.base.as_ref().map_or(0, String::len))
            .saturating_add(1),
    )?;
    if a.data.base != b.data.base || a.data.resources.len() != b.data.resources.len() {
        return Ok(false);
    }
    for ((u, a), (v, b)) in a.data.resources.iter().zip(&b.data.resources) {
        attempt.work(u.len().saturating_add(v.len()).saturating_add(1))?;
        if u != v || backend::occurrence(a.view()) != backend::occurrence(b.view()) {
            return Ok(false);
        }
    }
    Ok(true)
}
impl Resources {
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.index) as usize
    }
    pub(crate) fn new(input: &SchemaInput, attempt: &mut Attempt<'_>) -> Result<Self, Failure> {
        // A call canonicalizes independently constructed inputs for the same root.
        let context = &input.context;
        let mut index = context.index.clone();
        for position in 0..attempt.contexts.len() {
            let known = attempt.contexts[position].clone();
            if same(&known, context, attempt)? {
                index = known.index;
                break;
            }
        }
        if !attempt
            .contexts
            .iter()
            .any(|c| Arc::ptr_eq(&c.index, &index))
        {
            attempt.allocate(2 * std::mem::size_of::<ComparisonContext>())?;
            attempt.contexts.push(context.clone());
        }
        let result = Self { index };
        result
            .index
            .lock()
            .expect("comparison index")
            .setup(context, attempt)?;
        let mut top = input.schema.view();
        while !context_roots(context)
            .any(|root| backend::occurrence(root.view()) == backend::occurrence(top))
            && let Some(parent) = enclosing(top)
        {
            attempt.work(1)?;
            top = parent;
        }
        let configured = context_roots(context)
            .enumerate()
            .any(|(position, root)| configured_top(root.view(), top, position == 0));
        if configured {
            return Ok(result);
        }
        let key = (result.identity(), backend::occurrence(top));
        attempt.work(attempt.local_resources.len().saturating_add(1))?;
        for (index, node, resources) in &attempt.local_resources {
            if (*index, *node) == key {
                return Ok(resources.clone());
            }
        }
        // A selection outside configured schema positions layers its declarations
        // over the configured index without copying or retaining cache history.
        let mut local = Index::default();
        local.setup(context, attempt)?;
        local.local_root = Some(top.to_owned());
        local.parent = Some(result.index.clone());
        for scope in 1..local.entries.len() {
            attempt.allocate(256)?;
            local.imports.insert(scope, scope);
        }
        let result = Self {
            index: Arc::new(Mutex::new(local)),
        };
        attempt.allocate(2 * std::mem::size_of::<(usize, (usize, usize), Self)>())?;
        attempt.local_resources.push((key.0, key.1, result.clone()));
        Ok(result)
    }

    pub(crate) fn enter(
        &mut self,
        schema: JsonRef<'_>,
        fallback: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        self.index
            .lock()
            .expect("comparison index")
            .enter(schema, fallback, 0, attempt)
    }
    pub(crate) fn resolve(
        &self,
        scope: usize,
        reference: JsonRef<'_>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        self.index
            .lock()
            .expect("comparison index")
            .resolve(scope, reference, attempt)
    }
    pub(crate) fn context<'a>(
        &self,
        scope: usize,
        schema: JsonRef<'a>,
        spelling: &'a str,
        attempt: &mut Attempt<'_>,
    ) -> Result<crate::ReferenceContext<'a>, Failure> {
        let mut index = self.index.lock().expect("comparison index");
        let token = schema
            .get("$ref")
            .filter(|v| v.as_str() == Some(spelling))
            .or_else(|| schema.get("$dynamicRef"))
            .expect("reference context");
        let target = index.resolve(scope, token, attempt).map(|(v, _)| v);
        if let Err(e @ (Failure::LimitExceeded(_) | Failure::Cancelled)) = target {
            return Err(e);
        }
        let entry = &index.entries[scope];
        attempt.allocate(entry.base.as_ref().map_or(0, String::len))?;
        Ok(crate::ReferenceContext {
            spelling,
            schema,
            resource: entry.document.clone(),
            base: entry.base.clone(),
            target,
        })
    }
}
fn context_roots(context: &ComparisonContext) -> impl Iterator<Item = &JsonValue> {
    std::iter::once(&context.data.document)
        .chain(context.data.resources.iter().map(|(_, value)| value))
}
// Only a recognized schema edge can make an enclosing value a schema. In
// particular /schemas/X and /operations/X/input stop before their carrier root.
fn enclosing(schema: JsonRef<'_>) -> Option<JsonRef<'_>> {
    let parent = backend::parent(schema)?;
    if let Some(name) = backend::member_name(schema).and_then(|n| n.as_str())
        && SCHEMA_SELECTORS
            .iter()
            .any(|(k, s)| *k == name && *s == SchemaSelector::Direct)
    {
        return Some(parent);
    }
    let keyword = backend::member_name(parent)?.as_str()?;
    let grandparent = backend::parent(parent)?;
    SCHEMA_SELECTORS
        .iter()
        .any(|(k, s)| {
            *k == keyword
                && match s {
                    SchemaSelector::Map | SchemaSelector::MapOrStringArray => {
                        parent.kind() == JsonKind::Object
                    }
                    SchemaSelector::Array => parent.kind() == JsonKind::Array,
                    SchemaSelector::Direct => false,
                }
        })
        .then_some(grandparent)
}
fn configured_top(root: JsonRef<'_>, top: JsonRef<'_>, obi: bool) -> bool {
    let same = |a, b| backend::occurrence(a) == backend::occurrence(b);
    if !obi || root.get("openbindings").is_none() || root.get("operations").is_none() {
        return same(root, top);
    }
    let Some(parent) = backend::parent(top) else {
        return false;
    };
    if root
        .get("schemas")
        .is_some_and(|schemas| same(schemas, parent))
    {
        return true;
    }
    matches!(
        backend::member_name(top).and_then(|v| v.as_str()),
        Some("input" | "output")
    ) && backend::parent(parent)
        .is_some_and(|operations| root.get("operations").is_some_and(|v| same(v, operations)))
}
impl Index {
    fn setup(
        &mut self,
        context: &ComparisonContext,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        if !self.entries.is_empty() {
            return attempt.check();
        }
        let mut entries = attempt.vector(context.data.resources.len().saturating_add(1))?;
        entries.push(Resource {
            base: context
                .data
                .base
                .as_deref()
                .map(|s| identity(s, attempt))
                .transpose()?,
            document: context.data.document.clone(),
            origin: 0,
        });
        let mut uris = BTreeMap::new();
        if let Some(base) = &entries[0].base {
            attempt.allocate(base.len().saturating_add(2048))?;
            uris.insert(base.clone(), Some(0));
        }
        for (base, document) in &context.data.resources {
            let base = identity(base, attempt)?;
            attempt.work(
                base.len()
                    .saturating_add(1)
                    .saturating_mul(uris.len().saturating_add(1).ilog2() as usize + 1)
                    .saturating_mul(12),
            )?;
            if uris.contains_key(&base) {
                return Err(Failure::InvalidReference);
            }
            attempt.allocate(base.len().saturating_add(2048))?;
            uris.insert(base.clone(), Some(entries.len()));
            entries.push(Resource {
                base: Some(base),
                document: document.clone(),
                origin: entries.len(),
            });
        }
        attempt.check()?;
        self.entries = entries;
        self.uris = uris;
        Ok(())
    }
    fn enter(
        &mut self,
        schema: JsonRef<'_>,
        fallback: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        attempt.depth(depth)?;
        if fallback != 0
            && let Some(parent) = self.parent.clone()
        {
            attempt.work(self.imports.len().saturating_add(1))?;
            if let Some(parent_scope) = self
                .imports
                .iter()
                .find_map(|(p, local)| (*local == fallback).then_some(*p))
            {
                let mut parent = parent.lock().expect("comparison index");
                let scope = parent.enter(schema, parent_scope, depth, attempt)?;
                return self.import(&parent, scope, attempt);
            }
        }
        let node = backend::occurrence(schema);
        let origin = self.entries[fallback].origin;
        let key = (node, origin);
        attempt.work(self.scopes.len().saturating_add(1).ilog2() as usize + 1)?;
        if let Some(scope) = self.scopes.get(&key) {
            return Ok(*scope);
        }
        let mut scope = fallback;
        if backend::occurrence(self.entries[fallback].document.view()) != node
            && let Some(parent) = enclosing(schema)
        {
            scope = self.enter(parent, fallback, depth + 1, attempt)?;
        }
        let mut top = schema;
        let root = self.entries[origin].document.view();
        while backend::occurrence(top) != backend::occurrence(root)
            && let Some(parent) = enclosing(top)
        {
            attempt.work(1)?;
            top = parent;
        }
        if configured_top(root, top, origin == 0)
            || self
                .local_root
                .as_ref()
                .is_some_and(|local| backend::occurrence(local.view()) == backend::occurrence(top))
        {
            scope = self.declare(schema, scope, attempt)?;
        }
        attempt.allocate(4 * std::mem::size_of_val(&(key, scope)))?;
        self.scopes.insert(key, scope);
        Ok(scope)
    }
    fn declare(
        &mut self,
        schema: JsonRef<'_>,
        mut scope: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        let node = backend::occurrence(schema);
        let origin = self.entries[scope].origin;
        if let Some(id) = schema.get("$id") {
            let text = id.as_str().ok_or(Failure::InvalidReference)?;
            attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
            attempt.allocate(
                text.len()
                    .saturating_add(self.entries[scope].base.as_ref().map_or(0, String::len))
                    .saturating_mul(8)
                    .saturating_add(128),
            )?;
            let resolved = if uri::absolute(text) {
                text.to_owned()
            } else {
                uri::resolve(
                    self.entries[scope]
                        .base
                        .as_deref()
                        .ok_or(Failure::InvalidReference)?,
                    text,
                )
                .ok_or(Failure::InvalidReference)?
            };
            let base = identity(&resolved, attempt)?;
            self.uri_work(&base, attempt)?;
            if let Some(Some(index)) = self.uris.get(&base)
                && backend::occurrence(self.entries[*index].document.view()) == node
            {
                return Ok(*index);
            }
            // An already ambiguous URI can include this occurrence. Re-entering
            // it must not publish another declaration after a failed lookup.
            if self.uris.get(&base) == Some(&None) {
                for (index, resource) in self.entries.iter().enumerate() {
                    attempt.work(base.len().saturating_add(1))?;
                    if resource.base.as_deref() == Some(&base)
                        && backend::occurrence(resource.document.view()) == node
                    {
                        return Ok(index);
                    }
                }
            }
            attempt.allocate(
                base.len()
                    .saturating_add(2048)
                    .saturating_add(2 * std::mem::size_of::<Resource>()),
            )?;
            scope = self.entries.len();
            self.uris
                .entry(base.clone())
                .and_modify(|value| *value = None)
                .or_insert(Some(scope));
            self.entries.push(Resource {
                base: Some(base),
                document: schema.to_owned(),
                origin,
            });
        }
        Ok(scope)
    }
    fn roots(&self, scope: usize, attempt: &mut Attempt<'_>) -> Result<Vec<JsonValue>, Failure> {
        let root = &self.entries[scope].document;
        let mut roots = attempt.vector(1)?;
        if scope == 0 && self.parent.is_some() {
            roots.push(self.local_root.as_ref().expect("detached root").clone());
        } else if scope == 0
            && root.get("openbindings").is_some()
            && root.get("operations").is_some()
        {
            if let Some(schemas) = root.get("schemas").and_then(|v| v.members()) {
                for schema in schemas {
                    attempt.allocate(2 * std::mem::size_of::<JsonValue>())?;
                    roots.push(schema.value.to_owned());
                }
            }
            if let Some(operations) = root.get("operations").and_then(|v| v.members()) {
                for operation in operations {
                    for slot in ["input", "output"] {
                        if let Some(schema) = operation.value.get(slot) {
                            attempt.allocate(2 * std::mem::size_of::<JsonValue>())?;
                            roots.push(schema.to_owned());
                        }
                    }
                }
            }
        } else {
            roots.push(root.clone());
        }
        Ok(roots)
    }
    fn children(
        schema: JsonRef<'_>,
        pending: &mut Vec<JsonValue>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(), Failure> {
        attempt.work(
            schema
                .members()
                .map_or(0, |m| m.len())
                .saturating_mul(32)
                .saturating_add(1),
        )?;
        let mut failure = None;
        each_schema_child(schema, |child| {
            if failure.is_none() {
                match attempt.allocate(2 * std::mem::size_of::<JsonValue>()) {
                    Ok(()) => pending.push(child.to_owned()),
                    Err(e) => failure = Some(e),
                }
            }
        });
        failure.map_or(Ok(()), Err)
    }
    fn catalog(&mut self, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        if self.catalog {
            return Ok(());
        }
        // Discovery is transactional: a cancelled scan cannot make a partial
        // URI catalog look like completed successful indexing on the next call.
        for name in self.uris.keys() {
            attempt.work(name.len().saturating_add(1))?;
            attempt.allocate(name.len().saturating_add(2048))?;
        }
        let previous = self.uris.clone();
        let entries = self.entries.len();
        let result = self.catalog_inner(attempt);
        if result.is_err() {
            self.uris = previous;
            self.entries.truncate(entries);
        }
        result
    }
    fn catalog_inner(&mut self, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        let initial = if self.parent.is_some() {
            1
        } else {
            self.entries.len()
        };
        let mut visited = std::collections::BTreeSet::new();
        for scope in 0..initial {
            let roots = self.roots(scope, attempt)?;
            let mut pending = attempt.vector(roots.len())?;
            pending.extend(roots.into_iter().map(|v| (v, scope)));
            while let Some((schema, inherited)) = pending.pop() {
                let node = (
                    backend::occurrence(schema.view()),
                    self.entries[inherited].origin,
                );
                attempt.work(
                    self.cataloged
                        .len()
                        .saturating_add(visited.len())
                        .saturating_add(1)
                        .ilog2() as usize
                        + 1,
                )?;
                if self.cataloged.contains(&node) || visited.contains(&node) {
                    continue;
                }
                let scope = match self.declare(schema.view(), inherited, attempt) {
                    Ok(scope) => scope,
                    Err(e @ (Failure::LimitExceeded(_) | Failure::Cancelled)) => return Err(e),
                    Err(_) => inherited,
                };
                let mut children = attempt.vector(0)?;
                Self::children(schema.view(), &mut children, attempt)?;
                for child in children {
                    attempt.allocate(2 * std::mem::size_of::<(JsonValue, usize)>())?;
                    pending.push((child, scope));
                }
                attempt.allocate(128)?;
                visited.insert(node);
            }
        }
        attempt.check()?;
        self.cataloged.extend(visited);
        self.catalog = true;
        Ok(())
    }
    fn anchors(&mut self, scope: usize, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        if self.anchored.contains(&scope) {
            return Ok(());
        }
        let mut anchors = BTreeMap::new();
        let mut pending = self.roots(scope, attempt)?;
        let root = backend::occurrence(self.entries[scope].document.view());
        while let Some(schema) = pending.pop() {
            if schema.get("$id").is_some() && backend::occurrence(schema.view()) != root {
                continue;
            }
            for key in ["$anchor", "$dynamicAnchor"] {
                if let Some(name) = schema.get(key).and_then(|v| v.as_str()) {
                    attempt.work(
                        name.len()
                            .saturating_add(1)
                            .saturating_mul(anchors.len().saturating_add(1).ilog2() as usize + 1),
                    )?;
                    attempt.allocate(name.len().saturating_add(256))?;
                    // Every declaration counts, including both anchor keywords
                    // on one schema. Ambiguity fails only if selected.
                    anchors
                        .entry((scope, name.to_owned()))
                        .and_modify(|v| *v = None)
                        .or_insert_with(|| Some(schema.clone()));
                }
            }
            Self::children(schema.view(), &mut pending, attempt)?;
        }
        attempt.allocate(256)?;
        attempt.check()?;
        self.anchors.extend(anchors);
        self.anchored.insert(scope);
        Ok(())
    }
    fn import(
        &mut self,
        parent: &Index,
        scope: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<usize, Failure> {
        if let Some(local) = self.imports.get(&scope) {
            return Ok(*local);
        }
        let resource = &parent.entries[scope];
        attempt.allocate(
            resource
                .base
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(512),
        )?;
        let local = self.entries.len();
        self.entries.push(resource.clone());
        self.imports.insert(scope, local);
        Ok(local)
    }
    fn parent_target(
        &mut self,
        parent: &mut Index,
        scope: usize,
        fragment: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        let (target, scope) = parent.target(scope, fragment, attempt)?;
        Ok((target, self.import(parent, scope, attempt)?))
    }
    fn target(
        &mut self,
        scope: usize,
        fragment: &str,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        attempt.allocate(fragment.len())?;
        let decoded = uri::decode_fragment(fragment).ok_or(Failure::InvalidReference)?;
        if scope == 0
            && let Some(parent) = self.parent.clone()
        {
            let mut parent = parent.lock().expect("comparison index");
            if decoded.is_empty() || decoded.starts_with('/') {
                return self.parent_target(&mut parent, 0, fragment, attempt);
            }
            self.anchors(0, attempt)?;
            parent.anchors(0, attempt)?;
            let key = (0, decoded.clone());
            let own = self.anchors.get(&key);
            let inherited = parent.anchors.get(&key);
            if own.is_some() && inherited.is_some() {
                return Err(Failure::InvalidReference);
            }
            if inherited.is_some() {
                return self.parent_target(&mut parent, 0, fragment, attempt);
            }
        }
        let target = if decoded.is_empty() || decoded.starts_with('/') {
            fragment_target(&self.entries[scope].document, fragment, attempt)?
        } else {
            self.anchors(scope, attempt)?;
            attempt.work(
                decoded
                    .len()
                    .saturating_add(1)
                    .saturating_mul(self.anchors.len().saturating_add(1).ilog2() as usize + 1),
            )?;
            self.anchors
                .get(&(scope, decoded))
                .ok_or(Failure::MissingReference)?
                .clone()
                .ok_or(Failure::InvalidReference)?
        };
        let scope = self.enter(target.view(), scope, 0, attempt)?;
        Ok((target, scope))
    }
    fn uri_work(&self, name: &str, attempt: &mut Attempt<'_>) -> Result<(), Failure> {
        attempt.work(
            name.len()
                .saturating_add(1)
                .saturating_mul(self.uris.len().saturating_add(1).ilog2() as usize + 1)
                .saturating_mul(12),
        )
    }
    fn lookup(&self, name: &str, attempt: &mut Attempt<'_>) -> Result<Option<usize>, Failure> {
        self.uri_work(name, attempt)?;
        match self.uris.get(name) {
            Some(Some(scope)) => Ok(Some(*scope)),
            Some(None) => Err(Failure::InvalidReference),
            None => Ok(None),
        }
    }
    fn resolve(
        &mut self,
        scope: usize,
        reference: JsonRef<'_>,
        attempt: &mut Attempt<'_>,
    ) -> Result<(JsonValue, usize), Failure> {
        if scope != 0
            && let Some(parent) = self.parent.clone()
            && let Some(parent_scope) = self
                .imports
                .iter()
                .find_map(|(p, local)| (*local == scope).then_some(*p))
        {
            let mut parent = parent.lock().expect("comparison index");
            let (target, target_scope) = parent.resolve(parent_scope, reference, attempt)?;
            return Ok((target, self.import(&parent, target_scope, attempt)?));
        }
        let text = reference.as_str().ok_or(Failure::InvalidReference)?;
        attempt.work(text.len().saturating_mul(8).saturating_add(1))?;
        if !uri::valid(text) {
            return Err(Failure::InvalidReference);
        }
        if text.is_empty() {
            return self.target(scope, "", attempt);
        }
        if let Some(fragment) = text.strip_prefix('#') {
            return self.target(scope, fragment, attempt);
        }
        let current = &self.entries[scope];
        let bytes = text
            .len()
            .saturating_add(current.base.as_ref().map_or(0, String::len));
        attempt.allocate(bytes.saturating_mul(8).saturating_add(128))?;
        attempt.work(bytes.saturating_mul(8).saturating_add(1))?;
        let resolved = if uri::absolute(text) {
            uri::resolve(text, text)
        } else {
            uri::resolve(
                current.base.as_deref().ok_or(Failure::InvalidReference)?,
                text,
            )
        }
        .ok_or(Failure::InvalidReference)?;
        let (resource, fragment) = resolved.split_once('#').unwrap_or((&resolved, ""));
        self.catalog(attempt)?;
        let own = self.lookup(resource, attempt)?;
        if let Some(parent) = self.parent.clone() {
            let mut parent = parent.lock().expect("comparison index");
            parent.catalog(attempt)?;
            if let Some(inherited) = parent.lookup(resource, attempt)? {
                if let Some(own) = own
                    && backend::occurrence(self.entries[own].document.view())
                        != backend::occurrence(parent.entries[inherited].document.view())
                {
                    return Err(Failure::InvalidReference);
                }
                return self.parent_target(&mut parent, inherited, fragment, attempt);
            }
        }
        self.target(own.ok_or(Failure::MissingReference)?, fragment, attempt)
    }
}
fn fragment_target(
    root: &JsonValue,
    fragment: &str,
    attempt: &mut Attempt<'_>,
) -> Result<JsonValue, Failure> {
    attempt.work(fragment.len().saturating_mul(4).saturating_add(1))?;
    attempt.allocate(fragment.len())?;
    let pointer = uri::decode_fragment(fragment).ok_or(Failure::InvalidReference)?;
    if pointer.is_empty() {
        return Ok(root.clone());
    }
    let path = pointer
        .strip_prefix('/')
        .ok_or(Failure::UnsupportedReference)?;
    let mut node = root.view();
    for segment in path.split('/') {
        attempt.work(segment.len().saturating_mul(4).saturating_add(1))?;
        let mut decoded = attempt.vector::<u16>(segment.len())?;
        let mut units = segment.encode_utf16();
        while let Some(unit) = units.next() {
            decoded.push(if unit == b'~' as u16 {
                match units.next() {
                    Some(unit) if unit == b'0' as u16 => b'~' as u16,
                    Some(unit) if unit == b'1' as u16 => b'/' as u16,
                    _ => return Err(Failure::InvalidReference),
                }
            } else {
                unit
            });
        }
        node = match node.kind() {
            JsonKind::Object => {
                let mut found = None;
                attempt.allocate(decoded.len().saturating_mul(8))?;
                for member in
                    backend::member_candidates_utf16(node, &decoded, |cost| attempt.work(cost))?
                {
                    attempt.work(
                        member
                            .name
                            .text()
                            .len()
                            .saturating_add(decoded.len())
                            .saturating_add(1),
                    )?;
                    if member
                        .name
                        .as_string()
                        .expect("name")
                        .utf16_units()
                        .eq(decoded.iter().copied())
                    {
                        if found.is_some() {
                            return Err(Failure::DuplicateMember);
                        }
                        found = Some(member.value);
                    }
                }
                found.ok_or(Failure::MissingReference)?
            }
            JsonKind::Array => {
                // Index syntax is ASCII and must use the canonical array spelling.
                if decoded.is_empty() || (decoded.len() > 1 && decoded[0] == b'0' as u16) {
                    return Err(Failure::InvalidReference);
                }
                let mut index = 0usize;
                for unit in decoded {
                    if !(b'0' as u16..=b'9' as u16).contains(&unit) {
                        return Err(Failure::InvalidReference);
                    }
                    index = index
                        .checked_mul(10)
                        .and_then(|n| n.checked_add((unit - b'0' as u16) as usize))
                        .ok_or(Failure::MissingReference)?;
                }
                let mut elements = node.elements().expect("array");
                if index >= elements.len() {
                    return Err(Failure::MissingReference);
                }
                attempt.work(index.saturating_add(1))?;
                elements.nth(index).expect("existing element")
            }
            _ => return Err(Failure::MissingReference),
        };
    }
    Ok(node.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComparisonLimits;
    use std::sync::atomic::AtomicBool;
    fn json(text: &str) -> JsonValue {
        JsonValue::parse(text).unwrap()
    }
    #[test]
    fn references_use_each_supplied_root_base_and_external_context() {
        let selected = json(r##"{"$ref":"#/schemas/A"}"##);
        let input = SchemaInput::new(
            selected.clone(),
            json(r##"{"schemas":{"A":{"type":"string"}}}"##),
        )
        .with_base("https://example.test/root")
        .with_resource(
            "https://example.test/other/a",
            json(r##"{"$id":"https://example.test/canonical/a","$ref":"b"}"##),
        )
        .with_resource("https://example.test/canonical/b", json("false"));
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let resources = Resources::new(&input, &mut attempt).unwrap();
        let reference = json(r##""#/schemas/A""##);
        assert_eq!(
            resources
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            r##"{"type":"string"}"##
        );
        let external = json(r##""other/a""##);
        let (a, scope) = resources.resolve(0, external.view(), &mut attempt).unwrap();
        let (b, _) = resources
            .resolve(scope, a.get("$ref").unwrap(), &mut attempt)
            .unwrap();
        assert_eq!(b.text(), "false");
        let other = SchemaInput::new(selected, json(r##"{"schemas":{"A":true}}"##));
        assert_eq!(
            Resources::new(&other, &mut attempt)
                .unwrap()
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            "true"
        );
    }
    #[test]
    fn pointer_ambiguity_missing_context_and_resource_aliases_are_failures() {
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let input = {
            let schema = json(r##"{"a/b":[true,false],"duplicate":true,"duplicate":false}"##);
            SchemaInput::new(schema.clone(), schema)
        };
        let resources = Resources::new(&input, &mut attempt).unwrap();
        let reference = json(r##""#/a~1b/1""##);
        assert_eq!(
            resources
                .resolve(0, reference.view(), &mut attempt)
                .unwrap()
                .0
                .text(),
            "false"
        );
        for (text, error) in [
            (r##""#/duplicate""##, Failure::DuplicateMember),
            (r##""#/a~2b""##, Failure::InvalidReference),
            (r##""relative""##, Failure::InvalidReference),
            (r##""https://missing.test/""##, Failure::MissingReference),
        ] {
            assert!(
                matches!(resources.resolve(0,json(text).view(),&mut attempt),Err(found) if found==error)
            );
        }
        let duplicate = {
            let schema = json("true");
            SchemaInput::new(schema.clone(), schema)
        }
        .with_resource("https://x/a/../b", json("true"))
        .with_resource("https://x/b#", json("false"));
        assert!(matches!(
            Resources::new(&duplicate, &mut attempt),
            Err(Failure::InvalidReference)
        ));
    }
}

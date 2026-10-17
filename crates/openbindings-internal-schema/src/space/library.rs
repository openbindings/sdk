//! Static inspection through documented library APIs. Resource selection is
//! embedding/input policy; all ordinary pointer and anchor lookups are delegated.
use super::*;
use jsonschema::{Draft, Registry, Retrieve, Uri};
use serde_json::Value;
use std::{
    borrow::Cow,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone)]
pub(super) struct Refusal {
    reason: FailureReason,
    code: &'static str,
    message: &'static str,
    missing: Option<Arc<str>>,
    defect: Option<StaticReferenceDefect>,
}
impl Refusal {
    fn invalid(code: &'static str, message: &'static str) -> Self {
        Self {
            reason: FailureReason::ConservativePreparation,
            code,
            message,
            missing: None,
            defect: None,
        }
    }
    fn semantic(mut self, defect: StaticReferenceDefect) -> Self {
        self.defect = Some(defect);
        self
    }
}
// A reference's original spelling is already owned by its immutable source.
// Route by schema node and keyword slot, without owning another copy of it.
const REFERENCE_KEYWORDS: [&str; 2] = ["$ref", "$dynamicRef"];
pub(super) type Key = (usize, usize);
type Targets = HashMap<Key, Result<Target, Refusal>>;
struct Queries {
    references: Vec<Key>,
    first_name: String,
    other_names: HashSet<String>,
}
impl Queries {
    fn new(name: &str) -> Self {
        Self {
            references: Vec::new(),
            first_name: name.to_owned(),
            other_names: HashSet::new(),
        }
    }
    fn push(&mut self, key: Key, name: &str) {
        self.references.push(key);
        // Each distinct registration alias is needed only once per carrier.
        // Keep the first alias for the parent's anonymous-base fallback.
        if name != self.first_name && !self.other_names.contains(name) {
            self.other_names.insert(name.to_owned());
        }
    }
}
enum Route {
    Carrier(usize),
    Absent,
    Refused(Refusal),
}
struct Carrier {
    queries: Mutex<Option<Queries>>,
    targets: OnceLock<Targets>,
    #[cfg(test)]
    preparations: std::sync::atomic::AtomicUsize,
}
// Build the association index once for the immutable space. A requested
// reference initializes only its target carrier (or the missing-resource batch).
// Every referrer then shares that carrier's single copy and library preparation.
// Move each pending batch into its initializer and release it afterward. Only
// compact routes and owned lookup results remain; reference text stays in the
// original source. No borrowed registry outlives its inspection tree and no
// library implementation details are needed to retain identities.
pub(super) struct Inspection {
    routes: HashMap<Key, Route>,
    carriers: BTreeMap<usize, Carrier>,
    absent: Mutex<Option<BTreeMap<String, Vec<Key>>>>,
    missing: OnceLock<Targets>,
}

pub(super) type PackagedLookup = (Key, usize, Lookup<String>);
// Where inspection finds one reference's carrier, before identities are selected.
pub(super) enum Lookup<S> {
    // The absolute name of the resource the reference names.
    Name(S),
    // A malformed reference in a resource with a malformed scope: its carrier
    // is the holder's own resource, under its base.
    Scope(S),
    Invalid,
}
impl Lookup<&str> {
    fn owned(self) -> Lookup<String> {
        match self {
            Self::Name(name) => Lookup::Name(name.to_owned()),
            Self::Scope(base) => Lookup::Scope(base.to_owned()),
            Self::Invalid => Lookup::Invalid,
        }
    }
}
impl Lookup<String> {
    fn borrowed(&self) -> Lookup<&str> {
        match self {
            Self::Name(name) => Lookup::Name(name),
            Self::Scope(base) => Lookup::Scope(base),
            Self::Invalid => Lookup::Invalid,
        }
    }
}
fn inspection_registry() -> Registry<'static> {
    Registry::new()
        .retriever(Recording::default())
        .prepare()
        .expect("empty registry")
}
// Routes and carrier groups as references are added in node order.
#[derive(Default)]
struct Index {
    routes: HashMap<Key, Route>,
    groups: BTreeMap<usize, Queries>,
    absent: BTreeMap<String, Vec<Key>>,
}
impl Index {
    fn add(&mut self, space: &SchemaSpace, key: Key, resource: usize, found: Lookup<&str>) {
        let name = match found {
            Lookup::Name(name) => name,
            Lookup::Scope(base) => {
                self.groups
                    .entry(resource)
                    .or_insert_with(|| Queries::new(base))
                    .push(key, base);
                return;
            }
            Lookup::Invalid => {
                self.routes.insert(
                    key,
                    Route::Refused(Refusal::invalid(
                        "invalid-reference",
                        "reference is not a well-formed URI reference",
                    )),
                );
                return;
            }
        };
        // This association table is the contained/supplied identity policy,
        // not a pointer or plain-name resolver. A losing carrier cannot
        // become authoritative because a registry happened to index it last.
        match space.name(name) {
            Some(NameSelection {
                carrier: Some(id), ..
            }) if space.eligible(id) => {
                self.groups
                    .entry(id)
                    .or_insert_with(|| Queries::new(name))
                    .push(key, name);
            }
            Some(NameSelection {
                carrier: Some(_), ..
            }) => {
                self.routes.insert(key,Route::Refused(Refusal::invalid("resource-identity-conflict","the reached resource is not the selected carrier of its canonical identifier")));
            }
            Some(_) => {
                self.routes.insert(
                    key,
                    Route::Refused(Refusal::invalid(
                        "ambiguous-resource",
                        "more than one resource carries the referenced identifier",
                    )),
                );
            }
            None => {
                // BTreeMap entry would allocate another copy even for
                // an existing name; borrow first and own only new names.
                if let Some(keys) = self.absent.get_mut(name) {
                    keys.push(key);
                } else {
                    self.absent.insert(name.to_owned(), vec![key]);
                }
            }
        }
    }
}

#[derive(Clone, Default)]
struct Recording {
    requested: Arc<Mutex<HashSet<String>>>,
}
impl Retrieve for Recording {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.requested.lock().unwrap().insert(uri.to_string());
        // This registry is only an inspection tool. Its placeholders never enter
        // a strict program, paired program, or compiled validator.
        Ok(Value::Object(Default::default()))
    }
}

// Arbitrarily deep literal data must not recurse when the inspection copy drops.
struct Tree(Box<Value>);
impl Drop for Tree {
    fn drop(&mut self) {
        let mut pending = vec![std::mem::take(&mut *self.0)];
        while let Some(value) = pending.pop() {
            match value {
                Value::Array(values) => pending.extend(values),
                Value::Object(values) => pending.extend(values.into_values()),
                _ => {}
            }
        }
    }
}
fn scalar(value: JsonRef<'_>) -> String {
    // Use one injective presentation for every name, including scalar names
    // containing the literal marker. Otherwise a surrogate and its spelling can
    // collapse into one map entry and corrupt both lookup and provenance.
    openbindings_internal_json::backend::fixed_string(value)
        .expect("JSON string")
        .into_owned()
}

fn presented_reference(reference: &str) -> Cow<'_, str> {
    let Some((base, fragment)) = reference.split_once('#') else {
        return Cow::Borrowed(reference);
    };
    if !fragment.starts_with('/') {
        // Classification belongs to the library: an encoded leading slash is
        // still a plain name, not a pointer to translate.
        return Cow::Borrowed(reference);
    }
    // In URI pointer tokens, the scalar presentation's U+E000 marker is UTF-8
    // percent-encoded. Double only that spelling (case-insensitively); preserve
    // all other percent and ~ escapes verbatim for the library to interpret.
    // In particular, %25EE%2580%2580 remains literal percent text.
    const MARKER: &[u8] = b"%EE%80%80";
    let mut cursor = base.len() + 1;
    let mut copied = 0;
    let mut out = String::new();
    while let Some(offset) = reference.as_bytes()[cursor..]
        .windows(MARKER.len())
        .position(|window| window.eq_ignore_ascii_case(MARKER))
    {
        let marker = cursor + offset;
        let end = marker + MARKER.len();
        out.push_str(&reference[copied..end]);
        out.push_str(&reference[marker..end]);
        copied = end;
        cursor = end;
    }
    if copied == 0 {
        Cow::Borrowed(reference)
    } else {
        out.push_str(&reference[copied..]);
        Cow::Owned(out)
    }
}
fn copy(
    value: &JsonValue,
    positions: &HashMap<usize, usize>,
    repair_reference: impl Fn(usize) -> bool,
) -> (Tree, HashMap<usize, usize>) {
    let mut stack = vec![(value.view(), false)];
    let mut values = HashMap::new();
    while let Some((node, visited)) = stack.pop() {
        if !visited {
            stack.push((node, true));
            if let Some(items) = node.elements() {
                stack.extend(items.map(|v| (v, false)));
            }
            if let Some(members) = node.members() {
                stack.extend(members.map(|m| (m.value, false)));
            }
            continue;
        }
        let mut child = |v: JsonRef<'_>| {
            values
                .remove(&node_id(&v.to_owned()))
                .expect("postorder child")
        };
        let mut copied = match node.kind() {
            JsonKind::Null => Value::Null,
            JsonKind::Boolean => Value::Bool(node.as_bool().unwrap()),
            JsonKind::Number => Value::from(0), // Inspection never evaluates numeric operands.
            JsonKind::String => Value::String(scalar(node)),
            JsonKind::Array => Value::Array(node.elements().unwrap().map(&mut child).collect()),
            JsonKind::Object => Value::Object(
                node.members()
                    .unwrap()
                    .map(|m| {
                        let mut value = child(m.value);
                        if matches!(m.name.as_str(), Some("$ref" | "$dynamicRef"))
                            && let Value::String(reference) = &mut value
                            && let Cow::Owned(presented) = presented_reference(reference)
                        {
                            *reference = presented;
                        }
                        (scalar(m.name), value)
                    })
                    .collect(),
            ),
        };
        // Only inspection is repaired. The authored invocation and its syntax
        // evidence remain in SchemaSpace. No identifier, anchor, or target is
        // reconstructed here; ordinary lookup still belongs to the registry.
        if positions
            .get(&node_id(&node.to_owned()))
            .is_some_and(|&id| repair_reference(id))
            && let Value::Object(object) = &mut copied
            && object
                .get("$ref")
                .and_then(Value::as_str)
                .is_some_and(|r| !uri::valid(r))
        {
            object.remove("$ref");
        }
        values.insert(node_id(&node.to_owned()), copied);
    }
    let tree = Tree(Box::new(
        values.remove(&node_id(value)).expect("copied root"),
    ));
    let mut identity = HashMap::new();
    let mut stack = vec![(value.view(), &*tree.0)];
    while let Some((original, copied)) = stack.pop() {
        if let Some(id) = positions.get(&node_id(&original.to_owned())) {
            identity.insert(std::ptr::from_ref(copied) as usize, *id);
        }
        if let (Some(items), Some(copies)) = (original.elements(), copied.as_array()) {
            stack.extend(items.zip(copies));
        }
        if let (Some(members), Some(copies)) = (original.members(), copied.as_object()) {
            for member in members {
                if let Some(value) = copies.get(&scalar(member.name)) {
                    stack.push((member.value, value));
                }
            }
        }
    }
    (tree, identity)
}

impl SchemaSpace {
    pub(super) fn library_target(
        &self,
        holder: usize,
        reference: &str,
        missing: &mut impl FnMut(&str),
    ) -> Result<Target, Failure> {
        let index = self.inspection.get_or_init(|| self.index_inspection());
        // Both keywords may exist at one node. Matching the original spelling
        // also preserves the refusal for queries absent from the original input.
        let key = REFERENCE_KEYWORDS
            .iter()
            .position(|keyword| {
                self.node(holder)
                    .value
                    .get(keyword)
                    .and_then(|v| v.as_str())
                    == Some(reference)
            })
            .map(|slot| (holder, slot));
        let result = match key.and_then(|key| index.routes.get(&key).map(|route| (key, route))) {
            Some((key, Route::Carrier(id))) => {
                let carrier = &index.carriers[id];
                carrier
                    .targets
                    .get_or_init(|| {
                        #[cfg(test)]
                        carrier
                            .preparations
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        // OnceLock serializes initialization; take releases the query
                        // storage instead of retaining it beside the completed cache.
                        let queries = carrier
                            .queries
                            .lock()
                            .unwrap()
                            .take()
                            .expect("pending carrier");
                        self.inspect_carrier(*id, queries)
                    })
                    .get(&key)
                    .cloned()
            }
            Some((key, Route::Absent)) => index
                .missing
                .get_or_init(|| {
                    let absent = index
                        .absent
                        .lock()
                        .unwrap()
                        .take()
                        .expect("pending missing resources");
                    self.inspect_missing(absent)
                })
                .get(&key)
                .cloned(),
            Some((_, Route::Refused(refusal))) => Some(Err(refusal.clone())),
            None => None,
        };
        match result {
            Some(Ok(target)) => {
                self.check_resource(self.node(target.node).resource, holder)?;
                Ok(target)
            }
            Some(Err(refusal)) => {
                if let Some(uri) = &refusal.missing {
                    missing(uri);
                }
                let mut failure =
                    self.failure(holder, refusal.reason, refusal.code, refusal.message);
                failure.reference_defect = refusal.defect;
                Err(failure)
            }
            None => Err(self.failure(
                holder,
                FailureReason::EvaluatorFailure,
                "uninspected-reference",
                "reference was not present in the original context",
            )),
        }
    }
    fn inspection_reference(&self, key: Key) -> &str {
        self.node(key.0)
            .value
            .get(REFERENCE_KEYWORDS[key.1])
            .and_then(|v| v.as_str())
            .expect("indexed scalar reference")
    }
    // How inspection reaches one reference's carrier, before identities are
    // selected. `None` for a relative reference in the anonymous document,
    // which same-document lookup serves.
    fn look_up(
        &self,
        registry: &Registry<'_>,
        holder: usize,
        reference: &str,
        base: &str,
        found: impl FnOnce(Lookup<&str>),
    ) {
        let absolute = Uri::parse(base)
            .ok()
            .and_then(|base| registry.resolve_uri(&base, reference).ok());
        let Some(absolute) = absolute else {
            if !uri::valid(reference) && self.malformed_reference_scope(holder) {
                // A malformed invocation has no target to resolve. Use
                // the existing carrier preparation path to establish
                // that repairing it isolates the supported defect.
                return found(Lookup::Scope(base));
            }
            return found(Lookup::Invalid);
        };
        let (name, _) = absolute
            .as_str()
            .split_once('#')
            .unwrap_or((absolute.as_str(), ""));
        // URI normalization selects the carrier only. Preserve the
        // authored fragment for Resolver::lookup; resolve_uri can
        // normalize a percent-encoded plain name that lookup refuses.
        found(Lookup::Name(name));
    }
    // Every scalar reference with the base it resolves against, in node order.
    fn references_in<'a>(
        &'a self,
        nodes: impl Iterator<Item = (usize, &'a SpaceNode)>,
    ) -> impl Iterator<Item = (Key, usize, &'a str, &'a str)> {
        nodes.flat_map(move |(holder, node)| {
            let resource = self.resource(node.resource);
            REFERENCE_KEYWORDS
                .iter()
                .enumerate()
                .filter_map(move |(slot, keyword)| {
                    let reference = node.value.get(keyword).and_then(|v| v.as_str())?;
                    if resource.anonymous && !uri::absolute(reference) {
                        return None;
                    }
                    let base = resource
                        .base
                        .as_deref()
                        .unwrap_or("urn:openbindings:anonymous-inspection");
                    Some(((holder, slot), node.resource, reference, base))
                })
        })
    }
    // The packaged layer's lookups, the same in every space: they read only
    // packaged nodes and resources. Computed once, with the shared layer.
    fn packaged_lookups(&self) -> &[PackagedLookup] {
        self.packaged.lookups.get_or_init(|| {
            let registry = inspection_registry();
            let mut lookups = Vec::new();
            for (key, resource, reference, base) in self.references_in(self.packaged.nodes()) {
                self.look_up(&registry, key.0, reference, base, |found| {
                    lookups.push((key, resource, found.owned()));
                });
            }
            lookups
        })
    }
    fn index_inspection(&self) -> Inspection {
        let uri_registry = inspection_registry();
        let mut index = Index::default();
        for (key, resource, reference, base) in self.references_in(self.local.nodes()) {
            self.look_up(&uri_registry, key.0, reference, base, |found| {
                index.add(self, key, resource, found);
            });
        }
        for (key, resource, found) in self.packaged_lookups() {
            index.add(self, *key, *resource, found.borrowed());
        }
        let Index {
            mut routes,
            groups,
            absent,
        } = index;
        for (&id, queries) in &groups {
            for &key in &queries.references {
                routes.insert(key, Route::Carrier(id));
            }
        }
        for keys in absent.values() {
            for &key in keys {
                routes.insert(key, Route::Absent);
            }
        }
        Inspection {
            routes,
            carriers: groups
                .into_iter()
                .map(|(id, queries)| {
                    (
                        id,
                        Carrier {
                            queries: Mutex::new(Some(queries)),
                            targets: OnceLock::new(),
                            #[cfg(test)]
                            preparations: std::sync::atomic::AtomicUsize::new(0),
                        },
                    )
                })
                .collect(),
            absent: Mutex::new(Some(absent)),
            missing: OnceLock::new(),
        }
    }
    fn inspect_missing(&self, absent: BTreeMap<String, Vec<Key>>) -> Targets {
        let mut targets = Targets::new();
        // Ask the retriever through preparation, including every distinct absent
        // resource. No absent resource can be mistaken for a resolved schema.
        if !absent.is_empty() {
            // Known reference carriers are already in `names`; borrow the
            // remaining carrier names rather than copying all identity strings.
            let mut inspection_uri = "urn:openbindings:missing-inspection".to_owned();
            while self.has_name(&inspection_uri) || absent.contains_key(inspection_uri.as_str()) {
                inspection_uri.push('-');
            }
            let recording = Recording::default();
            let refs: Vec<_> = absent
                .keys()
                .map(|uri| serde_json::json!({"$ref":uri}))
                .collect();
            let _inspection = Registry::new()
                .draft(Draft::Draft202012)
                .retriever(recording.clone())
                .add(&inspection_uri, serde_json::json!({"allOf":refs}))
                .expect("private URI")
                .prepare();
            let recorded = recording.requested.lock().unwrap();
            for (name, keys) in absent {
                let refusal = if recorded.contains(&name) {
                    Refusal {
                        reason: FailureReason::ResourceUnavailable,
                        code: "resource-unavailable",
                        message: "static preparation requires a resource that was not supplied",
                        missing: Some(name.into()),
                        defect: None,
                    }
                } else {
                    Refusal::invalid(
                        "resource-inspection",
                        "the inspection registry could not identify the referenced resource",
                    )
                };
                for key in keys {
                    targets.insert(key, Err(refusal.clone()));
                }
            }
        }
        targets
    }
    fn inspect_carrier(&self, resource_id: usize, queries: Queries) -> Targets {
        let mut targets = Targets::new();

        let resource = self.resource(resource_id);
        let Some(root) = resource.root else {
            return targets;
        };
        let original = &self.node(root).value;
        let (mut tree, identity) = copy(
            original,
            &self.source(self.node(root).source).positions,
            |id| self.malformed_reference_scope(id),
        );
        let canonical = resource.base.as_deref().unwrap_or(&queries.first_name);
        // Registering a detached nested resource must preserve its already
        // established base. This only changes the root registration identity;
        // nested references and identifiers are interpreted by the library.
        if let Some(id) = tree.0.get_mut("$id") {
            *id = Value::String(canonical.to_owned());
        }
        let recording = Recording::default();
        let builder = Registry::new()
            .draft(Draft::Draft202012)
            .retriever(recording);
        let names: HashSet<_> = std::iter::once(queries.first_name.as_str())
            .chain(queries.other_names.iter().map(String::as_str))
            .chain(std::iter::once(canonical))
            .collect();
        let registry = builder
            .extend(names.into_iter().map(|name| (name, &*tree.0)))
            .and_then(|b| b.prepare());
        for key in queries.references {
            let reference = self.inspection_reference(key);
            let fragment = reference.split_once('#').map_or("", |(_, f)| f);
            if !uri::valid(reference) {
                let mut refusal = Refusal::invalid(
                    "invalid-reference",
                    "reference is not a well-formed URI reference",
                );
                if registry.is_ok() && self.eligible(resource_id) {
                    refusal = refusal.semantic(StaticReferenceDefect::MalformedReference);
                }
                targets.insert(key, Err(refusal));
                continue;
            }
            // §7.4 declaration evidence. Ambiguity is not
            // repaired by accepting the library's choice of one declaration.
            if !fragment.is_empty()
                && !fragment.starts_with('/')
                && resource.anchors.get(fragment).is_some_and(|a| a.len() > 1)
            {
                let mut refusal = Refusal::invalid(
                    "ambiguous-anchor",
                    "resource declares this plain name more than once",
                );
                // Declaration evidence alone cannot isolate an unsupported
                // carrier defect, such as a malformed sibling identifier.
                if registry.is_ok() {
                    refusal = refusal.semantic(StaticReferenceDefect::DuplicatePlainName);
                }
                targets.insert(key, Err(refusal));
                continue;
            }
            let result = match &registry {
                Ok(registry) => {
                    // RFC 6901 syntax only: never interpret pointer tokens
                    // or select a target independently of the library.
                    if fragment.starts_with('/')
                        && uri::decode_fragment(fragment).is_some_and(|decoded| {
                            let mut bytes = decoded.bytes();
                            while let Some(byte) = bytes.next() {
                                if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
                                    return true;
                                }
                            }
                            false
                        })
                    {
                        targets.insert(
                            key,
                            Err(Refusal::invalid(
                                "non-schema-target",
                                "resource pointer has an invalid escape",
                            )
                            .semantic(StaticReferenceDefect::PointerToNoSchema)),
                        );
                        continue;
                    }
                    // A fresh resolver has no dynamic history. This is the
                    // initial static target, never a runtime dynamic winner.
                    let base = self
                        .resource(self.node(key.0).resource)
                        .base
                        .as_deref()
                        .unwrap_or("urn:openbindings:anonymous-inspection");
                    let resolved = registry
                        .resolver(Uri::parse(base.to_owned()).expect("admitted base URI"))
                        .lookup(&presented_reference(reference));
                    match resolved {
                        Ok(resolved) => match identity
                            .get(&(std::ptr::from_ref(resolved.contents()) as usize))
                        {
                            Some(&node) => Ok(Target {
                                node,
                                dynamic_name: (!fragment.is_empty()
                                    && self
                                        .node(node)
                                        .value
                                        .get("$dynamicAnchor")
                                        .and_then(|v| v.as_str())
                                        == Some(fragment))
                                .then(|| fragment.to_owned()),
                            }),
                            None => {
                                let refusal = Refusal::invalid(
                                    "non-schema-target",
                                    "resource pointer does not identify an indexed schema",
                                );
                                Err(if fragment.starts_with('/') {
                                    refusal.semantic(StaticReferenceDefect::PointerToNoSchema)
                                } else {
                                    refusal
                                })
                            }
                        },
                        Err(error) => Err(if fragment.starts_with('/') {
                            let refusal = Refusal::invalid(
                                "non-schema-target",
                                "resource pointer does not identify an indexed schema",
                            );
                            match error {
                                jsonschema::ReferencingError::PointerToNowhere { .. }
                                | jsonschema::ReferencingError::InvalidArrayIndex { .. }
                                | jsonschema::ReferencingError::InvalidPercentEncoding { .. } => {
                                    refusal.semantic(StaticReferenceDefect::PointerToNoSchema)
                                }
                                _ => refusal,
                            }
                        } else {
                            let refusal = Refusal::invalid(
                                "anchor-missing",
                                "resource declares no such plain name",
                            );
                            // Encoded spellings follow the library, but its
                            // refusal is not proof that the decoded name is
                            // absent. Only literal plain names qualify.
                            if crate::schema_index::plain_name(fragment)
                                && matches!(
                                    error,
                                    jsonschema::ReferencingError::NoSuchAnchor { .. }
                                )
                            {
                                refusal.semantic(StaticReferenceDefect::MissingPlainName)
                            } else {
                                refusal
                            }
                        }),
                    }
                }
                // The documented preparation API processes the carrier,
                // including unused malformed $id siblings. These remain
                // outside the static-reference fragment.
                Err(_) => Err(Refusal::invalid(
                    "resource-inspection",
                    "the resource could not be prepared for static inspection",
                )),
            };
            targets.insert(key, result);
        }
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn preparations(space: &SchemaSpace) -> usize {
        space.inspection.get().map_or(0, |index| {
            index
                .carriers
                .values()
                .map(|carrier| carrier.preparations.load(Ordering::Relaxed))
                .sum()
        })
    }

    #[test]
    fn requested_carrier_is_lazy_and_missing_evidence_replays() {
        let document = RawDocument::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"allOf":[{"$ref":"https://resource.test/a"},{"$ref":"https://resource.test/b"}]}}},"schemas":{"a":{"$id":"https://resource.test/a","$ref":"https://resource.test/absent"},"b":{"$id":"https://resource.test/b","$ref":"https://resource.test/other"}}}"#).unwrap();
        let space = SchemaSpace::new(document, Supplied::default());
        let entry = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input/allOf/0")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        assert!(space.inspection.get().is_none());
        let target = space.resolve(entry, "https://resource.test/a").unwrap();
        assert_eq!(preparations(&space), 1);
        let index = space.inspection.get().unwrap();
        // Even another reference authored in the same resource stays lazy.
        assert!(
            index
                .carriers
                .iter()
                .filter(|(id, _)| **id != space.node(target.node).resource)
                .all(|(_, carrier)| carrier.targets.get().is_none())
        );
        // Preparing a carrier does not inspect references authored inside it.
        assert!(index.missing.get().is_none());
        for _ in 0..2 {
            let mut missing = Vec::new();
            let failure = space
                .resolve_with_missing(target.node, "https://resource.test/absent", &mut |uri| {
                    missing.push(uri.to_owned())
                })
                .unwrap_err();
            assert_eq!(missing, ["https://resource.test/absent"]);
            assert_eq!(failure.location.unwrap().pointer, "/schemas/a");
        }
        assert_eq!(preparations(&space), 1);
        assert!(index.missing.get().is_some());
        assert!(index.absent.lock().unwrap().is_none());
        assert!(
            index.carriers[&space.node(target.node).resource]
                .queries
                .lock()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn compact_routes_distinguish_keywords_and_require_original_spelling() {
        let document = RawDocument::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://route.test/a","$dynamicRef":"https://route.test/b"}}},"schemas":{"a":{"$id":"https://route.test/a"},"b":{"$id":"https://route.test/b"}}}"#).unwrap();
        let space = SchemaSpace::new(document, Supplied::default());
        let holder = space
            .document_node(
                &space
                    .document
                    .value()
                    .at("/operations/op/input")
                    .unwrap()
                    .to_owned(),
            )
            .unwrap();
        for uri in [
            "https://route.test/a",
            "https://route.test/b",
            "https://route.test/a",
        ] {
            let target = space.resolve(holder, uri).unwrap();
            assert_eq!(
                space
                    .resource(space.node(target.node).resource)
                    .base
                    .as_deref(),
                Some(uri)
            );
            // A contained schema keeps document-source provenance even with $id.
            assert!(space.location(target.node).resource.is_none());
            assert_eq!(
                space.location(target.node).pointer,
                format!("/schemas/{}", uri.rsplit('/').next().unwrap())
            );
        }
        let failure = space.resolve(holder, "https://route.test/a#").unwrap_err();
        assert_eq!(failure.code, "uninspected-reference");
    }

    #[test]
    fn fan_in_prepares_one_carrier_and_preserves_parent_checkpoints() {
        for callers in [1, 10, 100] {
            let refs: Vec<_> = (0..callers)
                .map(|i| {
                    serde_json::json!({
                        "$id":format!("https://scale.test/caller/{i}"),
                        "$ref":"https://scale.test/target"
                    })
                })
                .collect();
            let document = RawDocument::parse(
                serde_json::json!({
                    "openbindings":"0.2.0", "operations":{"op":{"input":{"allOf":refs}}}
                })
                .to_string(),
            )
            .unwrap();
            let definitions: serde_json::Map<_, _> = (0..5000)
                .map(|i| (format!("p{i}"), serde_json::json!({"type":"integer"})))
                .collect();
            let target = JsonValue::parse(
                serde_json::json!({
                    "$id":"https://scale.test/target", "type":"integer", "$defs":definitions
                })
                .to_string(),
            )
            .unwrap();
            let space = SchemaSpace::new(
                document,
                Supplied::new([ProjectedResource {
                    uri: "https://scale.test/target".into(),
                    document: target,
                }])
                .unwrap(),
            );
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
            let checks = AtomicUsize::new(0);
            let control = || {
                checks.fetch_add(1, Ordering::Relaxed);
                false
            };
            let program = space.program(entry, &control).unwrap();
            assert_eq!(program.resources.len(), callers + 2);
            // The parent's traversal, work charges and cancellation boundaries
            // are unchanged. Carrier sharing must not bypass any traversal step.
            assert_eq!(checks.load(Ordering::Relaxed), 6 * callers + 7);
            assert_eq!(preparations(&space), 1);
            // Reusing the immutable space across threads cannot multiply copies
            // or preparations, even when each thread starts with a cold cache.
            let cold = SchemaSpace::new(space.document.clone(), space.supplied.clone());
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    let cold = &cold;
                    scope.spawn(move || {
                        for (id, node) in cold.nodes().filter(|(_, n)| n.source == 0) {
                            if let Some(reference) = node.value.get("$ref").and_then(|v| v.as_str())
                            {
                                let target = cold.resolve(id, reference).unwrap();
                                assert_eq!(
                                    cold.location(target.node).resource.as_deref(),
                                    Some("https://scale.test/target")
                                );
                            }
                        }
                    });
                }
            });
            assert_eq!(preparations(&cold), 1);
        }
    }
}

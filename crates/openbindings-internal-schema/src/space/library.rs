//! Static inspection through documented library APIs. Resource selection is
//! embedding/input policy; all ordinary pointer and anchor lookups are delegated.
use super::*;
use jsonschema::{Draft, Registry, Retrieve, Uri};
use serde_json::Value;
use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(super) struct Refusal {
    reason: FailureReason,
    code: &'static str,
    message: &'static str,
    missing: Option<String>,
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
pub(super) type Targets = HashMap<(usize, String), Result<Target, Refusal>>;

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
        let referring_resource = self.nodes[holder].resource;
        match self.resources[referring_resource]
            .inspection
            .get_or_init(|| self.inspect_library(referring_resource))
            .get(&(holder, reference.to_owned()))
        {
            Some(Ok(target)) => {
                self.check_resource(self.nodes[target.node].resource, holder)?;
                Ok(target.clone())
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
    fn inspect_library(&self, referring_resource: usize) -> Targets {
        let mut targets = Targets::new();
        let uri_registry = Registry::new()
            .retriever(Recording::default())
            .prepare()
            .expect("empty registry");
        let mut groups: BTreeMap<usize, Vec<(usize, String, String, String)>> = BTreeMap::new();
        let mut absent = Vec::new();
        for (holder, node) in self.nodes.iter().enumerate() {
            // Defer unrelated queries, including those in packaged standards.
            // Do not restrict the carrier registry or change library resolution.
            if node.resource != referring_resource {
                continue;
            }
            let resource = &self.resources[node.resource];
            for keyword in ["$ref", "$dynamicRef"] {
                let Some(reference) = node.value.get(keyword).and_then(|v| v.as_str()) else {
                    continue;
                };
                if resource.anonymous && !uri::absolute(reference) {
                    continue;
                }
                let key = (holder, reference.to_owned());
                let base = resource
                    .base
                    .as_deref()
                    .unwrap_or("urn:openbindings:anonymous-inspection");
                let absolute = Uri::parse(base)
                    .ok()
                    .and_then(|base| uri_registry.resolve_uri(&base, reference).ok());
                let Some(absolute) = absolute else {
                    if !uri::valid(reference) && self.malformed_reference_scope(holder) {
                        // A malformed invocation has no target to resolve. Use
                        // the existing carrier preparation path to establish
                        // that repairing it isolates the supported defect.
                        groups.entry(node.resource).or_default().push((
                            holder,
                            reference.to_owned(),
                            base.to_owned(),
                            String::new(),
                        ));
                        continue;
                    }
                    targets.insert(
                        key,
                        Err(Refusal::invalid(
                            "invalid-reference",
                            "reference is not a well-formed URI reference",
                        )),
                    );
                    continue;
                };
                let (name, _) = absolute
                    .as_str()
                    .split_once('#')
                    .unwrap_or((absolute.as_str(), ""));
                // URI normalization selects the carrier only. Preserve the
                // authored fragment for Resolver::lookup; resolve_uri can
                // normalize a percent-encoded plain name that lookup refuses.
                let fragment = reference.split_once('#').map_or("", |(_, f)| f);
                // This association table is the contained/supplied identity policy,
                // not a pointer or plain-name resolver. A losing carrier cannot
                // become authoritative because a registry happened to index it last.
                match self.names.get(name) {
                    Some(NameSelection {
                        carrier: Some(id), ..
                    }) if self.resources[*id].eligible => {
                        groups.entry(*id).or_default().push((
                            holder,
                            reference.to_owned(),
                            name.to_owned(),
                            fragment.to_owned(),
                        ));
                    }
                    Some(NameSelection {
                        carrier: Some(_), ..
                    }) => {
                        targets.insert(key,Err(Refusal::invalid("resource-identity-conflict","the reached resource is not the selected carrier of its canonical identifier")));
                    }
                    Some(_) => {
                        targets.insert(
                            key,
                            Err(Refusal::invalid(
                                "ambiguous-resource",
                                "more than one resource carries the referenced identifier",
                            )),
                        );
                    }
                    None => {
                        absent.push((key, name.to_owned()));
                    }
                }
            }
        }
        // Ask the retriever through preparation, including every distinct absent
        // resource. No absent resource can be mistaken for a resolved schema.
        if !absent.is_empty() {
            // Known reference carriers are already in `names`; borrow the
            // remaining carrier names rather than copying all identity strings.
            let missing_names: HashSet<_> = absent.iter().map(|(_, name)| name.as_str()).collect();
            let mut inspection_uri = "urn:openbindings:missing-inspection".to_owned();
            while self.names.contains_key(&inspection_uri)
                || missing_names.contains(inspection_uri.as_str())
            {
                inspection_uri.push('-');
            }
            let recording = Recording::default();
            let refs: Vec<_> = absent
                .iter()
                .map(|(_, uri)| serde_json::json!({"$ref":uri}))
                .collect();
            let _inspection = Registry::new()
                .draft(Draft::Draft202012)
                .retriever(recording.clone())
                .add(&inspection_uri, serde_json::json!({"allOf":refs}))
                .expect("private URI")
                .prepare();
            let recorded = recording.requested.lock().unwrap();
            for (key, name) in absent {
                let refusal = if recorded.contains(&name) {
                    Refusal {
                        reason: FailureReason::ResourceUnavailable,
                        code: "resource-unavailable",
                        message: "static preparation requires a resource that was not supplied",
                        missing: Some(name),
                        defect: None,
                    }
                } else {
                    Refusal::invalid(
                        "resource-inspection",
                        "the inspection registry could not identify the referenced resource",
                    )
                };
                targets.insert(key, Err(refusal));
            }
        }
        for (resource_id, queries) in groups {
            let resource = &self.resources[resource_id];
            let Some(root) = resource.root else { continue };
            let original = &self.nodes[root].value;
            let (mut tree, identity) = copy(
                original,
                &self.sources[self.nodes[root].source].positions,
                |id| self.malformed_reference_scope(id),
            );
            let canonical = resource.base.as_ref().unwrap_or(&queries[0].2).clone();
            // Registering a detached nested resource must preserve its already
            // established base. This only changes the root registration identity;
            // nested references and identifiers are interpreted by the library.
            if let Some(id) = tree.0.get_mut("$id") {
                *id = Value::String(canonical.clone());
            }
            let recording = Recording::default();
            let builder = Registry::new()
                .draft(Draft::Draft202012)
                .retriever(recording);
            let names: HashSet<_> = queries
                .iter()
                .map(|(_, _, name, _)| name.as_str())
                .chain(std::iter::once(canonical.as_str()))
                .collect();
            let registry = builder
                .extend(names.into_iter().map(|name| (name, &*tree.0)))
                .and_then(|b| b.prepare());
            for (holder, reference, _, fragment) in queries {
                let key = (holder, reference);
                if !uri::valid(&key.1) {
                    let mut refusal = Refusal::invalid(
                        "invalid-reference",
                        "reference is not a well-formed URI reference",
                    );
                    if registry.is_ok() && resource.eligible {
                        refusal = refusal.semantic(StaticReferenceDefect::MalformedReference);
                    }
                    targets.insert(key, Err(refusal));
                    continue;
                }
                // §7.4 declaration evidence. Ambiguity is not
                // repaired by accepting the library's choice of one declaration.
                if !fragment.is_empty()
                    && !fragment.starts_with('/')
                    && resource.anchors.get(&fragment).is_some_and(|a| a.len() > 1)
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
                            && uri::decode_fragment(&fragment).is_some_and(|decoded| {
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
                        let base = self.resources[self.nodes[holder].resource]
                            .base
                            .as_deref()
                            .unwrap_or("urn:openbindings:anonymous-inspection");
                        let resolved = registry
                            .resolver(Uri::parse(base.to_owned()).expect("admitted base URI"))
                            .lookup(&presented_reference(&key.1));
                        match resolved {
                            Ok(resolved) => match identity
                                .get(&(std::ptr::from_ref(resolved.contents()) as usize))
                            {
                                Some(&node) => Ok(Target {
                                    node,
                                    dynamic_name: (!fragment.is_empty()
                                        && self.nodes[node]
                                            .value
                                            .get("$dynamicAnchor")
                                            .and_then(|v| v.as_str())
                                            == Some(fragment.as_str()))
                                    .then_some(fragment.clone()),
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
                                    | jsonschema::ReferencingError::InvalidPercentEncoding {
                                        ..
                                    } => refusal.semantic(StaticReferenceDefect::PointerToNoSchema),
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
                                if crate::schema_index::plain_name(&fragment)
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
        }
        targets
    }
}

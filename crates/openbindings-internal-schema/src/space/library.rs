//! Static inspection through documented library APIs. Resource selection is
//! embedding/input policy; all ordinary pointer and anchor lookups are delegated.
use super::*;
use jsonschema::{Draft, Registry, Retrieve, Uri};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct Refusal {
    reason: FailureReason,
    code: &'static str,
    message: &'static str,
    missing: Option<String>,
}
impl Refusal {
    fn invalid(code: &'static str, message: &'static str) -> Self {
        Self {
            reason: FailureReason::ConservativePreparation,
            code,
            message,
            missing: None,
        }
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
struct Tree(Value);
impl Drop for Tree {
    fn drop(&mut self) {
        let mut pending = vec![std::mem::take(&mut self.0)];
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
    value.as_str().map(str::to_owned).unwrap_or_else(|| {
        openbindings_internal_json::backend::fixed_string(value)
            .expect("JSON string")
            .into_owned()
    })
}
fn copy(value: &JsonValue) -> Tree {
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
        let copied = match node.kind() {
            JsonKind::Null => Value::Null,
            JsonKind::Boolean => Value::Bool(node.as_bool().unwrap()),
            JsonKind::Number => Value::Number(node.text().parse().expect("admitted exact number")),
            JsonKind::String => Value::String(scalar(node)),
            JsonKind::Array => Value::Array(node.elements().unwrap().map(&mut child).collect()),
            JsonKind::Object => Value::Object(
                node.members()
                    .unwrap()
                    .map(|m| (scalar(m.name), child(m.value)))
                    .collect(),
            ),
        };
        values.insert(node_id(&node.to_owned()), copied);
    }
    Tree(values.remove(&node_id(value)).expect("copied root"))
}

impl SchemaSpace {
    pub(super) fn library_target(
        &self,
        holder: usize,
        reference: &str,
        missing: &mut impl FnMut(&str),
    ) -> Result<Target, Failure> {
        match self
            .library
            .get_or_init(|| self.inspect_library())
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
                Err(self.failure(holder, refusal.reason, refusal.code, refusal.message))
            }
            None => Err(self.failure(
                holder,
                FailureReason::EvaluatorFailure,
                "uninspected-reference",
                "reference was not present in the original context",
            )),
        }
    }
    fn inspect_library(&self) -> Targets {
        let mut targets = Targets::new();
        let uri_registry = Registry::new()
            .retriever(Recording::default())
            .prepare()
            .expect("empty registry");
        let mut groups: BTreeMap<usize, Vec<(usize, String, String, String)>> = BTreeMap::new();
        let mut absent = Vec::new();
        for (holder, node) in self.nodes.iter().enumerate() {
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
            let recording = Recording::default();
            let refs: Vec<_> = absent
                .iter()
                .map(|(_, uri)| serde_json::json!({"$ref":uri}))
                .collect();
            let _inspection = Registry::new()
                .draft(Draft::Draft202012)
                .retriever(recording.clone())
                .add(
                    "urn:openbindings:missing-inspection",
                    serde_json::json!({"allOf":refs}),
                )
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
            let mut tree = copy(original);
            let canonical = resource.base.as_ref().unwrap_or(&queries[0].2).clone();
            // Registering a detached nested resource must preserve its already
            // established base. This only changes the root registration identity;
            // nested references and identifiers are interpreted by the library.
            if let Some(id) = tree.0.get_mut("$id") {
                *id = Value::String(canonical.clone());
            }
            // S8 integration point: build this identity table at the place copies
            // are made. Numeric-substituted copies must be paired with the same
            // original nodes here, never matched later by JSON equality.
            let mut identity = HashMap::new();
            let mut stack = vec![(original.view(), &tree.0)];
            while let Some((original, copied)) = stack.pop() {
                if let Some(id) = self.sources[self.nodes[root].source]
                    .positions
                    .get(&node_id(&original.to_owned()))
                {
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
                .extend(names.into_iter().map(|name| (name, &tree.0)))
                .and_then(|b| b.prepare());
            for (holder, reference, _, fragment) in queries {
                let key = (holder, reference);
                // §7.4 guard retained until S9. Declaration ambiguity is not
                // repaired by accepting the library's choice of one declaration.
                if !fragment.is_empty()
                    && !fragment.starts_with('/')
                    && resource.anchors.get(&fragment).is_some_and(|a| a.len() > 1)
                {
                    targets.insert(
                        key,
                        Err(Refusal::invalid(
                            "ambiguous-anchor",
                            "resource declares this plain name more than once",
                        )),
                    );
                    continue;
                }
                let result = match &registry {
                    Ok(registry) => {
                        // A fresh resolver has no dynamic history. This is the
                        // initial static target, never a runtime dynamic winner.
                        let base = self.resources[self.nodes[holder].resource]
                            .base
                            .as_deref()
                            .unwrap_or("urn:openbindings:anonymous-inspection");
                        let resolved = registry
                            .resolver(Uri::parse(base.to_owned()).expect("admitted base URI"))
                            .lookup(&key.1);
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
                                None => Err(Refusal::invalid(
                                    "non-schema-target",
                                    "resource pointer does not identify an indexed schema",
                                )),
                            },
                            Err(_) => Err(if fragment.starts_with('/') {
                                Refusal::invalid(
                                    "non-schema-target",
                                    "resource pointer does not identify an indexed schema",
                                )
                            } else {
                                Refusal::invalid(
                                    "anchor-missing",
                                    "resource declares no such plain name",
                                )
                            }),
                        }
                    }
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

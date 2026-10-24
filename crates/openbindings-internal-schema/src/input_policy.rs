//! Immutable supplied-resource input policy. Association conflicts are retained
//! for reached-resource admission, so an unused collision does not poison a job.
use crate::*;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, OnceLock},
};
#[derive(Clone, Default)]
pub struct InputPolicy {
    pub(crate) names: BTreeMap<String, Vec<(usize, usize)>>,
    pub(crate) protected: HashSet<String>,
}
/// Admitted supplied resources with their identity policy. Clones share both.
#[derive(Clone, Default)]
pub struct Supplied {
    resources: Arc<[ProjectedResource]>,
    pub(crate) policy: Arc<InputPolicy>,
}
/// A holder of admitted supplied resources, such as the SDK's resource set, that
/// lets a schema space reuse their identity policy instead of admitting them
/// again. Not a supported API: this crate is private to the OpenBindings SDK,
/// pinned at its exact version and free to change in any release.
#[doc(hidden)]
pub trait SuppliedSet {
    /// The admitted resources and their identity policy.
    fn supplied(&self) -> &Supplied;
}
/// A document revision whose schema index the SDK already built, so the
/// default evaluator shares it rather than walking the document again on
/// every preparation. Not a supported API, like [`SuppliedSet`].
#[doc(hidden)]
pub trait IndexedDocument {
    /// The revision's value with its schema index.
    fn indexed(&self) -> crate::RawDocument;
}
struct NoRetrieval;
impl jsonschema::Retrieve for NoRetrieval {
    fn retrieve(
        &self,
        _: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("resource acquisition is disabled".into())
    }
}
pub(crate) fn canonical_uri(name: &str) -> Option<String> {
    resolve_identifier(None, name.strip_suffix('#').unwrap_or(name))
}
pub(crate) fn resolve_identifier(parent: Option<&str>, reference: &str) -> Option<String> {
    static EMPTY: OnceLock<jsonschema::Registry<'static>> = OnceLock::new();
    let registry = EMPTY.get_or_init(|| {
        jsonschema::Registry::new()
            .retriever(NoRetrieval)
            .prepare()
            .expect("empty registry")
    });
    let base = parent.or_else(|| uri::absolute(reference).then_some(reference))?;
    registry
        .resolve_uri(&jsonschema::Uri::parse(base).ok()?, reference)
        .ok()
        .map(|uri| uri.as_str().trim_end_matches('#').to_owned())
}

fn standards() -> &'static jsonschema::Registry<'static> {
    static REGISTRY: OnceLock<jsonschema::Registry<'static>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut builder = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(NoRetrieval);
        for resource in crate::space::embedded_resources() {
            let value: serde_json::Value =
                serde_json::from_str(resource.document.text()).expect("packaged JSON");
            builder = builder
                .add(&resource.uri, value)
                .expect("packaged identity");
        }
        builder
            .prepare()
            .expect("packaged resources form a closed registry")
    })
}
impl Supplied {
    pub fn iter(&self) -> impl Iterator<Item = &ProjectedResource> {
        self.resources.iter()
    }
    pub fn new(resources: impl IntoIterator<Item = ProjectedResource>) -> Result<Self, String> {
        // Collected straight into the shared slice: one allocation, as a vector
        // takes, when the source has a trusted length like the resource set's;
        // an empty set shares the allocation-free empty slice.
        let mut resources = resources.into_iter().peekable();
        let resources: Arc<[ProjectedResource]> = if resources.peek().is_some() {
            resources.collect()
        } else {
            Arc::default()
        };
        let mut policy = InputPolicy::default();
        for (source, resource) in resources.iter().enumerate() {
            let index = schema_index::SchemaIndex::supplied(&resource.document, &resource.uri);
            let mut add = |name: &str, node: usize| {
                if let Some(name) = canonical_uri(name) {
                    if standards().contains_resource(&name) {
                        policy.protected.insert(name.clone());
                    }
                    let holders = policy.names.entry(name).or_default();
                    if !holders.contains(&(source, node)) {
                        holders.push((source, node));
                    }
                }
            };
            if let Some(root) = index.nodes.first() {
                add(&resource.uri, root.id as usize);
            }
            for (node, base) in index.declared_bases() {
                add(base, index.nodes[*node as usize].id as usize);
            }
        }
        Ok(Self {
            resources,
            policy: Arc::new(policy),
        })
    }
}

impl std::fmt::Debug for InputPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputPolicy").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Supplied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supplied").finish_non_exhaustive()
    }
}

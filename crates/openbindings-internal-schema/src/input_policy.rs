//! Immutable supplied-resource input policy. Association conflicts are retained
//! for reached-resource admission, so an unused collision does not poison a job.
use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};
#[derive(Clone, Default)]
pub struct InputPolicy {
    pub(crate) identity_bytes: usize,
    pub(crate) names: BTreeMap<String, Vec<(usize, usize)>>,
    pub(crate) protected: BTreeSet<String>,
}
/// Admitted supplied resources with their identity policy. Clones share both,
/// and the resources' schema positions once a schema space first walks them.
#[derive(Clone, Default)]
pub struct Supplied {
    resources: Arc<[ProjectedResource]>,
    pub(crate) policy: Arc<InputPolicy>,
    nodes: Arc<OnceLock<Box<[schema_index::SchemaIndex]>>>,
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

impl Supplied {
    pub fn iter(&self) -> impl Iterator<Item = &ProjectedResource> {
        self.resources.iter()
    }
    /// Each resource's schema nodes ([`schema_index::SchemaIndex::supplied_nodes`]),
    /// in resource order: walked once for every space over these resources.
    pub(crate) fn nodes_bounded(
        &self,
        maximum: usize,
        control: &dyn crate::Control,
    ) -> Result<&[schema_index::SchemaIndex], crate::Failure> {
        control.check()?;
        if self.nodes.get().is_none() {
            let mut remaining = maximum;
            let mut nodes = Vec::new();
            for resource in self.resources.iter() {
                let index = schema_index::SchemaIndex::supplied_nodes_bounded(
                    &resource.document,
                    remaining,
                    control,
                )?;
                remaining -= index.nodes.len();
                nodes.push(index);
            }
            control.check()?;
            let _ = self.nodes.set(nodes.into_boxed_slice());
        }
        let nodes = self.nodes.get().expect("complete supplied inventory");
        let count = nodes
            .iter()
            .try_fold(0usize, |count, index| count.checked_add(index.nodes.len()));
        if count.is_none_or(|count| count > maximum) {
            return Err(crate::Failure::new(
                crate::FailureReason::LimitExceeded,
                "schema-nodes",
                "schema inventory exceeds its configured node limit",
            ));
        }
        Ok(nodes)
    }
    pub fn new(resources: impl IntoIterator<Item = ProjectedResource>) -> Result<Self, String> {
        Self::new_bounded(resources, 100_000, crate::DEFAULT_IDENTITY_BYTES, &|| false)
            .map_err(|failure| failure.code.into_owned())
    }
    pub fn new_bounded(
        resources: impl IntoIterator<Item = ProjectedResource>,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn Control,
    ) -> Result<Self, Failure> {
        control.check()?;
        let mut collected = Vec::new();
        let mut retrieval_bytes = 0usize;
        for resource in resources {
            control.check()?;
            if collected.len() >= maximum_nodes {
                return Err(Failure::new(
                    FailureReason::LimitExceeded,
                    "schema-nodes",
                    "external schema registration exceeds its configured node budget",
                ));
            }
            retrieval_bytes = retrieval_bytes
                .checked_add(resource.uri.len())
                .filter(|bytes| *bytes <= maximum_identity)
                .ok_or_else(crate::identity_budget::exceeded)?;
            collected.push(resource);
        }
        let resources: Arc<[ProjectedResource]> = collected.into();
        Self::indexed(
            resources,
            maximum_nodes,
            maximum_identity,
            control,
            &crate::dialect::Profile::from_uri,
        )
    }
    /// Build document-dependent indexes without changing this immutable input
    /// set or publishing them to its standard-only cache. The resolver must
    /// contain only profiles with completed carrier-stability certificates.
    pub(crate) fn with_profiles(
        &self,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn Control,
        resolve_profile: &dyn Fn(&str) -> Option<crate::dialect::Profile>,
    ) -> Result<Self, Failure> {
        Self::indexed(
            self.resources.clone(),
            maximum_nodes,
            maximum_identity,
            control,
            resolve_profile,
        )
    }
    fn indexed(
        resources: Arc<[ProjectedResource]>,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn Control,
        resolve_profile: &dyn Fn(&str) -> Option<crate::dialect::Profile>,
    ) -> Result<Self, Failure> {
        control.check()?;
        // First walk only positions and integer length bounds. Expanding URI
        // bases in the ordinary association index is allowed only after every
        // source fits the cumulative budget.
        let mut policy = InputPolicy::default();
        let mut remaining_nodes = maximum_nodes;
        let mut indexes = Vec::new();
        for resource in resources.iter() {
            let index = schema_index::SchemaIndex::supplied_profiled(
                &resource.document,
                None,
                remaining_nodes,
                control,
                resolve_profile,
            )?;
            remaining_nodes -= index.nodes.len();
            let bytes = crate::identity_budget::measure(
                &index,
                Some(&resource.uri),
                maximum_identity - policy.identity_bytes,
                control,
            )?;
            policy.identity_bytes += bytes;
            indexes.push(index);
        }
        crate::preparation::identities(policy.identity_bytes, control)?;
        for (source, resource) in resources.iter().enumerate() {
            control.check()?;
            let index = schema_index::SchemaIndex::supplied_profiled(
                &resource.document,
                Some(&resource.uri),
                maximum_nodes,
                control,
                resolve_profile,
            )?;
            let mut add = |name: &str, node: usize| {
                if let Some(name) = canonical_uri(name) {
                    if crate::space::standard_identifier(&name) {
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
                control.check()?;
                add(base, index.nodes[*node as usize].id as usize);
            }
        }
        control.check()?;
        Ok(Self {
            resources,
            policy: Arc::new(policy),
            nodes: Arc::new(OnceLock::from(indexes.into_boxed_slice())),
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

#[cfg(test)]
mod profile_tests {
    use super::*;
    use crate::dialect::Profile;

    fn profile(applicator: bool) -> Profile {
        let extra = if applicator {
            r#", "https://json-schema.org/draft/2020-12/vocab/applicator":false"#
        } else {
            ""
        };
        let value = JsonValue::parse(format!(
            r#"{{"https://json-schema.org/draft/2020-12/vocab/core":true{extra}}}"#
        ))
        .unwrap();
        Profile::from_vocabulary(Some(value.view())).unwrap()
    }
    fn supplied() -> Supplied {
        Supplied::new([ProjectedResource {
            uri: "https://profiles.test/schema".into(),
            document: JsonValue::parse(r#"{"$schema":"https://profiles.test/meta","properties":{"x":{"$id":"https://profiles.test/child","type":"integer"}}}"#).unwrap(),
        }]).unwrap()
    }

    #[test]
    fn completed_profiles_change_actual_inventory_without_mutating_shared_inputs() {
        let original = supplied();
        let clone = original.clone();
        let build = |applicator| {
            original
                .with_profiles(100, 4096, &|| false, &|uri| {
                    if uri == "https://profiles.test/meta" {
                        Some(profile(applicator))
                    } else {
                        Profile::from_uri(uri)
                    }
                })
                .unwrap()
        };
        let core_only = build(false);
        let active = build(true);
        assert!(
            !core_only
                .policy
                .names
                .contains_key("https://profiles.test/child")
        );
        assert!(
            active
                .policy
                .names
                .contains_key("https://profiles.test/child")
        );
        assert!(
            !original
                .policy
                .names
                .contains_key("https://profiles.test/child")
        );
        assert!(
            !clone
                .policy
                .names
                .contains_key("https://profiles.test/child")
        );
    }

    #[test]
    fn profile_inventory_refusal_does_not_publish_a_partial_input_policy() {
        let original = supplied();
        let resolve = |uri: &str| {
            if uri == "https://profiles.test/meta" {
                Some(profile(true))
            } else {
                Profile::from_uri(uri)
            }
        };
        let refusal = original
            .with_profiles(1, 4096, &|| false, &resolve)
            .unwrap_err();
        assert_eq!(refusal.reason, FailureReason::LimitExceeded);
        let cancellation = original
            .with_profiles(100, 4096, &|| true, &resolve)
            .unwrap_err();
        assert_eq!(cancellation.reason, FailureReason::Cancelled);
        let retry = original
            .with_profiles(100, 4096, &|| false, &resolve)
            .unwrap();
        assert!(
            retry
                .policy
                .names
                .contains_key("https://profiles.test/child")
        );
    }
}

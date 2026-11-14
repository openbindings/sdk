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
pub(crate) fn canonical_uri(name: &str) -> Option<String> {
    resolve_identifier(None, name.strip_suffix('#').unwrap_or(name))
}
/// Canonical alias of an absolute base from a spelling-preserving resolution.
/// Keep normalization in the URI library. Its normalizer removes dot segments
/// from absolute paths; rootless paths still need the ordinary resolver.
pub(crate) fn canonical_resolved_uri(name: &str) -> Option<String> {
    normalized_hierarchical_uri(name)
        .map(jsonschema::Uri::into_string)
        .or_else(|| canonical_uri(name))
}
// An absolute hierarchical reference replaces the base completely. The URI
// library's normalizer also removes its dot segments, so no resolution is
// needed. Rootless paths retain the library's full resolution semantics.
// Required for the reference-fan-in allocation gate. Differential guard:
// identifier_resolution_tests::direct_resolution_agrees_with_a_registry.
fn normalized_hierarchical_uri(name: &str) -> Option<jsonschema::Uri<String>> {
    let uri = jsonschema::Uri::parse(name).ok()?;
    if uri.path().is_empty() || uri.path().as_str().starts_with('/') {
        Some(uri.normalize())
    } else {
        None
    }
}
pub(crate) fn resolve_identifier(parent: Option<&str>, reference: &str) -> Option<String> {
    let base = parent.or_else(|| uri::absolute(reference).then_some(reference))?;
    // The evaluator's own resolution, called directly. A registry caches every
    // base and reference it resolves, so a process-wide one would keep each
    // caller's identifiers for the life of the process.
    let base = jsonschema::Uri::parse(base).ok()?;
    let uri = normalized_hierarchical_uri(reference)
        .or_else(|| jsonschema::uri::resolve_against(&base, reference).ok())?;
    let mut resolved = uri.into_string();
    resolved.truncate(resolved.trim_end_matches('#').len());
    Some(resolved)
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
            if let Some(root) = index.nodes.first()
                && let Some(name) = canonical_uri(&resource.uri)
            {
                policy.add_canonical_name(name, source, root.id as usize);
            }
            for (node, base) in index.declared_bases() {
                control.check()?;
                if let Some(name) = canonical_resolved_uri(base) {
                    policy.add_canonical_name(
                        name,
                        source,
                        index.nodes[*node as usize].id as usize,
                    );
                }
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

impl InputPolicy {
    // Retrieval names pass through canonical_uri; resolved declared bases pass
    // through canonical_resolved_uri. Neither path registers unchecked text.
    fn add_canonical_name(&mut self, name: String, source: usize, node: usize) {
        if crate::space::standard_identifier(&name) {
            self.protected.insert(name.clone());
        }
        let holders = self.names.entry(name).or_default();
        if !holders.contains(&(source, node)) {
            holders.push((source, node));
        }
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

#[cfg(test)]
pub(crate) mod identifier_resolution_tests {
    use super::*;

    pub(crate) fn uri_corpus() -> Vec<String> {
        let mut corpus = Vec::new();
        for prefix in [
            "http://EXAMPLE.test:80",
            "HTTPS://User:pass@Example.test:443",
            "ftp://example.test:21",
            "file://",
            "x://[2001:0DB8:0:0::1]:90",
            "https://[::ffff:192.0.2.1]",
            "custom:",
        ] {
            for path in [
                "",
                "/",
                "/a",
                "/a/./b/../c",
                "/../a",
                "/a//b/",
                "/%7e/%41/%2f",
                "/a/%2e%2e/b",
                "/.%2E/a",
                "/%FF/%00",
                "/.//a",
                "/a/..",
            ] {
                for query in ["", "?", "?a=%7e&b=%2f", "?q=/../%2e"] {
                    for fragment in ["", "#", "#anchor", "#/%7e/%2f", "#%23"] {
                        corpus.push(format!("{prefix}{path}{query}{fragment}"));
                    }
                }
            }
        }
        for path in [
            "example:item",
            "a/../b",
            "a/%2e%2e/b",
            "EXAMPLE:%7e:%2f",
            "",
        ] {
            for fragment in ["", "#", "#frag", "#/%41"] {
                corpus.push(format!("URN:{path}?q=%7e{fragment}"));
            }
        }
        corpus.extend(
            [
                "",
                "relative",
                "#",
                "#frag",
                "not a URI",
                "https://[::g]",
                "https://x/%",
            ]
            .map(str::to_owned),
        );
        corpus
    }

    #[test]
    fn canonical_uri_is_idempotent() {
        for raw in uri_corpus() {
            if let Some(canonical) = canonical_uri(&raw) {
                assert_eq!(
                    canonical_uri(&canonical).as_ref(),
                    Some(&canonical),
                    "{raw}"
                );
            } else {
                assert!(
                    !uri::absolute(&raw),
                    "absolute corpus member must resolve: {raw}"
                );
            }
        }
    }

    #[test]
    fn resolved_base_normalization_agrees_with_canonicalizing_first() {
        for raw in uri_corpus() {
            if let Some(resolved) = uri::compared_id(None, &raw) {
                assert_eq!(
                    canonical_resolved_uri(&resolved),
                    canonical_uri(&resolved),
                    "{raw} -> {resolved}"
                );
            }
        }
    }

    #[test]
    fn indexed_canonical_names_preserve_canonicalizing_registration() {
        let retrieval = "HTTPS://INDEX.test:443/%7e/retrieval#";
        for raw in uri_corpus() {
            let value = JsonValue::parse(serde_json::json!({"$id": raw}).to_string()).unwrap();
            let index = schema_index::SchemaIndex::supplied(&value, retrieval);
            let mut expected = BTreeMap::<String, Vec<(usize, usize)>>::new();
            let mut add = |name: &str, node| {
                if let Some(name) = canonical_uri(name) {
                    let holders = expected.entry(name).or_default();
                    if !holders.contains(&(0, node)) {
                        holders.push((0, node));
                    }
                }
            };
            add(retrieval, index.nodes[0].id as usize);
            for (node, base) in index.declared_bases() {
                assert_eq!(canonical_resolved_uri(base), canonical_uri(base), "{raw}");
                add(base, index.nodes[*node as usize].id as usize);
            }
            let supplied = Supplied::new([ProjectedResource {
                uri: retrieval.into(),
                document: value,
            }])
            .unwrap();
            assert_eq!(supplied.policy.names, expected, "{raw}");
        }
    }
    #[test]
    fn direct_resolution_agrees_with_a_registry() {
        let registry = jsonschema::Registry::new().prepare().unwrap();
        for base in [
            "https://EXAMPLE.test/a/b",
            "HTTPS://EXAMPLE.test:443/%7e/base#",
            "https://test.invalid/a/%7e/b?x#old",
            "urn:example:item#frag",
        ] {
            let mut references = uri_corpus();
            references.extend(
                [
                    "",
                    "a/",
                    "../c",
                    "/a/./b/../c",
                    "?q",
                    "#frag",
                    "#old",
                    "#",
                    "%2e%2e/x",
                    "https://OTHER.test/%7e/a",
                    "not a URI",
                ]
                .map(str::to_owned),
            );
            for reference in references {
                let expected = registry
                    .resolve_uri(&jsonschema::Uri::parse(base).unwrap(), &reference)
                    .ok()
                    .map(|uri| uri.as_str().trim_end_matches('#').to_owned());
                assert_eq!(
                    resolve_identifier(Some(base), &reference),
                    expected,
                    "{base} {reference}"
                );
            }
        }
    }
}

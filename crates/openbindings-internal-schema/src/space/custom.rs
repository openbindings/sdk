//! Transactional custom-profile certification over one original schema space.
//! The selected meta-schema's own interpretation and its declared consumer
//! profile are separate facts. Certification does not prove consumer syntax.
use super::*;
use crate::dialect::{Dialect, Profile, ProfileConstructionFailure, ProfileFailure};
use potential::{Original, Potential};

#[derive(Default)]
pub(super) struct Custom {
    pub unknown_namespace: bool,
    pub metas: BTreeMap<String, Result<usize, QueryFailure>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Selection {
    query: Original,
    target: Original,
    own_profile: Profile,
}
struct Candidate {
    selection: Selection,
    query_resource: usize,
    target_resource: usize,
    profile: Profile,
}

impl SchemaSpace {
    fn original(&self, node: usize) -> Original {
        let node = self.node(node);
        Original {
            source: node.source,
            node: node_id(&node.value),
        }
    }

    fn selected_meta(&self, uri: &str) -> Result<(Selection, usize, usize, usize), QueryFailure> {
        let query_resource = self.named_resource(uri)?;
        let target = self.query_fragment(query_resource, uri)?.node;
        let node = self.node(target);
        let resource = self.resource(node.resource);
        if resource.root != Some(target) {
            return Err(QueryFailure::UnsupportedCapability(
                "custom-meta-resource-root",
            ));
        }
        let own_profile = node
            .profile
            .filter(|p| *p == Profile::standard(Dialect::Draft202012))
            .ok_or(QueryFailure::UnsupportedCapability(
                "custom-meta-own-profile",
            ))?;
        let query_root =
            self.resource(query_resource)
                .root
                .ok_or(QueryFailure::UnsupportedCapability(
                    "custom-meta-query-root",
                ))?;
        Ok((
            Selection {
                query: self.original(query_root),
                target: self.original(target),
                own_profile,
            },
            query_resource,
            node.resource,
            target,
        ))
    }

    fn stable_candidate(
        &self,
        uri: &str,
        candidate: &Candidate,
        potential: &Potential,
        control: &dyn Control,
    ) -> Result<bool, Failure> {
        let query_name = uri.split_once('#').map_or(uri, |(name, _)| name);
        // Retrieval/path carrier and the selected target's canonical carrier
        // are separate when a pointer crosses an embedded resource boundary.
        for (id, lookup) in [
            (candidate.query_resource, Some(query_name)),
            (
                candidate.query_resource,
                self.resource(candidate.query_resource).base.as_deref(),
            ),
            (
                candidate.target_resource,
                self.resource(candidate.target_resource).base.as_deref(),
            ),
        ] {
            control.check()?;
            let resource = self.resource(id);
            if resource.origin == ResourceOrigin::Contained {
                continue;
            }
            let Some(root) = resource.root else {
                return Ok(false);
            };
            let Some(name) = lookup else { return Ok(false) };
            crate::preparation::identities(name.len(), control)?;
            control.charge_work(name.len().saturating_mul(8).saturating_add(1))?;
            control.charge_allocation(name.len().saturating_mul(4).saturating_add(128))?;
            let Some(name) = crate::input_policy::canonical_uri(name) else {
                return Ok(false);
            };
            if !potential.stable_name(&name, self.original(root)) {
                return Ok(false);
            }
        }
        let query = self.resource(candidate.query_resource);
        if query.origin != ResourceOrigin::Contained
            && let Some((_, fragment)) = uri.split_once('#')
        {
            control.charge_work(fragment.len().saturating_mul(4).saturating_add(1))?;
            control.charge_allocation(fragment.len().saturating_mul(4).saturating_add(128))?;
            let Some(fragment) = uri::decode_fragment(fragment) else {
                return Ok(false);
            };
            if !fragment.is_empty()
                && !fragment.starts_with('/')
                && !potential.stable_anchor(
                    candidate.selection.query,
                    &fragment,
                    candidate.selection.target,
                )
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Complete one profile transaction, without validating any consumer
    /// against its selected meta-schema. Callers must perform that distinct
    /// preparation obligation before producing a usable value validator.
    pub(super) fn complete_custom(
        mut self,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn Control,
    ) -> Result<Self, Failure> {
        let requests =
            Potential::discover(&self.supplied, maximum_nodes, maximum_identity, control)?;
        if requests.is_empty() {
            return Ok(self);
        }
        let mut candidates = BTreeMap::new();
        let mut profiles = BTreeMap::new();
        let mut refused = BTreeMap::new();
        for uri in requests {
            crate::preparation::identities(uri.len(), control)?;
            control.charge_work(uri.len().saturating_mul(8).saturating_add(1))?;
            control.charge_allocation(uri.len().saturating_mul(8).saturating_add(1024))?;
            let (selection, query_resource, target_resource, target) =
                match self.selected_meta(&uri) {
                    Ok(selected) => selected,
                    Err(failure) => {
                        refused.insert(uri, failure);
                        continue;
                    }
                };
            let profile = match Profile::from_vocabulary_with_control(
                self.node(target).value.view().get("$vocabulary"),
                control,
            ) {
                Ok(profile) => profile,
                Err(ProfileConstructionFailure::Work(failure)) => return Err(failure),
                Err(ProfileConstructionFailure::Declaration(failure)) => {
                    let failure = match failure {
                        ProfileFailure::MissingVocabularyMap => {
                            QueryFailure::UnsupportedCapability("custom-vocabulary-map-required")
                        }
                        ProfileFailure::UnsupportedRequiredVocabulary => {
                            QueryFailure::UnsupportedCapability("required-vocabulary")
                        }
                        ProfileFailure::InvalidVocabularyMap
                        | ProfileFailure::MissingCore
                        | ProfileFailure::CoreNotRequired => QueryFailure::InvalidExternalSchema,
                    };
                    refused.insert(uri, failure);
                    continue;
                }
            };
            profiles.insert(uri.clone(), profile);
            candidates.insert(
                uri,
                Candidate {
                    selection,
                    query_resource,
                    target_resource,
                    profile,
                },
            );
        }
        let potential = Potential::scan(
            &self.supplied,
            &profiles,
            maximum_nodes,
            maximum_identity,
            control,
        )?;
        let mut stable = BTreeMap::new();
        let mut external_group = refused.is_empty() && !potential.unknown_namespace;
        for (uri, candidate) in &candidates {
            let certified = self.stable_candidate(uri, candidate, &potential, control)?;
            if !certified {
                external_group = false;
            }
            stable.insert(uri.clone(), certified);
        }
        profiles.clear();
        for (uri, candidate) in &candidates {
            let contained = self.resource(candidate.query_resource).origin
                == ResourceOrigin::Contained
                && self.resource(candidate.target_resource).origin == ResourceOrigin::Contained;
            if stable[uri] && (contained || external_group) {
                profiles.insert(uri.clone(), candidate.profile);
            } else {
                refused.insert(
                    uri.clone(),
                    QueryFailure::UnsupportedCapability("custom-meta-carrier-stability"),
                );
            }
        }
        let unknown_namespace = !external_group;
        if profiles.is_empty() {
            self.custom = Custom {
                unknown_namespace,
                metas: refused
                    .into_iter()
                    .map(|(uri, failure)| (uri, Err(failure)))
                    .collect(),
            };
            return Ok(self);
        }
        // The input set remains immutable. Only completed certificates may
        // participate in either pass of document-dependent materialization.
        let supplied =
            self.supplied
                .with_profiles(maximum_nodes, maximum_identity, control, &|uri| {
                    Profile::from_uri(uri).or_else(|| profiles.get(uri).copied())
                })?;
        let mut actual = Self::assemble(
            self.document.clone(),
            supplied,
            maximum_nodes,
            maximum_identity,
            control,
        )?;
        let mut metas: BTreeMap<_, _> = refused
            .into_iter()
            .map(|(uri, failure)| (uri, Err(failure)))
            .collect();
        for uri in profiles.keys() {
            crate::preparation::identities(uri.len(), control)?;
            control.charge_work(uri.len().saturating_mul(8).saturating_add(1))?;
            control.charge_allocation(uri.len().saturating_mul(4).saturating_add(256))?;
            let selected = actual.selected_meta(uri);
            let Ok((selection, _, _, target)) = selected else {
                return Err(Failure::new(
                    FailureReason::UnsupportedCapability,
                    "custom-meta-reselection",
                    "the completed inventory did not preserve the certified meta-schema selection",
                ));
            };
            if selection != candidates[uri].selection {
                return Err(Failure::new(
                    FailureReason::UnsupportedCapability,
                    "custom-meta-reselection",
                    "the completed inventory changed the certified original meta-schema carrier",
                ));
            }
            metas.insert(uri.clone(), Ok(target));
        }
        control.check()?;
        actual.custom = Custom {
            unknown_namespace,
            metas,
        };
        Ok(actual)
    }

    /// The actual selected meta-schema node (or a retained interpretation
    /// refusal) for a resource with a custom dialect declaration.
    pub fn custom_meta(&self, resource: usize) -> Option<Result<usize, QueryFailure>> {
        self.resource(resource)
            .dialect
            .as_deref()
            .and_then(|uri| self.custom.metas.get(uri))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const M: &str = "https://example.test/meta";
    const META: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/applicator":true}}"#;
    fn supplied(rows: &[(&str, &str)]) -> Supplied {
        Supplied::new(rows.iter().map(|(uri, document)| ProjectedResource {
            uri: (*uri).into(),
            document: JsonValue::parse(document).unwrap(),
        }))
        .unwrap()
    }
    fn preliminary(document: &str, supplied: Supplied) -> SchemaSpace {
        SchemaSpace::assemble(
            RawDocument::parse(document).unwrap(),
            supplied,
            10_000,
            1024 * 1024,
            &|| false,
        )
        .unwrap()
    }
    fn finish(space: SchemaSpace) -> SchemaSpace {
        crate::preparation::Attempt::run(1_000_000, 64 * 1024 * 1024, &|| false, |a| {
            space.complete_custom(10_000, 1024 * 1024, a)
        })
        .unwrap()
        .unwrap()
    }
    const DOC: &str = r#"{"schemas":{"entry":{}}}"#;

    #[test]
    fn actual_custom_indexes_share_original_meta_carriers_and_not_literal_positions() {
        let supplied = supplied(&[
            (M, META),
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","properties":{"x":{"type":"number"}},"definitions":{"opaque":{"$id":"https://example.test/literal"}}}"#,
            ),
        ]);
        let old = supplied.clone();
        let space = finish(preliminary(DOC, supplied));
        let consumer = space
            .named_resource("https://example.test/consumer")
            .unwrap();
        let root = space.resource(consumer).root.unwrap();
        assert!(space.node(root).profile.unwrap().is_custom());
        let meta = space.custom_meta(consumer).unwrap().unwrap();
        assert_eq!(
            space.node(meta).profile,
            Some(Profile::standard(Dialect::Draft202012))
        );
        assert_eq!(space.resource(space.node(meta).resource).root, Some(meta));
        assert!(space.query(root, "#/properties/x").is_ok());
        assert_eq!(
            space.query(root, "#/definitions/opaque").err(),
            Some(QueryFailure::NonSchemaTarget)
        );
        assert_eq!(
            old.nodes_bounded(10_000, &|| false).unwrap()[1].nodes.len(),
            1
        );
        assert!(!space.custom.unknown_namespace);
    }

    #[test]
    fn one_unstable_external_candidate_cannot_certify_another_from_the_same_assumptions() {
        let mut rows = vec![
            (M, META),
            ("https://example.test/other-meta", META),
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","properties":{"x":{"$id":"https://example.test/meta"}}}"#,
            ),
            (
                "https://example.test/other-consumer",
                r#"{"$schema":"https://example.test/other-meta"}"#,
            ),
        ];
        for _ in 0..2 {
            let space = finish(preliminary(DOC, supplied(&rows)));
            assert!(space.custom.unknown_namespace);
            for uri in [M, "https://example.test/other-meta"] {
                assert_eq!(
                    space.custom.metas[uri],
                    Err(QueryFailure::UnsupportedCapability(
                        "custom-meta-carrier-stability"
                    ))
                );
            }
            assert!(
                space
                    .local
                    .nodes
                    .iter()
                    .all(|node| node.profile.is_none_or(|p| !p.is_custom()))
            );
            rows.reverse();
        }
    }

    #[test]
    fn contained_meta_precedence_survives_an_unproved_external_group() {
        let document = format!(
            r#"{{"schemas":{{"entry":{{}},"meta":{{"$id":"{M}","$vocabulary":{{"https://json-schema.org/draft/2020-12/vocab/core":true}}}}}}}}"#
        );
        let space = finish(preliminary(
            &document,
            supplied(&[
                (
                    "https://example.test/consumer",
                    r#"{"$schema":"https://example.test/meta"}"#,
                ),
                (
                    "https://example.test/unproved",
                    r#"{"$schema":"https://example.test/absent-meta"}"#,
                ),
            ]),
        ));
        assert!(space.custom.unknown_namespace);
        assert!(space.custom.metas[M].is_ok());
        let meta_resource = space.named_resource(M).unwrap();
        assert_eq!(
            space.resource(meta_resource).origin,
            ResourceOrigin::Contained
        );
        assert_eq!(
            space.named_resource("https://example.test/consumer"),
            Err(QueryFailure::UnsupportedCapability(
                "custom-profile-namespace"
            ))
        );
        assert!(
            space
                .local
                .nodes
                .iter()
                .any(|node| node.profile.is_some_and(|p| p.is_custom()))
        );
    }

    #[test]
    fn a_meta_pointer_must_select_an_actual_resource_root() {
        let meta_document = r#"{"$defs":{"nested":{"$id":"https://example.test/embedded","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true}},"ordinary":{"$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/core":true}}}}"#;
        for (fragment, supported) in [("nested", true), ("ordinary", false)] {
            let consumer =
                format!(r#"{{"$schema":"https://example.test/bundle#/$defs/{fragment}"}}"#);
            let space = finish(preliminary(
                DOC,
                supplied(&[
                    ("https://example.test/bundle", meta_document),
                    ("https://example.test/consumer", &consumer),
                ]),
            ));
            let result =
                &space.custom.metas[&format!("https://example.test/bundle#/$defs/{fragment}")];
            if supported {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    *result,
                    Err(QueryFailure::UnsupportedCapability(
                        "custom-meta-resource-root"
                    ))
                );
            }
        }
    }
}

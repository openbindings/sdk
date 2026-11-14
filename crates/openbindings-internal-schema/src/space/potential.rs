//! Potential identities used only to certify a custom meta-schema carrier.
//! These are not aliases, schema positions, or identity-conflict evidence.
use super::*;
use crate::dialect::{Dialect, Profile, certificate::CertificateDialect};
use std::collections::BTreeSet;

/// Source *record* and original JSON node. Reusing one arena in two supplied
/// records does not make their carriers identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Original {
    pub source: usize,
    pub node: usize,
}

#[derive(Default)]
pub(super) struct Potential {
    pub unknown_namespace: bool,
    requests: BTreeSet<String>,
    names: BTreeMap<String, HashSet<Original>>,
    anchors: BTreeMap<(Original, String), HashSet<Original>>,
    positions: usize,
    bytes: usize,
}

#[derive(Clone, Copy)]
enum Mode {
    Profile(Profile),
    Certificate(CertificateDialect),
    Unknown,
}
impl Mode {
    fn from_uri(uri: &str, profiles: &BTreeMap<String, Profile>) -> Self {
        Profile::from_uri(uri)
            .or_else(|| profiles.get(uri).copied())
            .map(Self::Profile)
            .or_else(|| CertificateDialect::from_uri(uri).map(Self::Certificate))
            .unwrap_or(Self::Unknown)
    }
    fn reference_object(self, value: JsonRef<'_>) -> bool {
        match self {
            Self::Profile(p) => p.reference_object(value),
            Self::Certificate(c) => c.reference_object(value),
            Self::Unknown => false,
        }
    }
    fn fragment_identifier(self) -> bool {
        matches!(self, Self::Profile(p) if p.edition() == Dialect::Draft7)
            || matches!(self, Self::Certificate(CertificateDialect::Draft6))
    }
    fn selector(self, keyword: &str, value: JsonRef<'_>) -> Option<schema_index::SchemaSelector> {
        match self {
            // A prospective profile is not yet authoritative. Include every
            // supported 2020-12 vocabulary's positions, even omitted ones, so
            // disabling an applicator cannot establish its own authority.
            Self::Profile(p) if p.is_custom() => {
                Profile::standard(Dialect::Draft202012).selector(keyword, value)
            }
            Self::Profile(p) => p.selector(keyword, value),
            Self::Certificate(c) => c.selector(keyword, value),
            Self::Unknown => None,
        }
    }
}

#[derive(Clone)]
struct Scope {
    mode: Mode,
    base: Arc<str>,
    carrier: Original,
}

impl Potential {
    fn charge(
        &mut self,
        amount: usize,
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        control.charge_work(amount)?;
        control.charge_allocation(amount)?;
        self.bytes = self
            .bytes
            .checked_add(amount)
            .filter(|n| *n <= maximum)
            .ok_or_else(crate::identity_budget::exceeded)?;
        Ok(())
    }
    fn reserve_name(
        &mut self,
        name: &str,
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        // URI resolution/normalization has cumulative scratch allocations in
        // addition to the retained spelling. Use the same conservative bound
        // as actual inventory construction, before entering the URI library.
        crate::preparation::identities(name.len(), control)?;
        self.charge(
            name.len().saturating_mul(3).saturating_add(512),
            maximum,
            control,
        )
    }
    fn name(
        &mut self,
        name: &str,
        carrier: Original,
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        self.reserve_name(name, maximum, control)?;
        if let Some(name) = crate::input_policy::canonical_uri(name) {
            self.names.entry(name).or_default().insert(carrier);
        } else {
            self.unknown_namespace = true;
        }
        Ok(())
    }
    // Keep the same admission charges as name(), normalizing the resolved
    // base through the library without another hierarchical resolution.
    fn resolved_name(
        &mut self,
        name: &str,
        carrier: Original,
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        self.reserve_name(name, maximum, control)?;
        if let Some(name) = crate::input_policy::canonical_resolved_uri(name) {
            self.names.entry(name).or_default().insert(carrier);
        } else {
            self.unknown_namespace = true;
        }
        Ok(())
    }
    fn anchor(
        &mut self,
        name: &str,
        carrier: Original,
        node: Original,
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        self.charge(name.len().saturating_add(512), maximum, control)?;
        self.anchors
            .entry((carrier, name.into()))
            .or_default()
            .insert(node);
        Ok(())
    }

    pub(super) fn scan(
        supplied: &Supplied,
        profiles: &BTreeMap<String, Profile>,
        maximum_positions: usize,
        maximum_bytes: usize,
        control: &dyn Control,
    ) -> Result<Self, Failure> {
        Self::walk(
            supplied,
            profiles,
            maximum_positions,
            maximum_bytes,
            control,
            false,
        )
    }

    /// Discover prospective custom dialect requests without activating them.
    /// Known profiles retain their exact selectors and reference-object rules;
    /// an unknown custom scope uses the supported 2020-12 position union only.
    /// The subsequent potential scan must still retain unknown-namespace facts.
    pub(super) fn discover(
        supplied: &Supplied,
        maximum_positions: usize,
        maximum_bytes: usize,
        control: &dyn Control,
    ) -> Result<BTreeSet<String>, Failure> {
        Ok(Self::walk(
            supplied,
            &BTreeMap::new(),
            maximum_positions,
            maximum_bytes,
            control,
            true,
        )?
        .requests)
    }

    fn walk<'a>(
        supplied: &'a Supplied,
        profiles: &BTreeMap<String, Profile>,
        maximum_positions: usize,
        maximum_bytes: usize,
        control: &dyn Control,
        discovery: bool,
    ) -> Result<Self, Failure> {
        let mut out = Self::default();
        for (source, input) in supplied.iter().enumerate() {
            control.check()?;
            let root = input.document.view();
            let carrier = Original {
                source: source + 1,
                node: ref_id(root),
            };
            out.name(&input.uri, carrier, maximum_bytes, control)?;
            out.charge(input.uri.len(), maximum_bytes, control)?;
            let scope = Scope {
                mode: Mode::Profile(Profile::standard(Dialect::Draft202012)),
                base: Arc::from(input.uri.as_str()),
                carrier,
            };
            let mut pending = Vec::new();
            out.enqueue(
                &mut pending,
                root,
                scope,
                (true, false),
                maximum_positions,
                control,
            )?;
            while let Some((value, mut scope, (root, ignore_identifier))) = pending.pop() {
                control.check()?;
                let original = Original {
                    source: source + 1,
                    node: ref_id(value),
                };
                let identifier = (!ignore_identifier).then(|| value.get("$id")).flatten();
                let inherited = scope.mode;
                let inherited_ref = inherited.reference_object(value);
                if !root
                    && !ignore_identifier
                    && matches!(inherited, Mode::Profile(p) if p.edition() != Dialect::Draft7)
                    && identifier
                        .and_then(|v| v.as_str())
                        .and_then(|id| id.strip_prefix('#'))
                        .is_some_and(schema_index::plain_name)
                {
                    // If the invalid identifier establishes no new resource,
                    // a sibling $schema cannot replace the enclosing profile.
                    // Retain that selector/carrier branch as well as the
                    // possible resource-boundary branch below.
                    out.enqueue(
                        &mut pending,
                        value,
                        scope.clone(),
                        (false, true),
                        maximum_positions,
                        control,
                    )?;
                }
                let fragment = inherited.fragment_identifier()
                    && identifier
                        .and_then(|v| v.as_str())
                        .is_some_and(|id| id.starts_with('#'));
                let boundary = root || (identifier.is_some() && !inherited_ref && !fragment);
                if boundary && let Some(schema) = value.get("$schema") {
                    control.charge_work(schema.text().len().saturating_add(1))?;
                    if let Some(uri) = schema.as_str()
                        && Profile::from_uri(uri).is_none()
                        && CertificateDialect::from_uri(uri).is_none()
                        && crate::uri::absolute(uri)
                        && !out.requests.contains(uri)
                    {
                        out.charge(uri.len().saturating_add(256), maximum_bytes, control)?;
                        out.requests.insert(uri.into());
                    }
                    scope.mode = schema
                        .as_str()
                        .map_or(Mode::Unknown, |uri| Mode::from_uri(uri, profiles));
                }
                match scope.mode {
                    Mode::Unknown if discovery => {
                        scope.mode = Mode::Profile(Profile::standard(Dialect::Draft202012));
                    }
                    Mode::Unknown => {
                        // Arbitrary mandatory vocabularies can define names or
                        // child positions outside every finite selector union.
                        out.unknown_namespace = true;
                        continue;
                    }
                    _ => {}
                }
                if let Mode::Certificate(c) = scope.mode
                    && let Some(fields) = value.members()
                {
                    for field in fields {
                        control.charge_work(field.name.text().len().saturating_add(1))?;
                        if !field
                            .name
                            .as_str()
                            .is_some_and(|name| c.known_keyword(name))
                        {
                            out.unknown_namespace = true;
                        }
                    }
                }
                let declaring = if root { scope.mode } else { inherited };
                let ignored_identifier = if root {
                    scope.mode.reference_object(value)
                } else {
                    inherited_ref
                };
                if !ignored_identifier && let Some(identifier) = identifier {
                    let Some(id) = identifier.as_str() else {
                        out.unknown_namespace = true;
                        continue;
                    };
                    control.charge_work(id.len().saturating_add(1))?;
                    // A plain fragment alone cannot change the URI base. For
                    // modern profiles this is not a valid $id, but its possible
                    // effects are bounded: a local alias, or another carrier of
                    // the enclosing name. Retain both as potential evidence and
                    // continue the known selector walk, including absolute resets.
                    // Neither interpretation is installed as a real identity.
                    let bounded_fragment = matches!(declaring, Mode::Profile(p) if p.edition() != Dialect::Draft7)
                        && id.strip_prefix('#').is_some_and(schema_index::plain_name);
                    if bounded_fragment {
                        out.anchor(&id[1..], scope.carrier, original, maximum_bytes, control)?;
                        out.name(&scope.base, original, maximum_bytes, control)?;
                        scope.carrier = original;
                    }
                    let known = match declaring {
                        Mode::Certificate(c) => c.identifier(id),
                        Mode::Profile(p) if p.edition() == Dialect::Draft7 => {
                            crate::uri::valid(id)
                                && (id
                                    .strip_prefix('#')
                                    .is_some_and(crate::dialect::draft7_name)
                                    || crate::uri::fragment(id).is_none_or(str::is_empty))
                        }
                        Mode::Profile(_) => {
                            bounded_fragment
                                || (crate::uri::valid(id)
                                    && crate::uri::fragment(id).is_none_or(str::is_empty))
                        }
                        Mode::Unknown => false,
                    };
                    if !known {
                        out.unknown_namespace = true;
                        continue;
                    }
                    if bounded_fragment
                        || (declaring.fragment_identifier() && id.starts_with('#') && id.len() > 1)
                    {
                        out.anchor(&id[1..], scope.carrier, original, maximum_bytes, control)?;
                    } else {
                        let identity_bytes = scope
                            .base
                            .len()
                            .checked_add(id.len())
                            .ok_or_else(crate::identity_budget::exceeded)?;
                        crate::preparation::identities(identity_bytes, control)?;
                        out.charge(
                            scope.base.len().saturating_add(id.len()).saturating_mul(4),
                            maximum_bytes,
                            control,
                        )?;
                        let Some(base) =
                            crate::input_policy::resolve_identifier(Some(&scope.base), id)
                        else {
                            out.unknown_namespace = true;
                            continue;
                        };
                        out.resolved_name(&base, original, maximum_bytes, control)?;
                        scope.base = Arc::from(base);
                        scope.carrier = original;
                    }
                }
                if let Mode::Certificate(CertificateDialect::V1) = scope.mode {
                    control.charge_work(scope.base.len().saturating_add(1))?;
                    if !crate::dialect::certificate::normalized_ascii_reference(&scope.base) {
                        out.unknown_namespace = true;
                        continue;
                    }
                }
                if scope.mode.reference_object(value) {
                    continue;
                }
                for keyword in ["$anchor", "$dynamicAnchor"] {
                    let Some(anchor) = value.get(keyword) else {
                        continue;
                    };
                    let enabled = match scope.mode {
                        Mode::Profile(p) => p.edition() != Dialect::Draft7,
                        Mode::Certificate(c) => c == CertificateDialect::V1,
                        Mode::Unknown => false,
                    };
                    if !enabled {
                        continue;
                    }
                    let Some(name) = anchor.as_str() else {
                        out.unknown_namespace = true;
                        continue;
                    };
                    control.charge_work(name.len().saturating_add(1))?;
                    let known = match scope.mode {
                        Mode::Profile(p) => p.anchor_name(name),
                        Mode::Certificate(c) => c.anchor(name),
                        Mode::Unknown => false,
                    };
                    if known {
                        // v1 dynamic anchors are not ordinary aliases. Treating
                        // them as possible collisions here only narrows a proof;
                        // this entry is never installed in the real registry.
                        out.anchor(name, scope.carrier, original, maximum_bytes, control)?;
                    } else {
                        out.unknown_namespace = true;
                    }
                }
                if let Some(fields) = value.members() {
                    for field in fields {
                        control.charge_work(field.name.text().len().saturating_add(1))?;
                        let Some(selector) = field
                            .name
                            .as_str()
                            .and_then(|k| scope.mode.selector(k, field.value))
                        else {
                            continue;
                        };
                        let mut add = |child: JsonRef<'a>| {
                            control.check()?;
                            if matches!(child.kind(), JsonKind::Object | JsonKind::Boolean) {
                                out.enqueue(
                                    &mut pending,
                                    child,
                                    scope.clone(),
                                    (false, false),
                                    maximum_positions,
                                    control,
                                )?;
                            }
                            Ok::<_, Failure>(())
                        };
                        use schema_index::SchemaSelector;
                        match selector {
                            SchemaSelector::Direct => add(field.value)?,
                            SchemaSelector::Array => {
                                if let Some(children) = field.value.elements() {
                                    for child in children {
                                        add(child)?;
                                    }
                                }
                            }
                            SchemaSelector::Map | SchemaSelector::MapOrStringArray => {
                                if let Some(children) = field.value.members() {
                                    for child in children {
                                        add(child.value)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        control.check()?;
        Ok(out)
    }

    fn enqueue<'a>(
        &mut self,
        pending: &mut Vec<(JsonRef<'a>, Scope, (bool, bool))>,
        value: JsonRef<'a>,
        scope: Scope,
        state: (bool, bool),
        maximum: usize,
        control: &dyn Control,
    ) -> Result<(), Failure> {
        control.check()?;
        if self.positions >= maximum {
            return Err(Failure::new(
                FailureReason::LimitExceeded,
                "custom-profile-positions",
                "potential schema inventory exceeds its configured position limit",
            ));
        }
        self.positions += 1;
        control.charge_allocation(256)?;
        pending.push((value, scope, state));
        Ok(())
    }

    /// All possible associations at supplied priority name this original
    /// carrier. The caller separately applies contained/protected precedence.
    pub(super) fn stable_name(&self, canonical: &str, original: Original) -> bool {
        !self.unknown_namespace
            && self
                .names
                .get(canonical)
                .is_none_or(|claims| claims.iter().all(|claim| *claim == original))
    }
    pub(super) fn stable_anchor(&self, carrier: Original, name: &str, original: Original) -> bool {
        !self.unknown_namespace
            && self
                .anchors
                .get(&(carrier, name.into()))
                .is_none_or(|claims| claims.iter().all(|claim| *claim == original))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_name_preserves_associations_and_admission() {
        for raw in crate::input_policy::identifier_resolution_tests::uri_corpus() {
            let Some(name) = crate::input_policy::resolve_identifier(
                Some("HTTPS://EXAMPLE.test:443/%7e/base#"),
                &raw,
            ) else {
                continue;
            };
            for maximum in [0, usize::MAX] {
                let mut before = Potential::default();
                let mut after = Potential::default();
                for node in [0, 0, 1] {
                    let carrier = Original { source: 1, node };
                    let old = before.name(&name, carrier, maximum, &|| false);
                    let new = after.resolved_name(&name, carrier, maximum, &|| false);
                    assert_eq!(
                        old.err().map(|f| (f.reason, f.code)),
                        new.err().map(|f| (f.reason, f.code)),
                        "{raw}"
                    );
                    assert_eq!(before.names, after.names, "{raw}");
                    assert_eq!(before.unknown_namespace, after.unknown_namespace, "{raw}");
                    assert_eq!(before.bytes, after.bytes, "{raw}");
                }
            }
        }
    }
    const META: &str = "https://example.test/meta";
    fn resources(rows: &[(&str, &str)]) -> Supplied {
        Supplied::new(rows.iter().map(|(uri, text)| ProjectedResource {
            uri: (*uri).into(),
            document: JsonValue::parse(text).unwrap(),
        }))
        .unwrap()
    }
    fn profiles() -> BTreeMap<String, Profile> {
        let map = JsonValue::parse(r#"{"https://json-schema.org/draft/2020-12/vocab/core":true}"#)
            .unwrap();
        BTreeMap::from([(
            META.into(),
            Profile::from_vocabulary(Some(map.view())).unwrap(),
        )])
    }
    fn original(supplied: &Supplied, source: usize) -> Original {
        Original {
            source: source + 1,
            node: ref_id(supplied.iter().nth(source).unwrap().document.view()),
        }
    }
    fn scan(supplied: &Supplied) -> Potential {
        Potential::scan(supplied, &profiles(), 10_000, 1024 * 1024, &|| false).unwrap()
    }

    #[test]
    fn disabled_positions_can_prevent_certification_but_never_create_real_claims() {
        let supplied = resources(&[
            (META, r#"{"$id":"https://example.test/meta"}"#),
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","properties":{"x":{"$id":"https://example.test/meta"}}}"#,
            ),
        ]);
        let potential = scan(&supplied);
        assert!(!potential.unknown_namespace);
        assert!(!potential.stable_name(META, original(&supplied, 0)));
        let profiles = profiles();
        let actual = supplied
            .with_profiles(10_000, 1024 * 1024, &|| false, &|uri| {
                Profile::from_uri(uri).or_else(|| profiles.get(uri).copied())
            })
            .unwrap();
        assert_eq!(actual.policy.names[META].len(), 1);
        assert_eq!(supplied.policy.names[META].len(), 1);
    }

    #[test]
    fn literals_and_optional_extension_payloads_do_not_add_possible_claims() {
        let supplied = resources(&[
            (META, "{}"),
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","const":{"$id":"https://example.test/meta"},"default":{"$id":"https://example.test/meta"},"examples":[{"$id":"https://example.test/meta"}],"unknown":{"$id":"https://example.test/meta"}}"#,
            ),
        ]);
        let potential = scan(&supplied);
        assert!(potential.stable_name(META, original(&supplied, 0)));
    }

    #[test]
    fn canonical_identity_and_retrieval_alias_are_both_protected() {
        let supplied = resources(&[
            (
                "https://example.test/alias",
                r#"{"$id":"https://example.test/meta"}"#,
            ),
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","$defs":{"x":{"$id":"https://EXAMPLE.TEST/meta"}}}"#,
            ),
        ]);
        let potential = scan(&supplied);
        assert!(potential.stable_name("https://example.test/alias", original(&supplied, 0)));
        assert!(!potential.stable_name(META, original(&supplied, 0)));
    }

    #[test]
    fn equal_json_and_reused_arenas_do_not_merge_source_records() {
        let value = JsonValue::parse(r#"{"$id":"https://example.test/meta"}"#).unwrap();
        let supplied = Supplied::new(["https://example.test/a", "https://example.test/b"].map(
            |uri| ProjectedResource {
                uri: uri.into(),
                document: value.clone(),
            },
        ))
        .unwrap();
        assert_eq!(original(&supplied, 0).node, original(&supplied, 1).node);
        assert_ne!(original(&supplied, 0), original(&supplied, 1));
        assert!(!scan(&supplied).stable_name(META, original(&supplied, 0)));
    }

    #[test]
    fn nested_relative_bases_and_absolute_resets_are_accounted_for() {
        let supplied = resources(&[
            (META, "{}"),
            (
                "https://example.test/base/",
                r#"{"$schema":"https://example.test/meta","$defs":{"a":{"$id":"child/","$defs":{"b":{"$id":"../../meta"},"c":{"$id":"https://elsewhere.test/reset"}}}}}"#,
            ),
        ]);
        let potential = scan(&supplied);
        assert!(!potential.stable_name(META, original(&supplied, 0)));
        assert_eq!(potential.names["https://elsewhere.test/reset"].len(), 1);
        assert!(!potential.unknown_namespace);
    }

    #[test]
    fn unsupported_scope_cannot_prove_a_name_absent_or_unique() {
        for consumer in [
            r#"{"$schema":"https://unknown.test/dialect","hidden":{"$id":"https://example.test/meta"}}"#,
            r#"{"$schema":"https://json-schema.org/v1","extension":true}"#,
            r#"{"$schema":"https://json-schema.org/v1","$id":"https://example.test/%61"}"#,
            r#"{"$schema":"https://example.test/meta","$defs":{"x":{"$id":"other#unsupported"}}}"#,
        ] {
            let supplied = resources(&[(META, "{}"), ("https://example.test/consumer", consumer)]);
            let potential = scan(&supplied);
            assert!(potential.unknown_namespace, "{consumer}");
            assert!(!potential.stable_name(META, original(&supplied, 0)));
            assert!(!potential.stable_name("https://absent.test/name", original(&supplied, 0)));
        }
    }

    #[test]
    fn guarded_foreign_editions_exclude_collisions_without_installing_profiles() {
        let supplied = resources(&[
            (META, "{}"),
            (
                "https://example.test/old",
                r#"{"$schema":"http://json-schema.org/draft-06/schema#","$ref":"anything","$id":"https://example.test/meta","definitions":{"x":{"$id":"https://example.test/meta"}}}"#,
            ),
            (
                "https://example.test/new",
                r#"{"$schema":"https://json-schema.org/v1","$defs":{"x":{"$anchor":"name"}},"x-note":{"$id":"https://example.test/meta"}}"#,
            ),
        ]);
        let potential = scan(&supplied);
        assert!(potential.stable_name(META, original(&supplied, 0)));
        let indexes = supplied.nodes_bounded(10_000, &|| false).unwrap();
        assert!(indexes[1].profile(0).is_none());
        assert!(indexes[2].profile(0).is_none());
    }

    #[test]
    fn fragment_aliases_cannot_hide_duplicate_meta_anchors() {
        let supplied = resources(&[(
            META,
            r##"{"$schema":"http://json-schema.org/draft-06/schema#","$id":"#meta","definitions":{"x":{"$id":"#meta"}}}"##,
        )]);
        let potential = scan(&supplied);
        let root = original(&supplied, 0);
        assert!(potential.stable_name(META, root));
        assert!(!potential.stable_anchor(root, "meta", root));
    }

    #[test]
    fn stopped_plain_fragment_scopes_keep_known_selectors_and_absolute_resets() {
        let harmless = resources(&[
            (META, "{}"),
            (
                "https://example.test/other",
                r##"{"definitions":{"a":{"$id":"#foo","type":"integer"}}}"##,
            ),
        ]);
        assert!(scan(&harmless).stable_name(META, original(&harmless, 0)));
        let collision = resources(&[
            (META, "{}"),
            (
                "https://example.test/other",
                r##"{"definitions":{"a":{"$id":"#foo","$defs":{"b":{"$id":"https://example.test/meta"}}}}}"##,
            ),
        ]);
        let potential = scan(&collision);
        assert!(!potential.unknown_namespace);
        assert!(!potential.stable_name(META, original(&collision, 0)));
        let own_name = resources(&[(
            META,
            r##"{"$anchor":"foo","definitions":{"a":{"$id":"#foo"}}}"##,
        )]);
        let potential = scan(&own_name);
        assert!(!potential.stable_name(META, original(&own_name, 0)));
        assert!(!potential.stable_anchor(original(&own_name, 0), "foo", original(&own_name, 0)));
        let mixed = resources(&[
            (META, "{}"),
            (
                "https://example.test/other",
                r##"{"$defs":{"a":{"$id":"#foo","$schema":"http://json-schema.org/draft-07/schema#","$ref":"#","$defs":{"b":{"$id":"https://example.test/meta"}}}}}"##,
            ),
        ]);
        assert!(!scan(&mixed).stable_name(META, original(&mixed, 0)));
        let alias = resources(&[(
            "https://example.test/alias",
            r##"{"$id":"https://example.test/meta","$defs":{"a":{"$id":"#foo"}}}"##,
        )]);
        let potential = scan(&alias);
        assert!(potential.stable_name("https://example.test/alias", original(&alias, 0)));
        assert!(!potential.stable_name(META, original(&alias, 0)));
    }

    #[test]
    fn discovery_closes_possible_custom_boundaries_without_reading_literal_data() {
        let supplied = resources(&[
            (
                "https://example.test/consumer",
                r#"{"$schema":"https://example.test/meta","properties":{"nested":{"$id":"child","$schema":"https://example.test/other-meta"}},"const":{"$schema":"https://example.test/literal"},"unknown":{"$schema":"https://example.test/extension"}}"#,
            ),
            (
                "https://example.test/old",
                r##"{"$schema":"http://json-schema.org/draft-07/schema#","$ref":"#","definitions":{"hidden":{"$id":"ignored","$schema":"https://example.test/ignored"}}}"##,
            ),
        ]);
        let requests = Potential::discover(&supplied, 10_000, 1024 * 1024, &|| false).unwrap();
        assert_eq!(
            requests,
            BTreeSet::from([META.into(), "https://example.test/other-meta".into()])
        );
    }

    #[test]
    fn potential_work_is_bounded_and_failed_scans_do_not_mutate_inputs() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let supplied = resources(&[(META, r#"{"$defs":{"a":true,"b":true,"c":true}}"#)]);
        let profiles = profiles();
        let failure = Potential::scan(&supplied, &profiles, 2, 1024 * 1024, &|| false)
            .err()
            .unwrap();
        assert_eq!(failure.reason, FailureReason::LimitExceeded);
        let failure = Potential::scan(&supplied, &profiles, 100, 0, &|| false)
            .err()
            .unwrap();
        assert_eq!(failure.reason, FailureReason::LimitExceeded);
        let checks = AtomicUsize::new(0);
        let failure = Potential::scan(&supplied, &profiles, 100, 1024 * 1024, &|| {
            checks.fetch_add(1, Ordering::Relaxed) >= 4
        })
        .err()
        .unwrap();
        assert_eq!(failure.reason, FailureReason::Cancelled);
        assert_eq!(supplied.policy.names[META].len(), 1);
    }
}

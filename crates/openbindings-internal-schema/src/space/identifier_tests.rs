use super::*;
use crate::input_policy::{canonical_uri, identifier_resolution_tests::uri_corpus};

// The selection rule before the direct registered-key lookup.
fn canonicalizing_lookup(space: &SchemaSpace, absolute: &str) -> Result<usize, QueryFailure> {
    if !uri::absolute(absolute) {
        return Err(QueryFailure::MalformedReference);
    }
    let name = absolute.split_once('#').map_or(absolute, |(name, _)| name);
    let exact = space
        .name(name)
        .filter(|selection| selection.exact_contained);
    let selected = exact.or_else(|| canonical_uri(name).and_then(|name| space.name(&name)));
    space
        .selected_resource(selected)?
        .ok_or_else(|| QueryFailure::ResourceUnavailable(name.into()))
}

#[test]
fn registered_key_lookup_preserves_canonicalizing_selection() {
    let document = RawDocument::parse(
        r#"{"schemas":{
            "spelled":{"$id":"HTTPS://EXAMPLE.test:443/%7e/a"},
            "canonical":{"$id":"https://example.test/~/a"},
            "duplicate1":{"$id":"https://conflict.test/id"},
            "duplicate2":{"$id":"https://conflict.test/id"}
        }}"#,
    )
    .unwrap();
    let supplied = Supplied::new([
        ProjectedResource {
            uri: "HTTP://SUPPLIED.test:80/%7e/retrieval#".into(),
            document: JsonValue::parse(
                r#"{"$id":"https://supplied.test/declared","$defs":{"child":{"$id":"child"}}}"#,
            )
            .unwrap(),
        },
        ProjectedResource {
            uri: DIALECT.into(),
            document: JsonValue::parse("true").unwrap(),
        },
    ])
    .unwrap();
    let mut space = SchemaSpace::new(document, supplied).unwrap();
    let spelled = space
        .named_resource("HTTPS://EXAMPLE.test:443/%7e/a")
        .unwrap();
    let canonical = space.named_resource("https://example.test/~/a").unwrap();
    assert_ne!(
        spelled, canonical,
        "exact contained spellings retain priority"
    );
    assert_eq!(
        space.named_resource("http://supplied.test/~/retrieval"),
        space.named_resource("https://supplied.test/declared")
    );
    assert!(space.named_resource("https://supplied.test/child").is_ok());
    assert_eq!(
        space.named_resource("https://conflict.test/id"),
        Err(QueryFailure::IdentityConflict)
    );
    assert_eq!(
        space.named_resource(DIALECT),
        Err(QueryFailure::IdentityConflict)
    );

    let mut queries = uri_corpus();
    queries.extend(
        space
            .local
            .names
            .keys()
            .map(String::as_str)
            .chain(space.packaged.names())
            .flat_map(|name| {
                [
                    name.to_owned(),
                    format!("{name}#"),
                    format!("{name}#anchor"),
                ]
            }),
    );
    queries.extend([
        "https://EXAMPLE.test/%7E/a".into(),
        "HTTP://SUPPLIED.test:80/%7e/retrieval#".into(),
        "https://supplied.test/a/../declared".into(),
    ]);
    for (uncertain, unknown) in [(false, false), (true, false), (false, true), (true, true)] {
        space.local.uncertain_identifiers = uncertain;
        space.custom.unknown_namespace = unknown;
        for name in &queries {
            assert_eq!(
                space.named_resource(name),
                canonicalizing_lookup(&space, name),
                "{name}, uncertain={uncertain}, unknown={unknown}"
            );
        }
    }
}

fn layer(base: &str, origin: ResourceOrigin) -> Layer {
    let mut layer = Layer::default();
    layer.resources.push(Resource {
        root: Some(0),
        base: Some(base.into()),
        anonymous: false,
        dialect: Some(DIALECT.into()),
        profile: None,
        anchors: BTreeMap::new(),
        origin,
        eligible: true,
        fragment_base: !base.contains('#'),
    });
    layer
}

fn associations(layer: &Layer) -> Vec<(&str, ResourceOrigin, Option<usize>, bool)> {
    layer
        .names
        .iter()
        .map(|(name, selection)| {
            (
                name.as_str(),
                selection.origin,
                selection.carrier,
                selection.exact_contained,
            )
        })
        .collect()
}

#[test]
fn registration_paths_preserve_canonicalizing_associations_and_eligibility() {
    for raw in uri_corpus() {
        let Some(resolved) = uri::compared_id(None, &raw) else {
            continue;
        };
        let canonical = canonical_uri(&resolved).unwrap();
        for origin in [
            ResourceOrigin::Contained,
            ResourceOrigin::Supplied,
            ResourceOrigin::Standard,
        ] {
            for protected in [BTreeSet::new(), BTreeSet::from([canonical.clone()])] {
                let mut before = layer(&resolved, origin);
                let mut after = layer(&resolved, origin);
                before.add_name(resolved.clone(), 0, &protected);
                after.add_resolved_name(resolved.clone(), 0, &protected);
                assert_eq!(associations(&before), associations(&after), "{raw}");
                let association = if origin == ResourceOrigin::Contained {
                    &resolved
                } else {
                    &canonical
                };
                let eligible = before.names.get(association).and_then(|s| s.carrier) == Some(0);
                after.finalize(|names, name| names.get(name).copied());
                assert_eq!(eligible, after.resources[0].eligible, "{raw}");

                let mut before = layer(&resolved, origin);
                let mut after = layer(&resolved, origin);
                before.add_name(canonical.clone(), 0, &protected);
                after.add_canonical_name(canonical.clone(), 0, &protected);
                assert_eq!(associations(&before), associations(&after), "{raw}");
            }
        }
    }
}

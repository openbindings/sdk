use openbindings::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn schema(text: &str) -> Schema {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .schema("entry")
        .unwrap()
}
fn environment(schema: &Schema, external: &ExternalSchemas) -> SchemaEnvironment {
    schema
        .environment(
            external,
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
}
fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

#[test]
fn preparation_attempts_bound_work_and_allocations_without_poisoning_caches() {
    let selected = schema(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{}}}"#);
    let external = ExternalSchemas::default();
    let control = AtomicBool::new(false);
    for (limits, resource) in [
        (
            PreparationLimits {
                max_sdk_work_units: Some(0),
                ..PreparationLimits::default()
            },
            "preparation-sdk-work-units",
        ),
        (
            PreparationLimits {
                max_sdk_reservation_bytes: 0,
                ..PreparationLimits::default()
            },
            "preparation-sdk-reservation-bytes",
        ),
    ] {
        assert_eq!(
            selected
                .environment(&external, &limits, &control)
                .unwrap_err()
                .kind,
            PreparationFailureKind::Limit {
                resource: resource.into()
            }
        );
    }
    let held = environment(&selected, &external);
    // Completed immutable inventory is shared rather than charged again as a
    // new allocation. The returned environment handle still reserves its cost.
    let warm = PreparationLimits {
        max_sdk_work_units: Some(100),
        max_sdk_reservation_bytes: 256,
        ..PreparationLimits::default()
    };
    assert!(selected.environment(&external, &warm, &control).is_ok());
    let stopped = PreparationLimits {
        max_sdk_work_units: Some(0),
        ..warm
    };
    assert_eq!(
        selected
            .environment(&external, &stopped, &control)
            .unwrap_err()
            .kind,
        PreparationFailureKind::Limit {
            resource: "preparation-sdk-work-units".into()
        }
    );
    let small = PreparationLimits {
        max_schema_nodes: 0,
        ..warm
    };
    assert_eq!(
        selected
            .environment(&external, &small, &control)
            .unwrap_err()
            .kind,
        PreparationFailureKind::Limit {
            resource: "schema-nodes".into()
        }
    );
    control.store(true, Ordering::Release);
    assert_eq!(
        selected
            .environment(&external, &warm, &control)
            .unwrap_err()
            .kind,
        PreparationFailureKind::Cancelled
    );
    assert_eq!(held.root().value().kind(), JsonKind::Object);
}

#[test]
fn supplied_preparations_keep_each_attempts_inventory_limits_and_snapshot() {
    let resources = || {
        ExternalSchemas::new([
            ExternalSchema {
                uri: "urn:root".into(),
                document: value(r#"{"$defs":{"child":true}}"#),
            },
            ExternalSchema {
                uri: "urn:other".into(),
                document: value("false"),
            },
        ])
        .unwrap()
    };
    let external = resources();
    let control = AtomicBool::new(false);
    let request = EvaluationRequest {
        root: SchemaRoot::Supplied("urn:root"),
        external_schemas: &external,
    };
    let held = request
        .environment(&PreparationLimits::default(), &control)
        .unwrap();
    for (limits, resource) in [
        (
            PreparationLimits {
                max_schema_nodes: 1,
                ..PreparationLimits::default()
            },
            "schema-nodes",
        ),
        (
            PreparationLimits {
                max_identity_bytes: "urn:root".len(),
                ..PreparationLimits::default()
            },
            "schema-identity-bytes",
        ),
    ] {
        for set in [&external, &resources()] {
            let request = EvaluationRequest {
                external_schemas: set,
                ..request
            };
            assert_eq!(
                request.environment(&limits, &control).unwrap_err().kind,
                PreparationFailureKind::Limit {
                    resource: resource.into()
                }
            );
        }
    }
    let second = EvaluationRequest {
        root: SchemaRoot::Supplied("urn:other"),
        ..request
    }
    .environment(&PreparationLimits::default(), &control)
    .unwrap();
    assert_eq!(second.root().value().text(), "false");
    assert_eq!(
        request
            .environment(&PreparationLimits::default(), &control)
            .unwrap()
            .root()
            .value()
            .text(),
        held.root().value().text()
    );
    let other = ExternalSchemas::new([ExternalSchema {
        uri: "urn:root".into(),
        document: value("true"),
    }])
    .unwrap();
    assert_eq!(
        EvaluationRequest {
            external_schemas: &other,
            ..request
        }
        .environment(&PreparationLimits::default(), &control)
        .unwrap()
        .root()
        .value()
        .text(),
        "true"
    );
    drop(external);
    assert_eq!(held.root().value().text(), r#"{"$defs":{"child":true}}"#);
}

#[test]
fn a_failed_attempt_does_not_materialize_an_unbounded_origin() {
    let name = "n".repeat(100_000);
    let text =
        format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"{name}":{{}}}}}}"#);
    let schema = Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .schema(&name)
        .unwrap();
    let failure = schema
        .environment(
            &ExternalSchemas::default(),
            &PreparationLimits {
                max_sdk_reservation_bytes: 0,
                ..PreparationLimits::default()
            },
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(
        failure.kind,
        PreparationFailureKind::Limit {
            resource: "preparation-sdk-reservation-bytes".into()
        }
    );
    assert!(failure.origin.is_none());
}

#[test]
fn original_sources_bases_and_resource_roots_are_separate() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{},"local":{"$id":"https://id.test/local","$defs":{"number":{"type":"number"}}}}}"##,
    );
    let supplied = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://fetch.test/bundle").into(),
        document: value(
            r##"{"$defs":{"N":{"$id":"https://id.test/n","$defs":{"flag":{"type":"boolean"}}}}}"##,
        ),
    }])
    .unwrap();
    let env = environment(&selected, &supplied);
    let root = env.root();
    assert_eq!(root.resource_base(), ResourceBase::AnonymousDocument);
    assert!(root.resource_root().is_none());
    assert_eq!(root.origin().source_uri, None);
    assert_eq!(root.pointer().as_str(), Some("/schemas/entry"));
    let local = env
        .resolve(&root, "https://id.test/local#/$defs/number")
        .unwrap()
        .schema;
    assert_eq!(local.origin().source_uri, None);
    assert_eq!(
        local.pointer().as_str(),
        Some("/schemas/local/$defs/number")
    );
    assert_eq!(
        local.resource_base(),
        ResourceBase::Named("https://id.test/local")
    );
    assert_eq!(
        local.resource_root().unwrap().pointer().as_str(),
        Some("/schemas/local")
    );
    let nested = env
        .resolve(&root, "https://id.test/n#/$defs/flag")
        .unwrap()
        .schema;
    assert_eq!(
        nested.origin().source_uri.as_deref(),
        Some("https://fetch.test/bundle")
    );
    assert_eq!(nested.pointer().as_str(), Some("/$defs/N/$defs/flag"));
    assert_eq!(
        nested.resource_base(),
        ResourceBase::Named("https://id.test/n")
    );
    assert_eq!(
        nested.resource_root().unwrap().pointer().as_str(),
        Some("/$defs/N")
    );
    assert_eq!(
        nested.source().text(),
        supplied.iter().next().unwrap().document.text()
    );
    assert_eq!(
        nested.dialect(),
        Some("https://json-schema.org/draft/2020-12/schema")
    );
    drop((env, root, selected, supplied));
    assert_eq!(
        nested.value().get("type").unwrap().as_str(),
        Some("boolean")
    );
}

#[test]
fn dynamic_lookup_tracks_declaration_and_actual_resource_identity() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$id":"https://id.test/root","$defs":{"plain":{"$anchor":"plain"},"dyn":{"$dynamicAnchor":"dyn"},"named":{"$id":"https://id.test/n","$dynamicAnchor":"dyn"}}}}}"##,
    );
    let env = environment(&selected, &ExternalSchemas::default());
    let root = env.root();
    let named = env.resolve(&root, "#dyn").unwrap();
    assert_eq!(named.dynamic_anchor.as_deref(), Some("dyn"));
    let pointer = env.resolve(&root, "#/$defs/dyn").unwrap();
    assert_eq!(pointer.dynamic_anchor, None);
    assert_eq!(pointer.schema.resource(), root.resource());
    assert!(
        env.dynamic_anchor(&root.resource(), "plain")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        env.dynamic_anchor(&root.resource(), "dyn")
            .unwrap()
            .unwrap()
            .pointer(),
        named.schema.pointer()
    );
    let other = env.resolve(&root, "https://id.test/n#dyn").unwrap();
    assert_ne!(other.schema.resource(), root.resource());
    assert_eq!(
        other.schema.pointer().as_str(),
        Some("/schemas/entry/$defs/named")
    );
    let second = environment(&selected, &ExternalSchemas::default());
    let misuse = second.resolve(&root, "#dyn").unwrap_err();
    assert_eq!(misuse.kind, ResolutionFailureKind::WrongEnvironment);
    assert_eq!(misuse.origin, None);
    assert_eq!(
        second
            .dynamic_anchor(&root.resource(), "dyn")
            .unwrap_err()
            .kind,
        ResolutionFailureKind::WrongEnvironment
    );
    assert_eq!(env.clone().root().resource(), root.resource());
}

#[test]
fn checked_subschemas_never_promote_maps_literals_or_opaque_objects() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"properties":{"a/b~c":{"type":"string"}},"const":{"literal":{}},"vendor":{"unknown":{}}}}}"##,
    );
    let env = environment(&selected, &ExternalSchemas::default());
    let root = env.root();
    assert_eq!(
        root.subschema(&["properties", "a/b~c"])
            .unwrap()
            .pointer()
            .as_str(),
        Some("/schemas/entry/properties/a~1b~0c")
    );
    for path in [
        &["properties"][..],
        &["const"],
        &["const", "literal"],
        &["vendor"],
        &["vendor", "unknown"],
        &["absent"],
    ] {
        assert_eq!(
            root.subschema(path).unwrap_err().kind,
            ResolutionFailureKind::NotSchemaLocation
        );
    }
    assert_eq!(root.subschema(&[]).unwrap().pointer(), root.pointer());
    let external = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://example.test/unpaired").into(),
        document: value(r#"{"$defs":{"\ud800":{"type":"string"}}}"#),
    }])
    .unwrap();
    let env = environment(&selected, &external);
    let external = env
        .resolve(&env.root(), "https://example.test/unpaired")
        .unwrap()
        .schema;
    let mut units: Vec<_> = "/$defs/".encode_utf16().collect();
    units.push(0xd800);
    let pointer = Pointer::from_utf16(&units).unwrap();
    let child = external.subschema_at(&pointer).unwrap();
    assert_eq!(child.pointer(), &pointer);
    assert_eq!(
        child.origin().source_uri.as_deref(),
        Some("https://example.test/unpaired")
    );
}

#[test]
fn defects_are_distinct_from_unavailable_content_and_unknown_dialects() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$id":"https://id.test/n","properties":{"x":false},"const":{"literal":{}}}}}"##,
    );
    let env = environment(&selected, &ExternalSchemas::default());
    let root = env.root();
    for (query, defect) in [
        ("#missing", ReferenceDefect::MissingAnchor),
        ("#/absent", ReferenceDefect::MissingTarget),
        ("#/properties", ReferenceDefect::NonSchemaTarget),
        ("#/const/literal", ReferenceDefect::NonSchemaTarget),
        ("#/bad~2escape", ReferenceDefect::MalformedReference),
        ("#%FF", ReferenceDefect::MalformedReference),
        ("#%", ReferenceDefect::MalformedReference),
    ] {
        let failure = env.resolve(&root, query).unwrap_err();
        assert_eq!(
            failure.kind,
            ResolutionFailureKind::Undefined(defect),
            "{query}"
        );
        assert_eq!(failure.origin.as_ref(), Some(root.origin()));
    }
    assert!(matches!(
        env.resolve(&root, "https://missing.test/schema")
            .unwrap_err()
            .kind,
        ResolutionFailureKind::ResourceUnavailable { .. }
    ));
    assert_eq!(
        env.resolve(&root, "#/properties/x")
            .unwrap()
            .schema
            .value()
            .text(),
        "false"
    );
}

#[test]
fn unknown_required_vocabularies_do_not_certify_external_name_absence() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://missing.test/unvisited"}}}"##,
    );
    let external=ExternalSchemas::new([openbindings::ExternalSchema { uri: ("https://fetch.test/unknown").into(), document: value(r##"{"$id":"https://id.test/unknown","$schema":"https://dialect.test/custom","$defs":{"hidden":{"$id":"https://id.test/hidden","$dynamicAnchor":"h"}}}"##) }, openbindings::ExternalSchema { uri: ("https://dialect.test/custom").into(), document: value(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://vocab.test/custom":true}}"##) }, openbindings::ExternalSchema { uri: ("https://fetch.test/known").into(), document: value(r##"{"$defs":{"nested":{"$id":"https://id.test/nested","$schema":"https://dialect.test/custom","$defs":{"child":true}}}}"##) }]).unwrap();
    let env = environment(&selected, &external);
    // The original input records remain observable without claiming that an
    // unrecognized vocabulary cannot introduce competing resource identities.
    let original = external
        .iter()
        .find(|r| r.uri == "https://fetch.test/unknown")
        .unwrap();
    assert_eq!(
        original.document.get("$schema").unwrap().as_str(),
        Some("https://dialect.test/custom")
    );
    assert!(original.document.get("$defs").is_some());
    for query in [
        "https://id.test/unknown",
        "https://dialect.test/custom",
        "https://fetch.test/unknown#/$defs/hidden",
        "https://fetch.test/known#/$defs/nested",
        "https://fetch.test/known#/$defs/nested/$defs/child",
        "https://id.test/hidden",
    ] {
        assert_eq!(
            env.resolve(&env.root(), query).unwrap_err().kind,
            ResolutionFailureKind::UnsupportedCapability {
                capability: "custom-profile-namespace".into()
            }
        );
    }
}

#[test]
fn certificate_known_unevaluated_dialect_retains_original_root_metadata() {
    let selected = schema(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":true}}"#);
    let external = ExternalSchemas::new([openbindings::ExternalSchema { uri: ("https://fetch.test/old").into(), document: value(r#"{"$schema":"http://json-schema.org/draft-06/schema#","definitions":{"hidden":true}}"#) }]).unwrap();
    let env = environment(&selected, &external);
    let old = env
        .resolve(&env.root(), "https://fetch.test/old")
        .unwrap()
        .schema;
    assert_eq!(
        old.dialect(),
        Some("http://json-schema.org/draft-06/schema#")
    );
    assert!(old.source().get("definitions").is_some());
    assert!(matches!(
        env.resolve(&old, "#/definitions/hidden").unwrap_err().kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
    assert!(matches!(
        old.subschema(&["definitions", "hidden"]).unwrap_err().kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
    assert!(matches!(
        env.dynamic_anchor(&old.resource(), "h").unwrap_err().kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
}

#[test]
fn exact_contained_identifiers_outrank_normalized_aliases_for_general_queries() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{},"upper":{"$id":"https://EXAMPLE.test:443/A","type":"string"},"lower":{"$id":"https://example.test/A","type":"number"}}}"##,
    );
    let env = environment(&selected, &ExternalSchemas::default());
    for (uri, expected) in [
        ("https://EXAMPLE.test:443/A", "string"),
        ("https://example.test/A", "number"),
    ] {
        assert_eq!(
            env.resolve(&env.root(), uri)
                .unwrap()
                .schema
                .value()
                .get("type")
                .unwrap()
                .as_str(),
            Some(expected)
        );
    }
}

struct CancelAfter(AtomicUsize);
impl WorkControl for CancelAfter {
    fn is_cancelled(&self) -> bool {
        let mut remaining = self.0.load(Ordering::SeqCst);
        loop {
            if remaining == 0 {
                return true;
            }
            match self.0.compare_exchange(
                remaining,
                remaining - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return false,
                Err(current) => remaining = current,
            }
        }
    }
}
#[test]
fn limits_and_cancellation_leave_retriable_immutable_inputs() {
    let selected = schema(r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{}}}"##);
    let mut fields = String::from("{\"$defs\":{");
    for n in 0..500 {
        if n > 0 {
            fields.push(',')
        }
        fields.push_str(&format!("\"n{n}\":true"));
    }
    fields.push_str("}}");
    let external = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://example.test/large").into(),
        document: value(&fields),
    }])
    .unwrap();
    let false_control = AtomicBool::new(false);
    assert!(matches!(
        selected
            .environment(
                &external,
                &PreparationLimits {
                    max_schema_nodes: 1,
                    ..PreparationLimits::default()
                },
                &false_control
            )
            .unwrap_err()
            .kind,
        PreparationFailureKind::Limit { .. }
    ));
    assert_eq!(
        selected
            .environment(
                &external,
                &PreparationLimits::default(),
                &CancelAfter(AtomicUsize::new(30))
            )
            .unwrap_err()
            .kind,
        PreparationFailureKind::Cancelled
    );
    let env = environment(&selected, &external);
    let root = env.root();
    assert_eq!(
        env.resolve_with_control(&root, "https://example.test/large", &AtomicBool::new(true))
            .unwrap_err()
            .kind,
        ResolutionFailureKind::Cancelled
    );
    assert!(
        env.resolve(&root, "https://example.test/large#/$defs/n499")
            .is_ok()
    );
    assert!(matches!(
        selected
            .environment(
                &external,
                &PreparationLimits {
                    max_schema_nodes: 1,
                    ..PreparationLimits::default()
                },
                &false_control
            )
            .unwrap_err()
            .kind,
        PreparationFailureKind::Limit { .. }
    ));
}

fn nested_identifiers(count: usize, segment_bytes: usize) -> String {
    let segment = format!("{}/", "x".repeat(segment_bytes));
    let mut schema = "true".to_owned();
    for _ in 0..count {
        schema = format!(r#"{{"$id":"{segment}","$defs":{{"child":{schema}}}}}"#);
    }
    format!(r#"{{"$id":"https://identity.test/","$defs":{{"child":{schema}}}}}"#)
}

#[test]
fn expanded_identifiers_are_bounded_before_environment_and_registration_indexes() {
    let oversized = nested_identifiers(400, 500);
    let selected = schema(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{oversized}}}}}"#
    ));
    let error = selected
        .environment(
            &ExternalSchemas::default(),
            &PreparationLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        PreparationFailureKind::Limit {
            resource: "schema-identity-bytes".into()
        }
    );
    let error = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://identity.test/supplied").into(),
        document: value(&oversized),
    }])
    .unwrap_err();
    assert_eq!(
        error.kind,
        ExternalSchemaErrorKind::Limit {
            resource: "schema-identity-bytes".into()
        }
    );

    let small = nested_identifiers(40, 20);
    let selected = schema(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{small}}}}}"#
    ));
    let external = ExternalSchemas::default();
    let held = environment(&selected, &external);
    let tight = PreparationLimits {
        max_identity_bytes: 100,
        ..PreparationLimits::default()
    };
    assert_eq!(
        selected
            .environment(&external, &tight, &AtomicBool::new(false))
            .unwrap_err()
            .kind,
        PreparationFailureKind::Limit {
            resource: "schema-identity-bytes".into()
        }
    );
    assert_eq!(
        environment(&selected, &external).root().value().text(),
        held.root().value().text()
    );
    let input = ExternalSchema {
        uri: "https://identity.test/supplied".into(),
        document: value(&small),
    };
    assert!(matches!(
        ExternalSchemas::new_with_limits([input.clone()], &tight, &AtomicBool::new(false))
            .unwrap_err()
            .kind,
        ExternalSchemaErrorKind::Limit { .. }
    ));
    assert_eq!(
        ExternalSchemas::new_with_limits(
            [input.clone()],
            &PreparationLimits::default(),
            &AtomicBool::new(true)
        )
        .unwrap_err()
        .kind,
        ExternalSchemaErrorKind::Cancelled
    );
    assert!(ExternalSchemas::new([input]).is_ok());
}

#[test]
fn registration_stops_consuming_inputs_at_the_first_budget_overrun() {
    let consumed = std::cell::Cell::new(0usize);
    let inputs = (0..1000).map(|n| {
        consumed.set(consumed.get() + 1);
        ExternalSchema {
            uri: format!("https://registration.test/{n}/{}", "x".repeat(4096)),
            document: JsonValue::boolean(true),
        }
    });
    let tight = PreparationLimits {
        max_identity_bytes: 1024,
        ..PreparationLimits::default()
    };
    assert!(matches!(
        ExternalSchemas::new_with_limits(inputs, &tight, &AtomicBool::new(false))
            .unwrap_err()
            .kind,
        ExternalSchemaErrorKind::Limit { .. }
    ));
    assert_eq!(consumed.get(), 1);
    consumed.set(0);
    let inputs = (0..1000).map(|n| {
        consumed.set(consumed.get() + 1);
        ExternalSchema {
            uri: format!("https://registration.test/{n}"),
            document: JsonValue::boolean(true),
        }
    });
    let tight = PreparationLimits {
        max_schema_nodes: 1,
        ..PreparationLimits::default()
    };
    assert!(matches!(
        ExternalSchemas::new_with_limits(inputs, &tight, &AtomicBool::new(false))
            .unwrap_err()
            .kind,
        ExternalSchemaErrorKind::Limit { .. }
    ));
    assert_eq!(consumed.get(), 2);
}

#[test]
fn document_and_external_schema_locations_use_the_original_source_coordinates() {
    let text = "{\n\"openbindings\":\"0.2.0\",\n\"operations\":{},\n\"schemas\":{\"entry\":{\"type\":\"integer\"}}\n}";
    let selected = schema(text);
    let origin = selected.location();
    let expected = value(text).at("/schemas/entry").unwrap().location();
    assert_eq!(selected.document().locate(&origin), Some(expected));
    let carrier = value("{\n\"outside\":0,\n\"schema\":{\n \"properties\":{\"field\":false}\n}}");
    let subtree = carrier.at("/schema").unwrap().to_owned();
    let external = ExternalSchemas::new([openbindings::ExternalSchema {
        uri: ("https://example.test/schema").into(),
        document: subtree.clone(),
    }])
    .unwrap();
    let env = environment(&selected, &external);
    let node = env
        .resolve(&env.root(), "https://example.test/schema#/properties/field")
        .unwrap()
        .schema;
    let expected = value(subtree.text())
        .at("/properties/field")
        .unwrap()
        .location();
    assert_eq!(external.locate(node.origin()), Some(expected));
    assert!(external.locate(&origin).is_none());
    assert!(selected.document().locate(node.origin()).is_none());
}

#[test]
fn cached_schema_inventory_limits_accept_the_boundary_and_refuse_one_less() {
    let text = r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$id":"https://example.test/root","$defs":{"child":true}}}}"#;
    for supplied in [false, true] {
        let external = if supplied {
            ExternalSchemas::new([openbindings::ExternalSchema {
                uri: ("https://example.test/external").into(),
                document: value("true"),
            }])
            .unwrap()
        } else {
            ExternalSchemas::default()
        };
        let selected = schema(text);
        let held = environment(&selected, &external);
        for identities in [false, true] {
            let resource = if identities {
                "schema-identity-bytes"
            } else {
                "schema-nodes"
            };
            let limit = |maximum| PreparationLimits {
                max_schema_nodes: if identities {
                    PreparationLimits::default().max_schema_nodes
                } else {
                    maximum
                },
                max_identity_bytes: if identities {
                    maximum
                } else {
                    PreparationLimits::default().max_identity_bytes
                },
                ..PreparationLimits::default()
            };
            // Find the accepted boundary on fresh documents, then require the
            // retained environment to honor that same boundary on reuse.
            let (mut low, mut high) = (0, 100_000);
            while low < high {
                let mid = low + (high - low) / 2;
                match schema(text).environment(&external, &limit(mid), &AtomicBool::new(false)) {
                    Ok(_) => high = mid,
                    Err(failure) => {
                        assert_eq!(
                            failure.kind,
                            PreparationFailureKind::Limit {
                                resource: resource.into()
                            }
                        );
                        assert!(failure.origin.is_none());
                        low = mid + 1;
                    }
                }
            }
            assert!(low > 0);
            for maximum in [low, low + 1] {
                selected
                    .environment(&external, &limit(maximum), &AtomicBool::new(false))
                    .unwrap();
            }
            let failure = selected
                .environment(&external, &limit(low - 1), &AtomicBool::new(false))
                .unwrap_err();
            assert_eq!(
                failure.kind,
                PreparationFailureKind::Limit {
                    resource: resource.into()
                }
            );
            assert!(failure.origin.is_none());
            assert_eq!(held.root().value().text(), selected.json().text());
        }
    }
}

#[test]
fn external_registration_accepts_empty_fragments_and_preserves_subtree_bytes() {
    for uri in [
        "https://example.test/schema",
        "https://example.test/schema#",
    ] {
        let external = ExternalSchemas::new([openbindings::ExternalSchema {
            uri: (uri).into(),
            document: value("true"),
        }])
        .unwrap();
        assert_eq!(
            external.iter().next().unwrap().uri,
            "https://example.test/schema"
        );
        assert_eq!(external.iter().next().unwrap().document.text(), "true");
    }
    for uri in ["https://example.test/schema#fragment", "relative"] {
        assert_eq!(
            ExternalSchemas::new([openbindings::ExternalSchema {
                uri: (uri).into(),
                document: value("true")
            }])
            .unwrap_err()
            .kind,
            ExternalSchemaErrorKind::InvalidIdentity
        );
    }
    // Root-owned text is already admitted. A generous but finite allowance can
    // register it without funding another input-sized copy.
    let body = format!("{{\"const\":\"{}\"}}", "x".repeat(20_000));
    let limits = PreparationLimits {
        max_sdk_work_units: Some(10_000),
        max_sdk_reservation_bytes: 100_000,
        ..PreparationLimits::default()
    };
    let root = ExternalSchema {
        uri: "https://example.test/schema".into(),
        document: value(&body),
    };
    let registered =
        ExternalSchemas::new_with_limits([root], &limits, &AtomicBool::new(false)).unwrap();
    assert_eq!(registered.iter().next().unwrap().document.text(), body);
    let carrier = value(&format!("{{\"schema\":{body}}}"));
    let subtree = ExternalSchema {
        uri: "https://example.test/schema".into(),
        document: carrier.at("/schema").unwrap().to_owned(),
    };
    let failure =
        ExternalSchemas::new_with_limits([subtree.clone()], &limits, &AtomicBool::new(false))
            .unwrap_err();
    assert!(matches!(
        failure.kind,
        ExternalSchemaErrorKind::Limit { .. }
    ));
    let registered = ExternalSchemas::new([subtree]).unwrap();
    assert_eq!(registered.iter().next().unwrap().document.text(), body);
}

#[test]
fn resolution_and_cancellation_display_describe_the_failed_operation() {
    let selected = schema(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":true}}"#);
    let env = environment(&selected, &ExternalSchemas::default());
    let failure = env
        .resolve(&env.root(), "https://private.test/missing")
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "schema reference resource is unavailable; supply the external schema"
    );
    assert_eq!(Cancelled.to_string(), "the caller cancelled this work");
}

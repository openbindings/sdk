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
fn original_sources_bases_and_resource_roots_are_separate() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{},"local":{"$id":"https://id.test/local","$defs":{"number":{"type":"number"}}}}}"##,
    );
    let supplied = ExternalSchemas::default().with("https://fetch.test/bundle", value(r##"{"$defs":{"N":{"$id":"https://id.test/n","$defs":{"flag":{"type":"boolean"}}}}}"##)).unwrap();
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
    let external = ExternalSchemas::default()
        .with(
            "https://example.test/unpaired",
            value(r#"{"$defs":{"\ud800":{"type":"string"}}}"#),
        )
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
fn unknown_dialects_are_local_and_metadata_is_reachable() {
    let selected = schema(
        r##"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://missing.test/unvisited"}}}"##,
    );
    let external=ExternalSchemas::default()
        .with("https://fetch.test/unknown",value(r##"{"$id":"https://id.test/unknown","$schema":"https://dialect.test/custom","$defs":{"hidden":{"$id":"https://id.test/hidden","$dynamicAnchor":"h"}}}"##)).unwrap()
        .with("https://dialect.test/custom",value(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://vocab.test/custom":true}}"##)).unwrap()
        .with("https://fetch.test/known",value(r##"{"$defs":{"nested":{"$id":"https://id.test/nested","$schema":"https://dialect.test/custom","$defs":{"child":true}}}}"##)).unwrap();
    let env = environment(&selected, &external);
    let unknown = env
        .resolve(&env.root(), "https://id.test/unknown")
        .unwrap()
        .schema;
    assert_eq!(unknown.dialect(), Some("https://dialect.test/custom"));
    assert!(unknown.source().get("$defs").is_some());
    let meta = env
        .resolve(&unknown, unknown.dialect().unwrap())
        .unwrap()
        .schema;
    assert!(meta.value().get("$vocabulary").is_some());
    for query in ["#/$defs/hidden", "#h"] {
        assert!(matches!(
            env.resolve(&unknown, query).unwrap_err().kind,
            ResolutionFailureKind::UnsupportedDialect { .. }
        ));
    }
    assert!(matches!(
        env.dynamic_anchor(&unknown.resource(), "h")
            .unwrap_err()
            .kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
    assert!(matches!(
        unknown.subschema(&["$defs", "hidden"]).unwrap_err().kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
    let nested = env
        .resolve(&env.root(), "https://fetch.test/known#/$defs/nested")
        .unwrap()
        .schema;
    assert_eq!(nested.dialect(), unknown.dialect());
    assert!(matches!(
        env.resolve(
            &env.root(),
            "https://fetch.test/known#/$defs/nested/$defs/child"
        )
        .unwrap_err()
        .kind,
        ResolutionFailureKind::UnsupportedDialect { .. }
    ));
    assert!(matches!(
        env.resolve(&env.root(), "https://id.test/hidden")
            .unwrap_err()
            .kind,
        ResolutionFailureKind::ResourceUnavailable { .. }
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
    let external = ExternalSchemas::default()
        .with("https://example.test/large", value(&fields))
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
    let error = ExternalSchemas::default()
        .with("https://identity.test/supplied", value(&oversized))
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

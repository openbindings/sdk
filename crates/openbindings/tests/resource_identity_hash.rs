use openbindings::{Document, ExternalSchemas, PreparationLimits, ResourceIdentity};
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::atomic::AtomicBool,
};

fn hash(identity: &ResourceIdentity) -> u64 {
    let mut state = DefaultHasher::new();
    identity.hash(&mut state);
    state.finish()
}

#[test]
#[allow(
    clippy::mutable_key_type,
    reason = "identity hashes only the stable environment address and resource index"
)]
fn hash_keys_follow_environment_and_resource_equality_not_uri_spelling() {
    let proof = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"S":{"$id":"https://example.test/root","$defs":{"child":{"$id":"https://example.test/child"}}}}}"#,
    ).unwrap().validate().unwrap().conformant().unwrap();
    let schema = proof.schema("S").unwrap();
    let make = || {
        schema
            .environment(
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap()
    };
    let environment = make();
    let root = environment.root().resource();
    let clone = environment.clone().root().resource();
    let resolved = environment
        .resolve(&environment.root(), "https://example.test/root")
        .unwrap()
        .schema
        .resource();
    assert_eq!(root, clone);
    assert_eq!(root, resolved);
    assert_eq!(hash(&root), hash(&clone));
    assert_eq!(hash(&root), hash(&resolved));

    let same_uri_other_environment = make().root().resource();
    assert_ne!(root, same_uri_other_environment);
    let child = environment
        .resolve(&environment.root(), "https://example.test/child")
        .unwrap()
        .schema
        .resource();
    assert_ne!(root, child);
    let mut seen = HashSet::new();
    assert!(seen.insert(root.clone()));
    assert!(!seen.insert(clone.clone()));
    assert!(!seen.insert(resolved));
    assert!(seen.insert(child));
    assert!(seen.insert(same_uri_other_environment));
    assert_eq!(seen.len(), 3);
    let cache = HashMap::from([(root, "visited")]);
    drop(environment);
    assert_eq!(cache.get(&clone), Some(&"visited"));
}

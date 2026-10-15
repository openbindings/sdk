// selectors are derived from normative data, not another list.
use super::*;
use crate::schema_index::{SCHEMA_SELECTORS, SchemaSelector, schema_children};
use std::collections::BTreeSet;

fn originals() -> BTreeMap<String, Value> {
    META_FILES
        .iter()
        .map(|text| {
            let value: Value = serde_json::from_str(text).unwrap();
            (value["$id"].as_str().unwrap().to_owned(), value)
        })
        .collect()
}
fn resolve<'a>(
    resources: &'a BTreeMap<String, Value>,
    base: &str,
    reference: &str,
) -> (String, &'a Value) {
    let uri = crate::uri::resolve(base, reference).unwrap();
    let (uri, pointer) = uri.split_once('#').unwrap_or((&uri, ""));
    (
        uri.into(),
        resources
            .get(uri)
            .expect("trusted local resource")
            .pointer(pointer)
            .expect("trusted local pointer"),
    )
}
fn recursive_sites(value: &Value, sites: &mut BTreeSet<usize>) {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$dynamicRef").and_then(Value::as_str) {
                assert_eq!(reference, "#meta", "unrecognized dynamic reference");
                sites.insert(std::ptr::from_ref(value) as usize);
            }
            for value in map.values() {
                recursive_sites(value, sites);
            }
        }
        Value::Array(values) => {
            for value in values {
                recursive_sites(value, sites);
            }
        }
        _ => {}
    }
}
// A trace preserves instance-position selectors while traversing schema references.
// Every recursive site, even one in an unfamiliar schema position, must be visited.
fn traces(
    resources: &BTreeMap<String, Value>,
    uri: &str,
    node: &Value,
    trace: (&[SchemaSelector], bool),
    active: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<usize>,
    out: &mut Vec<(Vec<SchemaSelector>, bool)>,
) {
    let (path, alternative) = trace;
    let Some(map) = node.as_object() else { return };
    let identity = std::ptr::from_ref(node) as usize;
    assert!(
        active.insert(identity),
        "unexpected static meta-schema cycle"
    );
    if map.contains_key("$dynamicRef") {
        assert_eq!(map["$dynamicRef"], "#meta");
        visited.insert(identity);
        out.push((path.to_vec(), alternative));
    }
    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
        let (uri, value) = resolve(resources, uri, reference);
        traces(
            resources,
            &uri,
            value,
            (path, alternative),
            active,
            visited,
            out,
        );
    }
    for union in ["anyOf", "allOf", "oneOf"] {
        if let Some(branches) = map.get(union).and_then(Value::as_array) {
            let legacy_alternative = union == "anyOf"
                && branches.len() == 2
                && branches.iter().any(|v| {
                    v.get("$ref").and_then(Value::as_str).is_some_and(|r| {
                        let (_, v) = resolve(resources, uri, r);
                        v.get("type").and_then(Value::as_str) == Some("array")
                            && v.pointer("/items/type").and_then(Value::as_str) == Some("string")
                            && v.get("uniqueItems").and_then(Value::as_bool) == Some(true)
                    })
                });
            // Recursive alternatives other than the legacy string array require a new proof.
            let before = out.len();
            for child in branches {
                traces(
                    resources,
                    uri,
                    child,
                    (path, alternative || legacy_alternative),
                    active,
                    visited,
                    out,
                );
            }
            if out.len() != before && union != "allOf" {
                assert!(legacy_alternative, "unrecognized recursive alternative");
            }
        }
    }
    for (keyword, selector) in [
        ("items", SchemaSelector::Array),
        ("additionalProperties", SchemaSelector::Map),
    ] {
        if let Some(child) = map.get(keyword) {
            let mut nested = path.to_vec();
            nested.push(selector);
            traces(
                resources,
                uri,
                child,
                (&nested, alternative),
                active,
                visited,
                out,
            );
        }
    }
    active.remove(&identity);
}
fn derived(resources: &BTreeMap<String, Value>) -> BTreeMap<String, SchemaSelector> {
    let mut visited = BTreeSet::new();
    let mut all = BTreeSet::new();
    let mut derived = BTreeMap::new();
    for (uri, resource) in resources {
        recursive_sites(resource, &mut all);
        for (name, schema) in resource["properties"].as_object().unwrap() {
            let mut paths = Vec::new();
            traces(
                resources,
                uri,
                schema,
                (&[], false),
                &mut BTreeSet::new(),
                &mut visited,
                &mut paths,
            );
            if paths.is_empty() {
                continue;
            }
            assert_eq!(paths.len(), 1, "multiple recursive positions for {name}");
            let (path, alternative) = paths.pop().unwrap();
            let selector = match (path.as_slice(), alternative) {
                ([], false) => SchemaSelector::Direct,
                ([SchemaSelector::Map], false) => SchemaSelector::Map,
                ([SchemaSelector::Array], false) => SchemaSelector::Array,
                ([SchemaSelector::Map], true) => SchemaSelector::MapOrStringArray,
                _ => panic!("unrecognized recursive selector: {name} {path:?}"),
            };
            assert!(derived.insert(name.clone(), selector).is_none());
        }
    }
    assert_eq!(visited, all, "unrecognized recursive meta position");
    derived
}
fn equivalent(resources: &BTreeMap<String, Value>, table: &[(&str, SchemaSelector)]) -> bool {
    let count = table.len();
    let table: BTreeMap<_, _> = table
        .iter()
        .map(|(name, selector)| ((*name).into(), *selector))
        .collect();
    assert_eq!(table.len(), count, "duplicate walker selector");
    derived(resources) == table
}
#[test]
fn original_recursive_positions_equal_the_complete_walker_table() {
    let resources = originals();
    assert!(equivalent(&resources, SCHEMA_SELECTORS));
    let mut table = SCHEMA_SELECTORS.to_vec();
    table.pop();
    assert!(
        !equivalent(&resources, &table),
        "deleting an edge must fail"
    );
    table = SCHEMA_SELECTORS.to_vec();
    table.push(("extra", SchemaSelector::Direct));
    assert!(!equivalent(&resources, &table), "an extra edge must fail");
    let mut changed = resources.clone();
    changed.get_mut(crate::schema_index::DIALECT).unwrap()["properties"]["futureSchema"] =
        serde_json::json!({"$dynamicRef":"#meta"});
    assert!(
        !equivalent(&changed, SCHEMA_SELECTORS),
        "adding a recursive position must fail"
    );
    changed.get_mut(crate::schema_index::DIALECT).unwrap()["unrecognized"] =
        serde_json::json!({"$dynamicRef":"#meta"});
    assert!(
        std::panic::catch_unwind(|| derived(&changed)).is_err(),
        "unrecognized positions must fail closed"
    );
}
#[test]
fn actual_walk_matches_derived_shapes_and_occurrences() {
    let resources = originals();
    let selectors = derived(&resources);
    let mut names: BTreeSet<_> = resources
        .values()
        .flat_map(|v| v["properties"].as_object().unwrap().keys().cloned())
        .collect();
    names.extend(["unknown", "default", "const", "examples", "enum", "x-data"].map(str::to_owned));
    for name in names {
        for text in [
            "true",
            "false",
            "null",
            "42",
            r#""s""#,
            "[]",
            "{}",
            r#"{"a":true,"b":{"$id":"https://example.org/r","not":false},"c":42,"d":["a"]}"#,
            r#"[true,{"type":42},42]"#,
        ] {
            let raw = format!("{{{}:{text}}}", serde_json::to_string(&name).unwrap());
            let value = JsonValue::parse(&raw).unwrap();
            let child = value.get(&name).unwrap();
            let is_schema = |v: &JsonRef<'_>| {
                matches!(v.kind(), crate::JsonKind::Object | crate::JsonKind::Boolean)
            };
            let expected: Vec<_> = match selectors.get(&name) {
                Some(SchemaSelector::Direct) => Some(child).into_iter().filter(is_schema).collect(),
                Some(SchemaSelector::Map | SchemaSelector::MapOrStringArray) => child
                    .members()
                    .into_iter()
                    .flatten()
                    .map(|m| m.value)
                    .filter(is_schema)
                    .collect(),
                Some(SchemaSelector::Array) => child
                    .elements()
                    .into_iter()
                    .flatten()
                    .filter(is_schema)
                    .collect(),
                None => vec![],
            };
            let actual = schema_children(value.view());
            assert_eq!(
                actual.iter().map(|v| v.location()).collect::<Vec<_>>(),
                expected.iter().map(|v| v.location()).collect::<Vec<_>>(),
                "{raw}"
            );
        }
    }
    let value = JsonValue::parse(r#"{"allOf":[{},{}]}"#).unwrap();
    let children = schema_children(value.view());
    assert_ne!(occurrence(children[0]), occurrence(children[1]));
}
fn assert_transformed(before: &Value, after: &Value, key: &str) {
    if key == "$id" && before.is_string() {
        assert_eq!(
            after.as_str().unwrap(),
            before.as_str().unwrap().replace(
                "https://json-schema.org/draft/2020-12/",
                "https://openbindings.invalid/internal-meta/"
            )
        );
    } else if let Some(map) = before.as_object() {
        let after = after.as_object().unwrap();
        let recursive = map.get("$dynamicRef").and_then(Value::as_str) == Some("#meta");
        assert_eq!(map.len(), after.len());
        for (key, value) in map {
            if recursive && key == "$dynamicRef" {
                assert!(!after.contains_key(key));
                assert!(
                    !map.contains_key("type"),
                    "localization would overwrite a sibling constraint"
                );
                assert_eq!(after["type"], serde_json::json!(["object", "boolean"]));
            } else {
                assert_transformed(value, &after[key], key);
            }
        }
    } else if let Some(values) = before.as_array() {
        assert_eq!(values.len(), after.as_array().unwrap().len());
        for (a, b) in values.iter().zip(after.as_array().unwrap()) {
            assert_transformed(a, b, "");
        }
    } else {
        assert_eq!(before, after);
    }
}
#[test]
fn localization_changes_only_recursive_edges_and_identities() {
    let original = originals();
    let mut localized = BTreeMap::new();
    for value in original.values() {
        let mut copy = value.clone();
        localize(&mut copy);
        assert_transformed(value, &copy, "");
        localized.insert(copy["$id"].as_str().unwrap().to_owned(), copy);
    }
    for (uri, resource) in &localized {
        let mut stack = vec![resource];
        while let Some(value) = stack.pop() {
            match value {
                Value::Object(map) => {
                    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                        resolve(&localized, uri, reference);
                    }
                    assert!(map.get("$dynamicRef").and_then(Value::as_str).is_none());
                    stack.extend(map.values());
                }
                Value::Array(values) => stack.extend(values),
                _ => {}
            }
        }
    }
    // Explicitly pin preservation of constraints beside a recursive edge.
    let mut sibling = serde_json::json!({"$dynamicRef":"#meta","required":["sentinel"]});
    let original = sibling.clone();
    localize(&mut sibling);
    assert_transformed(&original, &sibling, "");
}
#[test]
fn reviewer_meta_cases_agree_with_original_and_local_checks() {
    let resources = originals();
    let original = compile(&resources[crate::schema_index::DIALECT], resources.clone()).unwrap();
    let cases: Vec<(String, bool)> =
        serde_json::from_str(include_str!("meta-review-cases.json")).unwrap();
    assert_eq!(cases.len(), 42);
    for (text, expected) in cases {
        let value = JsonValue::parse(&text).unwrap();
        assert_eq!(original.is_valid(view(&value)), expected, "original {text}");
        let mut stack = vec![value];
        let mut valid = true;
        while let Some(node) = stack.pop() {
            valid &= !check(&node, true, 100).unwrap().violated;
            stack.extend(
                schema_children(node.view())
                    .into_iter()
                    .map(|v| v.to_owned()),
            );
        }
        assert_eq!(valid, expected, "localized {text}");
    }
}

#[test]
fn fixed_string_presentation_preserves_every_pinned_string_constraint() {
    // a new fixed constraint must not silently outgrow the presentation proof.
    let document: Value =
        serde_json::from_str(include_str!("../../schemas/openbindings.schema.json")).unwrap();
    let allowed_patterns = [
        r"^[A-Za-z0-9_][A-Za-z0-9_.-]*$",
        r"^[A-Za-z_][-A-Za-z0-9._]*$",
        r"^[^#]*#?$",
        r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-((?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$",
    ];
    let originals = originals();
    let mut stack: Vec<_> = originals.values().chain([&document]).collect();
    let mut patterns = BTreeSet::new();
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                if let Some(pattern) = map.get("pattern").and_then(Value::as_str) {
                    assert!(
                        allowed_patterns.contains(&pattern),
                        "review the new pattern: {pattern}"
                    );
                    patterns.insert(pattern);
                }
                if let Some(length) = map.get("minLength").and_then(Value::as_u64) {
                    assert_eq!(length, 1);
                }
                assert!(map.get("maxLength").is_none_or(Value::is_object));
                if let Some(choices) = map.get("enum").and_then(Value::as_array) {
                    assert!(
                        choices
                            .iter()
                            .all(|v| v.as_str().is_some_and(str::is_ascii))
                    );
                }
                assert!(!map.contains_key("const") || map["const"] == true);
                stack.extend(map.values());
            }
            Value::Array(values) => stack.extend(values),
            _ => {}
        }
    }
    assert_eq!(patterns.len(), allowed_patterns.len());
}

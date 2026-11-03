use openbindings::JsonValue;
use openbindings_schema_comparison::*;
use std::{collections::BTreeSet, path::PathBuf};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document_identity(left: &JsonValue, right: &JsonValue) -> Result<Identity, ComparisonFailure> {
    let left_operations = left.get("operations").unwrap();
    let right_operations = right.get("operations").unwrap();
    let left_keys: BTreeSet<_> = left_operations
        .members()
        .unwrap()
        .map(|m| m.name.as_str().unwrap())
        .collect();
    let right_keys: BTreeSet<_> = right_operations
        .members()
        .unwrap()
        .map(|m| m.name.as_str().unwrap())
        .collect();
    if left_keys != right_keys {
        return Ok(Identity::Different);
    }
    let mut answer = Identity::Equal;
    for key in left_keys {
        for slot in ["input", "output"] {
            let a = left_operations
                .get(key)
                .unwrap()
                .get(slot)
                .map(|schema| SchemaInput::new(schema.to_owned(), left.clone()));
            let b = right_operations
                .get(key)
                .unwrap()
                .get(slot)
                .map(|schema| SchemaInput::new(schema.to_owned(), right.clone()));
            if identical(a.as_ref(), b.as_ref(), &ComparisonLimits::default())?
                == Identity::Different
            {
                answer = Identity::Different;
            }
        }
    }
    Ok(answer)
}
#[test]
fn all_14_original_identity_groups_preserve_the_published_verdicts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/manifest.json")).unwrap();
    let mut identity = 0;
    let mut directional = 0;
    for record in manifest["files"].as_array().unwrap() {
        if record["mode"] != "identical" {
            directional += 1;
            continue;
        }
        identity += 1;
        let fixture =
            json(&std::fs::read_to_string(root.join(record["path"].as_str().unwrap())).unwrap());
        let result = document_identity(
            &fixture.get("left").unwrap().to_owned(),
            &fixture.get("right").unwrap().to_owned(),
        );
        let expected = if record["verdict"] == "compatible" {
            Identity::Equal
        } else {
            Identity::Different
        };
        assert_eq!(result, Ok(expected), "{}", record["path"]);
    }
    assert_eq!(
        (identity, directional),
        (14, 59),
        "directional groups are outside this identity API, not passed"
    );
}
#[test]
fn all_10_exact_value_identity_groups_preserve_the_published_verdicts() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/exact-values.json")).unwrap();
    let mut identity = 0;
    let mut directional = 0;
    for case in pack["cases"].as_array().unwrap() {
        if case["mode"] != "identical" {
            directional += 1;
            continue;
        }
        identity += 1;
        let result = document_identity(
            &json(case["leftJSON"].as_str().unwrap()),
            &json(case["rightJSON"].as_str().unwrap()),
        );
        let expected = if case["expected"]["verdict"] == "compatible" {
            Identity::Equal
        } else {
            Identity::Different
        };
        assert_eq!(result, Ok(expected), "{}", case["id"]);
    }
    assert_eq!(
        (identity, directional),
        (10, 29),
        "directional groups are outside this identity API, not passed"
    );
}

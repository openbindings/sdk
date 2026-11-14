use openbindings::ReferenceReport;
use openbindings::{Document, ExternalSchema, ExternalSchemas, JsonValue};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;

#[test]
fn probe_reports_preserve_the_original_resolver_results() {
    let mut observed = serde_json::Map::new();
    for shape in [
        "fragment",
        "identical",
        "long-leaf",
        "long-directory",
        "deep-relative-id",
    ] {
        let base = format!(
            "https://e.test/{}{}",
            "a".repeat(32),
            if shape == "long-directory" { "/" } else { "" }
        );
        let mut entry = json!({"$id":base,"$defs":{"s0":true,"s1":false,"s2":{"type":"string"}}});
        if shape == "deep-relative-id" {
            let mut nested = json!({"allOf":[{"$ref":"#"},{"$ref":"s0"}]});
            for _ in 0..8 {
                nested = json!({"$id":"a/","not":nested});
            }
            entry["not"] = nested;
        } else {
            entry["allOf"] = Value::Array(
                (0..3)
                    .map(|i| {
                        let reference = match shape {
                            "fragment" => format!("#/$defs/s{i}"),
                            "identical" => "#".into(),
                            _ => format!("s{i}"),
                        };
                        json!({"$ref":reference})
                    })
                    .collect(),
            );
        }
        let document = Document::parse(
            json!({"openbindings":"0.2.0","operations":{},"schemas":{"entry":entry}}).to_string(),
        )
        .unwrap();
        let proof = document.validate().unwrap().conformant().unwrap();
        let directory = if shape == "long-directory" {
            base.clone()
        } else if shape == "deep-relative-id" {
            format!("https://e.test/{}", "a/".repeat(8))
        } else {
            "https://e.test/".into()
        };
        for supplied in [false, true] {
            let resources = ExternalSchemas::new((0..if supplied { 3 } else { 0 }).map(|i| {
                ExternalSchema {
                    uri: format!("{directory}s{i}"),
                    document: JsonValue::parse(
                        r##"{"$defs":{"target":true},"$ref":"#/$defs/target"}"##,
                    )
                    .unwrap(),
                }
            }))
            .unwrap();
            let draft =
                document.references_with_resources(resources.clone(), &AtomicBool::new(false));
            let conformant = proof
                .document()
                .references_with_resources(resources, &AtomicBool::new(false));
            assert_eq!(draft, conformant, "{shape}/{supplied}");
            assert!(matches!(draft, ReferenceReport::Complete { .. }));
            observed.insert(
                format!("{shape}/{supplied}"),
                serde_json::to_value(draft).unwrap(),
            );
        }
    }
    let actual = Value::Object(observed);
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/reference_resolution_shapes.json")).unwrap();
    assert_eq!(actual, expected);
}

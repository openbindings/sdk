use openbindings::*;
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;

#[test]
fn resource_roots_agree_on_drafts_proofs_and_schema_nodes() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/reference_resource_roots.json")).unwrap();
    for case in cases {
        let resources =
            ExternalSchemas::new(case["resources"].as_array().unwrap().iter().map(|r| {
                ExternalSchema {
                    uri: r["uri"].as_str().unwrap().into(),
                    document: JsonValue::parse(r["document"].to_string()).unwrap(),
                }
            }))
            .unwrap();
        let mut body = case["document"].clone();
        let draft = Document::parse(body.to_string()).unwrap();
        let control = AtomicBool::new(false);
        let expected_report = draft.references_with_resources(resources.clone(), &control);
        assert!(expected_report.complete, "{}", case["name"]);
        let mut actual: Vec<Value> = expected_report
            .references
            .iter()
            .map(|r| {
                json!({"location": r.location, "resource_root": r.resource_root,
                "status": match r.resolution {
                    ReferenceResolution::Located { .. } => "located",
                    ReferenceResolution::Unresolved { .. } => "unresolved",
                }})
            })
            .collect();
        // Compare by original site, independently of JSON member insertion order.
        actual.sort_by_key(|v| v["location"].to_string());
        let mut expected = case["expected"].as_array().unwrap().clone();
        expected.sort_by_key(|v| v["location"].to_string());
        assert_eq!(actual, expected, "{}", case["name"]);
        body["openbindings"] = json!("0.2.0");
        let document = Document::parse(body.to_string()).unwrap();
        assert_eq!(
            document.references_with_resources(resources.clone(), &control),
            expected_report
        );
        if case["conformant"] == true {
            let proof = document.validate().unwrap().conformant().unwrap();
            assert_eq!(
                proof
                    .document()
                    .references_with_resources(resources.clone(), &control),
                expected_report
            );
            for reference in &expected_report.references {
                let holder = reference
                    .location
                    .pointer
                    .as_str()
                    .unwrap()
                    .rsplit_once('/')
                    .unwrap()
                    .0;
                let schema = match reference.location.source_uri.as_deref() {
                    None => {
                        let mut parts = holder.split('/').skip(1);
                        assert_eq!(parts.next(), Some("schemas"));
                        let schema = proof.schema(parts.next().unwrap()).unwrap();
                        let env = schema
                            .environment(&resources, &PreparationLimits::default(), &control)
                            .unwrap();
                        env.root().subschema(&parts.collect::<Vec<_>>()).unwrap()
                    }
                    Some(uri) => {
                        let selected = proof.schema("supplied").unwrap();
                        let env = selected
                            .environment(&resources, &PreparationLimits::default(), &control)
                            .unwrap();
                        let root = env.resolve(&env.root(), uri).unwrap().schema;
                        if holder.is_empty() {
                            root
                        } else {
                            root.subschema(&holder.split('/').skip(1).collect::<Vec<_>>())
                                .unwrap()
                        }
                    }
                };
                assert_eq!(
                    schema.resource_root().map(|root| root.origin().clone()),
                    reference.resource_root
                );
            }
        }
    }
}

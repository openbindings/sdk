//! S6e documented-API behavior witnesses for migration review.
//! Differences from frozen expectations do not themselves establish a spec gap.
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Recording(Arc<Mutex<Vec<String>>>);

impl jsonschema::Retrieve for Recording {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0.lock().unwrap().push(uri.to_string());
        // Inspection only: this registry is never used to evaluate an instance.
        Ok(json!({}))
    }
}

#[test]
fn library_treats_encoded_leading_slash_as_an_anchor() {
    const BASE: &str = "https://s6e.example.invalid/root";
    const REFERENCE: &str = "#%2F$defs%2Ftask";
    let schema = json!({
        "$id": BASE,
        "$defs": {"task": {"type": "string"}},
        "$ref": REFERENCE
    });
    let recording = Recording::default();
    let registry = jsonschema::Registry::new()
        .draft(jsonschema::Draft::Draft202012)
        .retriever(recording.clone())
        .add(BASE, &schema)
        .unwrap()
        .prepare()
        .unwrap();
    assert!(recording.0.lock().unwrap().is_empty());
    for reference in ["#/$defs/task", "#/%24defs/task"] {
        let resolved = registry
            .resolver(jsonschema::Uri::parse(BASE.to_owned()).unwrap())
            .lookup(reference)
            .unwrap();
        assert!(std::ptr::eq(resolved.contents(), &schema["$defs"]["task"]));
    }
    for reference in [REFERENCE.to_owned(), format!("{BASE}{REFERENCE}")] {
        let error = registry
            .resolver(jsonschema::Uri::parse(BASE.to_owned()).unwrap())
            .lookup(&reference)
            .unwrap_err();
        assert_eq!(error.to_string(), "Anchor '%2F$defs%2Ftask' does not exist");
        println!("S6E-ENCODED-LEADING-SLASH: {reference}: {error}");
    }
    // URI resolution is another documented entry point, but it retains the
    // encoded fragment. No SDK decoding or alternate lookup is introduced.
    let absolute = registry
        .resolve_uri(&jsonschema::Uri::parse(BASE).unwrap(), REFERENCE)
        .unwrap();
    assert_eq!(absolute.as_str(), format!("{BASE}{REFERENCE}"));
    assert!(
        registry
            .resolver(jsonschema::Uri::parse(BASE.to_owned()).unwrap())
            .lookup(absolute.as_str())
            .is_err()
    );
}

#[test]
fn recording_retrieval_and_borrowed_identity_controls() {
    const BASE: &str = "https://s6e.example.invalid/root";
    let schema = json!({
        "$id": BASE,
        "$defs": {
            "task": {"type": "string"},
            "missing": {"$ref": "https://missing.invalid/a"}
        },
        "$ref": "https://missing.invalid/b"
    });
    let recording = Recording::default();
    let registry = jsonschema::Registry::new()
        .draft(jsonschema::Draft::Draft202012)
        .retriever(recording.clone())
        .add(BASE, &schema)
        .unwrap()
        .prepare()
        .unwrap();
    let mut missing = recording.0.lock().unwrap().clone();
    missing.sort();
    assert_eq!(
        missing,
        ["https://missing.invalid/a", "https://missing.invalid/b"]
    );
    let resolved = registry
        .resolver(jsonschema::Uri::parse(BASE.to_owned()).unwrap())
        .lookup("#/%24defs/task")
        .unwrap();
    assert!(std::ptr::eq(resolved.contents(), &schema["$defs"]["task"]));
}

#[test]
fn library_normalizes_the_frozen_case_variant_unmatched_identity() {
    const ID: &str = "HTTPS://EX.invalid/%7ea/A";
    const REFERENCE: &str = "https://ex.invalid/%7Ea/A";
    let schema = json!({"$id":ID,"type":"string"});
    let recording = Recording::default();
    let registry = jsonschema::Registry::new()
        .draft(jsonschema::Draft::Draft202012)
        .retriever(recording.clone())
        .add(ID, &schema)
        .unwrap()
        .prepare()
        .unwrap();
    let resolver = registry
        .resolver(jsonschema::Uri::parse("urn:openbindings:inspection".to_owned()).unwrap());
    let resolved = resolver.lookup(REFERENCE).unwrap();
    assert!(std::ptr::eq(resolved.contents(), &schema));
    assert!(recording.0.lock().unwrap().is_empty());
    println!(
        "adversarial/case-variant-unmatched/0: library resolves authored reference to the original contained string schema without retrieval"
    );
}

#[test]
fn encoded_plain_names_keep_document_decoding_and_follow_the_library_elsewhere() {
    use openbindings::*;
    use openbindings_json_schema_evaluator::DefaultEvaluator;
    const URI: &str = "https://s6e.example.invalid/anchor";
    for carrier in ["document", "contained", "supplied"] {
        for fragment in ["task", "t%61sk"] {
            let mut schema = json!({"$defs":{"target":{"$anchor":"task","type":"string"}}});
            let mut supplied = ResourceSet::default();
            let reference = if carrier == "document" {
                format!("#{fragment}")
            } else {
                schema["$id"] = json!(URI);
                format!("{URI}#{fragment}")
            };
            let schemas = if carrier == "supplied" {
                supplied = ResourceSet::new([SchemaResource {
                    uri: URI.into(),
                    document: JsonValue::parse(schema.to_string()).unwrap(),
                }])
                .unwrap();
                json!({})
            } else {
                json!({"E":schema})
            };
            let doc = ParsedDocument::parse(
                json!({
                    "openbindings":"0.2.0", "schemas":schemas,
                    "operations":{"op":{"input":{"$ref":reference}}}
                })
                .to_string(),
            )
            .unwrap();
            let validation = doc.validate().unwrap();
            let proof = validation
                .validated()
                .expect("reference spellings are conformant");
            let report = proof.references_with_resources(
                supplied.clone(),
                &std::sync::atomic::AtomicBool::new(false),
            );
            let reference = report
                .references
                .iter()
                .find(|r| r.location.pointer == "/operations/op/input/$ref")
                .unwrap();
            let prepared = proof
                .value_contracts(Arc::new(DefaultEvaluator::new()), supplied)
                .prepare("op", Direction::Input);
            if fragment == "task" || carrier == "document" {
                assert!(matches!(
                    reference.resolution,
                    ReferenceResolution::Located { .. }
                ));
                let ValueContractPreparation::Ready(contract) = prepared else {
                    panic!("{prepared:?}")
                };
                assert!(matches!(
                    contract.validate(&JsonValue::parse(r#""s""#).unwrap()),
                    ValueOutcome::Satisfies
                ));
            } else {
                assert!(
                    matches!(&reference.resolution, ReferenceResolution::Unresolved { detail }
                    if detail.reason == ReferenceRefusalReason::MissingTarget && detail.code == "anchor-missing")
                );
                assert!(
                    matches!(prepared, ValueContractPreparation::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::ConservativePreparation && detail.code == "anchor-missing")
                );
            }
        }
    }
}

#[test]
fn documented_registry_routes_require_preparing_the_malformed_carrier() {
    const BASE: &str = "https://probe.invalid/root";
    struct Carrier(Value);
    impl jsonschema::Retrieve for Carrier {
        fn retrieve(
            &self,
            uri: &jsonschema::Uri<String>,
        ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
            assert_eq!(uri.as_str(), BASE);
            Ok(self.0.clone())
        }
    }
    for bad in [json!({"not":{"$ref":"%"}}), json!({"$id":"%"})] {
        let schema = json!({"$id":BASE,"$defs":{"good":{"type":"string"},"bad":bad}});
        let borrowed = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(Recording::default())
            .add(BASE, &schema)
            .unwrap()
            .prepare();
        assert!(borrowed.is_err(), "borrowed carrier unexpectedly prepared");
        let owned = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(Recording::default())
            .add(BASE, schema.clone())
            .unwrap()
            .prepare();
        assert!(owned.is_err(), "owned carrier unexpectedly prepared");
        // Start with only the desired reference and let the documented retriever
        // supply its carrier. Preparation still refuses the unused bad sibling.
        let queried = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(Carrier(schema.clone()))
            .add(
                "urn:probe:query",
                json!({"$ref":format!("{BASE}#/$defs/good")}),
            )
            .unwrap()
            .prepare();
        assert!(
            queried.is_err(),
            "query-only registration unexpectedly prepared"
        );
        let empty = jsonschema::Registry::new()
            .draft(jsonschema::Draft::Draft202012)
            .retriever(Recording::default())
            .prepare()
            .unwrap();
        assert!(empty.add(BASE, &schema).unwrap().prepare().is_err());
        let healthy = json!({"$id":BASE,"$defs":{"good":{"type":"string"}}});
        let registry = empty.add(BASE, &healthy).unwrap().prepare().unwrap();
        let resolved = registry
            .resolver(jsonschema::Uri::parse(BASE.to_owned()).unwrap())
            .lookup("#/$defs/good")
            .unwrap();
        assert!(std::ptr::eq(resolved.contents(), &healthy["$defs"]["good"]));
    }
}

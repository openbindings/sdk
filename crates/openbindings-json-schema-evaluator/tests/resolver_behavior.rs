//! Documented resolver API behavior witnesses.
//! Differences from frozen expectations do not themselves establish a spec gap.
use serde_json::json;

#[test]
fn encoded_plain_names_resolve_in_each_original_resource() {
    use openbindings::*;
    use openbindings_json_schema_evaluator::DefaultEvaluator;
    const URI: &str = "https://resolver.example.invalid/anchor";
    for carrier in ["document", "contained", "supplied"] {
        for fragment in ["task", "t%61sk"] {
            let mut schema = json!({"$defs":{"target":{"$anchor":"task","type":"string"}}});
            let mut supplied = ExternalSchemas::default();
            let reference = if carrier == "document" {
                format!("#{fragment}")
            } else {
                schema["$id"] = json!(URI);
                format!("{URI}#{fragment}")
            };
            let schemas = if carrier == "supplied" {
                supplied = ExternalSchemas::new([ExternalSchema {
                    uri: URI.into(),
                    document: JsonValue::parse(schema.to_string()).unwrap(),
                }])
                .unwrap();
                json!({})
            } else {
                json!({"E":schema})
            };
            let doc = Document::parse(
                json!({
                    "openbindings":"0.2.0", "schemas":schemas,
                    "operations":{"op":{"input":{"$ref":reference}}}
                })
                .to_string(),
            )
            .unwrap();
            let validation = doc.validate().unwrap();
            let proof = validation
                .conformant()
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
            let operation = proof.resolve_operation("op").expect("declared operation");
            let prepared = operation.input().expect("declared input").prepare(
                &DefaultEvaluator::new(),
                &supplied,
                &openbindings::PreparationLimits::default(),
            );
            assert!(matches!(
                reference.resolution,
                ReferenceResolution::Located { .. }
            ));
            let contract =
                prepared.unwrap_or_else(|detail| panic!("{carrier} {fragment}: {detail:?}"));
            assert!(matches!(
                contract.evaluate(&JsonValue::parse(r#""s""#).unwrap()),
                ValueOutcome::Satisfies
            ));
            assert!(matches!(
                contract.evaluate(&JsonValue::parse("1").unwrap()),
                ValueOutcome::Fails { .. }
            ));
        }
    }
}

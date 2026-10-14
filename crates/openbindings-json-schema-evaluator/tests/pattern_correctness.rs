use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use serde_json::{Value, json};
use std::sync::Arc;

fn prepare(schema: Value, resources: ResourceSet) -> ValueContractPreparation {
    let document = ParsedDocument::parse(
        json!({"openbindings":"0.2.0","operations":{"op":{"input":schema}}}).to_string(),
    )
    .unwrap();
    document
        .value_contracts(Arc::new(DefaultEvaluator::new()), resources)
        .unwrap()
        .prepare("op", Direction::Input)
}

#[test]
fn node_surrogate_differential_for_both_pattern_keywords() {
    for row in include_str!(
        "../../../vendor/openbindings-internal-regress/tests/fixtures/lone-surrogate.tsv"
    )
    .lines()
    .skip(1)
    {
        let fields: Vec<_> = row.split('\t').collect();
        let [id, pattern, input, expected] = fields.as_slice() else {
            panic!("{row}")
        };
        for (keyword, schema, value) in [
            ("pattern", json!({"pattern":pattern}), json!(input)),
            (
                "patternProperties",
                json!({"patternProperties":{*pattern:true},"additionalProperties":false}),
                json!({*input:null}),
            ),
        ] {
            let ValueContractPreparation::Ready(contract) = prepare(schema, ResourceSet::default())
            else {
                panic!("{id}/{keyword}")
            };
            let value = JsonValue::parse(value.to_string()).unwrap();
            let result = contract.validate(&value);
            match result {
                ValueOutcome::Satisfies => assert_eq!(*expected, "true", "{id}/{keyword}"),
                ValueOutcome::Fails { .. } => assert_eq!(*expected, "false", "{id}/{keyword}"),
                other => panic!("{id}/{keyword}: {other:?}"),
            }
        }
    }
}

#[test]
fn compilation_refusals_locate_authored_keywords_in_document_and_supplied_resources() {
    const URI: &str = "https://patterns.invalid/supplied";
    for (keyword, invalid) in [
        ("pattern", json!({"pattern":"[SECRET/~%"})),
        (
            "patternProperties",
            json!({"patternProperties":{"[SECRET/~%":true}}),
        ),
    ] {
        for supplied in [false, true] {
            let schema = json!({"properties":{"a/b~c":invalid}});
            let (entry, resources, resource, prefix) = if supplied {
                (
                    json!({"$ref":URI}),
                    ResourceSet::new([SchemaResource {
                        uri: URI.into(),
                        document: JsonValue::parse(schema.to_string()).unwrap(),
                    }])
                    .unwrap(),
                    Some(URI.to_owned()),
                    "",
                )
            } else {
                (schema, ResourceSet::default(), None, "/operations/op/input")
            };
            let ValueContractPreparation::NoVerdict { detail } = prepare(entry, resources) else {
                panic!("{keyword}/{supplied}")
            };
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(detail.code, "schema-pattern-compilation");
            assert_eq!(
                detail.location,
                Some(SchemaLocation {
                    resource,
                    pointer: format!("{prefix}/properties/a~1b~0c/{keyword}"),
                })
            );
            assert_eq!(
                detail.message,
                format!("the schema {keyword} regular expression could not be compiled")
            );
            assert!(!detail.message.contains("SECRET"));
        }
    }
}

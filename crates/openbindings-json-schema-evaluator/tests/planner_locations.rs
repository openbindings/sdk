//! Public preparation and direct-planner controls, using the production caps.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::{Arc, Mutex};

const U: &str = "https://absent.invalid/U";
const R: &str = "https://known.invalid/R";
const INPUT: &str = "/operations/op/input";
struct Capture(Mutex<Option<SchemaRequest>>);
impl SchemaEvaluator for Capture {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        *self.0.lock().unwrap() = Some(request.clone());
        Err(NoVerdict::new(
            NoVerdictReason::EvaluatorFailure,
            "capture",
            "test capture",
        ))
    }
}
fn document(schema: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}},"healthy":{{"input":true}}}}}}"#)).unwrap()
}
fn resources(entries: &[(&str, &str)]) -> ResourceSet {
    ResourceSet::new(entries.iter().map(|(uri, text)| SchemaResource {
        uri: (*uri).into(),
        document: JsonValue::parse(*text).unwrap(),
    }))
    .unwrap()
}
fn request(document: &ParsedDocument, resources: ResourceSet) -> SchemaRequest {
    let capture = Arc::new(Capture(Mutex::new(None)));
    document
        .value_contracts(capture.clone(), resources)
        .unwrap()
        .prepare("op", Side::Input);
    capture.0.lock().unwrap().take().unwrap()
}
fn refused(context: &ValueContracts) -> NoVerdict {
    let ContractPreparation::NoVerdict { detail } = context.prepare("op", Side::Input) else {
        panic!("expected refusal")
    };
    detail
}
fn at(detail: &NoVerdict, code: &str, resource: Option<&str>, pointer: &str) {
    assert_eq!(detail.code, code);
    let location = detail.location.as_ref().expect("original source location");
    assert_eq!(location.resource.as_deref(), resource);
    assert_eq!(location.pointer, pointer);
    assert!(!detail.message.contains("sdk-bounds.openbindings.invalid"));
    assert!(detail.message.len() < 192);
}
#[test]
fn frozen_fragment_witnesses_and_escaped_supplied_source_keep_original_locations() {
    for (schema, suffix) in [
        (
            format!(r#"{{"anyOf":[true,{{"$ref":"{U}#/~2"}}]}}"#),
            "/anyOf/1/$ref",
        ),
        (
            format!(r#"{{"properties":{{"quote\"/tilde~雪":{{"$ref":"{U}#/~2"}}}}}}"#),
            "/properties/quote\"~1tilde~0雪/$ref",
        ),
    ] {
        for external in [false, true] {
            let doc = document(&if external {
                format!(r#"{{"$ref":"{R}"}}"#)
            } else {
                schema.clone()
            });
            let entries = resources(&if external {
                vec![(R, schema.as_str())]
            } else {
                vec![]
            });
            let request = request(&doc, entries.clone());
            let direct = request.evaluation_bounds(&WorkControl::new()).unwrap_err();
            let context = doc
                .value_contracts(Arc::new(DefaultEvaluator::new()), entries)
                .unwrap();
            let detail = refused(&context);
            let pointer = format!("{}{suffix}", if external { "" } else { INPUT });
            at(
                &detail,
                "invalid-reference-fragment",
                external.then_some(R),
                &pointer,
            );
            assert_eq!(detail.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(
                serde_json::to_value(&detail).unwrap(),
                serde_json::to_value(direct).unwrap()
            );
            assert_eq!(
                serde_json::to_value(refused(&context)).unwrap(),
                serde_json::to_value(&detail).unwrap()
            );
            assert!(matches!(
                context.prepare("healthy", Side::Input),
                ContractPreparation::Ready(_)
            ));
        }
    }
}
#[test]
fn direct_keyword_refusals_and_recovered_strict_holders_are_precise_and_stable() {
    // Both branches remain excluded; oneOf is qualified and is not a refusal control.
    let schema = format!(
        r#"{{"properties":{{"a":{{"not":{{"$ref":"{U}"}}}},"b":{{"contains":{{"$ref":"https://absent.invalid/V"}}}}}}}}"#
    );
    for _ in 0..24 {
        let doc = document(&schema);
        let request = request(&doc, ResourceSet::default());
        at(
            &request.evaluation_bounds(&WorkControl::new()).unwrap_err(),
            "partial-nonpositive-influence",
            None,
            &format!("{INPUT}/properties/a/not"),
        );
        let strict = request.evaluation_program(&WorkControl::new()).unwrap_err();
        at(
            &strict,
            "resource-unavailable",
            None,
            &format!("{INPUT}/properties/a/not"),
        );
        let context = doc
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
            .unwrap();
        assert_eq!(
            serde_json::to_value(refused(&context)).unwrap(),
            serde_json::to_value(strict).unwrap()
        );
    }
    for keyword in [
        "unevaluatedProperties",
        "unevaluatedItems",
        "$dynamicRef",
        "$dynamicAnchor",
    ] {
        let value = match keyword {
            "$dynamicRef" => format!("\"{U}\""),
            "$dynamicAnchor" => "\"node\"".into(),
            _ => "false".into(),
        };
        let schema = format!(r#"{{"$ref":"{U}","{keyword}":{value}}}"#);
        let doc = document(&format!(r#"{{"$ref":"{R}"}}"#));
        let request = request(&doc, resources(&[(R, &schema)]));
        at(
            &request.evaluation_bounds(&WorkControl::new()).unwrap_err(),
            "partial-annotation-or-dynamic",
            Some(R),
            &format!("/{keyword}"),
        );
    }
}
#[test]
fn strict_and_partial_multinode_cycle_roots_are_stable() {
    for missing in [false, true] {
        let prefix = if missing {
            format!(r#""$ref":"{U}","#)
        } else {
            String::new()
        };
        let schema = format!(
            r##"{{{prefix}"$defs":{{"a":{{"$ref":"#/operations/op/input/$defs/b"}},"b":{{"$ref":"#/operations/op/input/$defs/a"}}}},"allOf":[{{"$ref":"#/operations/op/input/$defs/a"}}]}}"##
        );
        for _ in 0..24 {
            let doc = document(&schema);
            let request = request(&doc, ResourceSet::default());
            let direct = request.evaluation_bounds(&WorkControl::new()).unwrap_err();
            at(&direct, "in-place-cycle", None, &format!("{INPUT}/$defs/a"));
            let context = doc
                .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
                .unwrap();
            at(
                &refused(&context),
                "in-place-cycle",
                None,
                &format!("{INPUT}/$defs/a"),
            );
        }
    }
}
#[test]
fn actual_production_node_and_edge_caps_preserve_known_limits_before_and_after_supply() {
    for count in [100_000, 200_000] {
        let schema = format!(
            r#"{{"$ref":"{U}","allOf":[{}]}}"#,
            vec!["true"; count].join(",")
        );
        let doc = document(&schema);
        let request = request(&doc, ResourceSet::default());
        let direct = request.evaluation_bounds(&WorkControl::new()).unwrap_err();
        let code = if count == 100_000 {
            "schema-node-limit"
        } else {
            "schema-edge-limit"
        };
        let pointer = format!("{INPUT}/allOf/{}", count - 1);
        at(&direct, code, None, &pointer);
        assert_eq!(direct.reason, NoVerdictReason::LimitExceeded);
        for supplied in [false, true] {
            let entries = resources(&if supplied { vec![(U, "true")] } else { vec![] });
            let context = doc
                .value_contracts(Arc::new(DefaultEvaluator::new()), entries)
                .unwrap();
            let detail = refused(&context);
            at(
                &detail,
                if supplied { "schema-node-limit" } else { code },
                None,
                &if supplied {
                    format!("{INPUT}/allOf/99998")
                } else {
                    pointer.clone()
                },
            );
            assert_eq!(detail.reason, NoVerdictReason::LimitExceeded);
            assert!(matches!(
                context.prepare("healthy", Side::Input),
                ContractPreparation::Ready(_)
            ));
        }
    }
}

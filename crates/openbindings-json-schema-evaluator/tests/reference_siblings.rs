use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

const ROOT: &str = "https://siblings.test/root";
const VALUES: [&str; 4] = [r#"{"a":"s"}"#, r#"{"a":1}"#, "{}", "1"];

fn bundle(dialect: &str, sibling: bool) -> String {
    let sibling = if sibling { r#", "type":"string""# } else { "" };
    format!(
        r##"{{"$schema":"{dialect}","$ref":"#/definitions/X"{sibling},"definitions":{{"X":{{"type":"object","required":["a"],"properties":{{"a":{{"type":"string"}}}}}}}}}}"##
    )
}

fn resources(schema: &str) -> ExternalSchemas {
    ExternalSchemas::new([ExternalSchema {
        uri: ROOT.into(),
        document: JsonValue::parse(schema).unwrap(),
    }])
    .unwrap()
}

#[test]
fn bundled_compiler_ignores_reference_siblings_in_drafts_04_and_07() {
    for (uri, draft) in [
        (
            "http://json-schema.org/draft-04/schema#",
            jsonschema::Draft::Draft4,
        ),
        (
            "http://json-schema.org/draft-07/schema#",
            jsonschema::Draft::Draft7,
        ),
    ] {
        for sibling in [false, true] {
            let schema = serde_json::from_str(&bundle(uri, sibling)).unwrap();
            let compiled = jsonschema::options()
                .with_draft(draft)
                .build(&schema)
                .unwrap();
            for (value, expected) in VALUES.into_iter().zip([true, false, false, false]) {
                assert_eq!(
                    compiled.is_valid(&serde_json::from_str(value).unwrap()),
                    expected
                );
            }
        }
    }
}

#[test]
fn draft_07_reference_siblings_remain_addressable_and_unevaluated() {
    for sibling in [false, true] {
        let resources = resources(&bundle("http://json-schema.org/draft-07/schema#", sibling));
        for work in [None, Some(usize::MAX)] {
            let prepared = resources
                .prepare(
                    ROOT,
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_work_units: work,
                        ..Limits::default()
                    }),
                )
                .unwrap();
            for (value, expected) in VALUES.into_iter().zip([true, false, false, false]) {
                let value = JsonValue::parse(value).unwrap();
                for outcome in [prepared.evaluate(&value), prepared.explain(&value)] {
                    assert!(
                        matches!(
                            outcome,
                            ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                        ),
                        "{outcome:?}"
                    );
                    assert_eq!(matches!(outcome, ValueOutcome::Satisfies), expected);
                }
            }
        }
    }
}

#[test]
fn draft_04_reference_bundles_keep_the_unavailable_meta_schema_refusal() {
    for sibling in [false, true] {
        let resources = resources(&bundle("http://json-schema.org/draft-04/schema#", sibling));
        for work in [None, Some(usize::MAX)] {
            let failure = resources
                .prepare(
                    ROOT,
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_work_units: work,
                        ..Limits::default()
                    }),
                )
                .unwrap_err();
            assert!(
                matches!(failure.kind, PreparationFailureKind::Resolution {
                cause: ResolutionFailureKind::ResourceUnavailable { ref uri }
            } if uri == "http://json-schema.org/draft-04/schema"),
                "{}",
                serde_json::to_string(&failure).unwrap()
            );
        }
    }
}

#[test]
fn draft_2020_reference_siblings_still_constrain_the_value() {
    for sibling in [false, true] {
        let resources = resources(&bundle(
            "https://json-schema.org/draft/2020-12/schema",
            sibling,
        ));
        for work in [None, Some(usize::MAX)] {
            let prepared = resources
                .prepare(
                    ROOT,
                    &DefaultEvaluator::with_limits(Limits {
                        evaluation_sdk_work_units: work,
                        ..Limits::default()
                    }),
                )
                .unwrap();
            for (value, expected) in VALUES.into_iter().zip([!sibling, false, false, false]) {
                let outcome = prepared.evaluate(&JsonValue::parse(value).unwrap());
                assert!(matches!(
                    outcome,
                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                ));
                assert_eq!(matches!(outcome, ValueOutcome::Satisfies), expected);
            }
        }
    }
}

#[test]
fn draft_07_pointer_targets_must_still_be_schema_values() {
    for target in ["12", "null", "[]", r#""text""#] {
        let resources = resources(&format!(
            r##"{{"$schema":"http://json-schema.org/draft-07/schema#","$ref":"#/definitions/X","definitions":{{"X":{target}}}}}"##
        ));
        let prepared = resources.prepare(ROOT, &DefaultEvaluator::new()).unwrap();
        assert!(matches!(
            prepared.evaluate(&JsonValue::parse("1").unwrap()),
            ValueOutcome::Undefined { .. }
        ));
    }
}

#[test]
fn addressable_reference_siblings_keep_their_identifiers_ignored() {
    let schema = format!(
        r##"{{"$schema":"http://json-schema.org/draft-07/schema#","$ref":"#/definitions/X","definitions":{{"X":{{"type":"integer"}}}},"properties":{{"ignored":{{"$id":"{ROOT}","$schema":"urn:unread","type":17}}}}}}"##
    );
    let prepared = resources(&schema)
        .prepare(ROOT, &DefaultEvaluator::new())
        .unwrap();
    assert_eq!(
        prepared.evaluate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Satisfies
    );
    let schema = r##"{"$schema":"http://json-schema.org/draft-07/schema#","$ref":"#hidden","definitions":{"X":{"$id":"#hidden","type":"integer"}}}"##;
    let prepared = resources(schema)
        .prepare(ROOT, &DefaultEvaluator::new())
        .unwrap();
    assert!(matches!(
        prepared.evaluate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Undefined { .. }
    ));
}

#[test]
fn mixed_dialect_reference_siblings_keep_nested_identifiers_ignored() {
    let schema = format!(
        r##"{{"$ref":"nested.json","$defs":{{"nested":{{"$id":"nested.json","$schema":"http://json-schema.org/draft-07/schema#","$ref":"#/definitions/X","definitions":{{"X":{{"type":"integer"}}}},"properties":{{"ignored":{{"$id":"{ROOT}"}}}}}}}}}}"##
    );
    let prepared = resources(&schema)
        .prepare(ROOT, &DefaultEvaluator::new())
        .unwrap();
    assert_eq!(
        prepared.evaluate(&JsonValue::parse("1").unwrap()),
        ValueOutcome::Satisfies
    );
    assert!(matches!(
        prepared.evaluate(&JsonValue::parse(r#""x""#).unwrap()),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn contained_schema_reading_stays_2020_with_compiled_and_sdk_evaluation() {
    let schema = bundle("http://json-schema.org/draft-07/schema#", true)
        .replace("#/definitions/X", "#/operations/op/input/definitions/X");
    let wrap = |schema| {
        format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#)
    };
    assert!(
        Document::parse(wrap(&schema))
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .is_none()
    );
    let schema = schema.replace(
        "http://json-schema.org/draft-07/schema#",
        "https://json-schema.org/draft/2020-12/schema",
    );
    let document = Document::parse(wrap(&schema))
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    for work in [None, Some(usize::MAX)] {
        let prepared = document
            .operation("op")
            .unwrap()
            .input()
            .unwrap()
            .prepare(
                &DefaultEvaluator::with_limits(Limits {
                    evaluation_sdk_work_units: work,
                    ..Limits::default()
                }),
                &ExternalSchemas::default(),
            )
            .unwrap();
        assert_eq!(prepared.retained_bytes().is_none(), work.is_none());
        for value in VALUES {
            assert!(matches!(
                prepared.evaluate(&JsonValue::parse(value).unwrap()),
                ValueOutcome::Fails { .. }
            ));
        }
    }
}

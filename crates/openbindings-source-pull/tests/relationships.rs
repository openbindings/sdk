use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_source_pull::*;
use std::sync::atomic::AtomicBool;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn checked(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}

#[test]
fn independent_74_design_controls_keep_their_original_classifications() {
    let pack = json(include_str!("fixtures/design-controls.json"));
    let cases = pack.get("cases").unwrap();
    assert_eq!(cases.len(), Some(74));
    for case in cases.elements().unwrap() {
        let name = case.get("id").unwrap().as_str().unwrap();
        let expected = case.get("expected").unwrap().as_str().unwrap();
        let request = PullRequest::from_json(&case.get("request").unwrap().to_owned());
        if expected == "reject-request" {
            assert!(request.is_err(), "{name}");
            continue;
        }
        let request = request.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let result = PullResult::from_json(case.get("result").unwrap().to_owned());
        if matches!(expected, "reject-carrier" | "reject-proposal-authority") {
            assert!(result.is_err(), "{name}");
            continue;
        }
        // The reviewed carrier already rejects duplicate uses, before document
        // relationships. Keep the independent case unchanged and name this earlier gate.
        if name == "uses-labels-distinct" {
            assert!(matches!(result, Err(CarrierError::DuplicateUse)), "{name}");
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let checked =
            result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
        match expected {
            "accept" | "kind-evidence-required" => assert!(checked.is_ok(), "{name}: {checked:?}"),
            "reject-before-provider" => assert!(
                matches!(checked, Err(PullValidationError::MissingSource)),
                "{name}: {checked:?}"
            ),
            "reject-relationship" => assert!(
                matches!(checked, Err(PullValidationError::Relationship(_))),
                "{name}: {checked:?}"
            ),
            "reject-prospective" => assert!(
                matches!(checked, Err(PullValidationError::Prospective(_))),
                "{name}: {checked:?}"
            ),
            _ => panic!("unknown expected classification {expected}"),
        }
    }
}

#[test]
fn five_published_examples_pass_request_relationship_and_prospective_gates() {
    let publication = json(include_str!("fixtures/source-puller.json"));
    let examples = publication
        .get("operations")
        .unwrap()
        .get("openbindings.source-puller.pullSource")
        .unwrap()
        .get("examples")
        .unwrap();
    for example in examples.members().unwrap() {
        let request =
            PullRequest::from_json(&example.value.get("input").unwrap().to_owned()).unwrap();
        let result =
            PullResult::from_json(example.value.get("output").unwrap().to_owned()).unwrap();
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_or_else(|e| panic!("{}: {e:?}", example.name.text()));
    }
}

#[test]
fn prospective_snapshot_adds_only_declared_maps_and_never_applies_proposals() {
    let original = checked(
        r#"{"openbindings":"0.2.0","operations":{"a":{"description":"author","input":true}},"sources":{"s":{"kind":"example@1","data":null}},"bindings":{"a":{"source":"s","operation":"a","data":{"old":true}}}}"#,
    );
    let request = PullRequest::new(original.clone(), "s");
    let result = PullResult::from_json(json(r#"{"additions":{"schemas":{"S":false},"metadata":{"secret":1}},"report":{"exhaustive":true,"added":[],"existing":[{"binding":"a","status":"stale","change":"dangerous","proposal":{"input":false,"data":null}},{"source":"s","status":"stale","change":"dangerous","proposal":{"data":{"refreshed":true}}}]}}"#)).unwrap();
    let prospective = result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        prospective
            .value()
            .get("schemas")
            .unwrap()
            .get("S")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    for role in ["operations", "sources", "bindings"] {
        assert_eq!(
            prospective.value().get(role).unwrap().text(),
            original.value().get(role).unwrap().text()
        );
    }
    assert!(original.value().get("schemas").is_none());
    assert!(prospective.value().get("metadata").is_none());
}

#[test]
fn request_and_response_boundaries_refuse_before_prospective_work() {
    let request = PullRequest::new(
        checked(r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#),
        "s",
    );
    let result = PullResult::from_json(json(
        r#"{"additions":{},"report":{"exhaustive":true,"added":[]}}"#,
    ))
    .unwrap();
    for limits in [
        PullLimits {
            max_request_bytes: 0,
            ..PullLimits::default()
        },
        PullLimits {
            max_response_bytes: 0,
            ..PullLimits::default()
        },
        PullLimits {
            max_relationship_work: 0,
            ..PullLimits::default()
        },
    ] {
        assert!(matches!(
            result.validate_for(&request, &limits, &AtomicBool::new(false)),
            Err(PullValidationError::LimitExceeded(_))
        ));
    }
    assert!(matches!(
        result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(true)),
        Err(PullValidationError::Cancelled)
    ));
}

#[test]
fn every_validation_cancellation_checkpoint_allows_an_independent_retry() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counter {
        calls: AtomicUsize,
        cancel_at: usize,
    }
    impl openbindings::WorkControl for Counter {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst) >= self.cancel_at
        }
    }
    let request = PullRequest::new(
        checked(r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#),
        "s",
    );
    let result = PullResult::from_json(json(r#"{"additions":{"operations":{"a":{},"b":{}},"schemas":{"S":true},"bindings":{"a":{"source":"s","operation":"a"},"b":{"source":"s","operation":"b"}}},"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a","uses":["bindings/b","operations/b","schemas/S"]},{"binding":"b","operation":"b","uses":["schemas/S"]}]}}"#)).unwrap();
    let baseline = Counter {
        calls: AtomicUsize::new(0),
        cancel_at: usize::MAX,
    };
    result
        .validate_for(&request, &PullLimits::default(), &baseline)
        .unwrap();
    let checkpoints = baseline.calls.load(Ordering::SeqCst);
    assert!(checkpoints > 30);
    for cancel_at in 0..checkpoints {
        let counter = Counter {
            calls: AtomicUsize::new(0),
            cancel_at,
        };
        assert!(
            matches!(
                result.validate_for(&request, &PullLimits::default(), &counter),
                Err(PullValidationError::Cancelled)
            ),
            "checkpoint {cancel_at}"
        );
    }
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

#[test]
fn core_references_close_uses_across_operations_schemas_and_resources() {
    let request = PullRequest::new(
        checked(r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}}}"#),
        "s",
    );
    for reference in ["#/schemas/A", "urn:example:A"] {
        for (uses, valid) in [
            (vec![], false),
            (vec!["schemas/A"], false),
            (vec!["schemas/A", "schemas/B"], true),
        ] {
            let value = serde_json::json!({"additions":{
                "operations":{"o":{"input":{"$ref":reference}}},
                "schemas":{"A":{"$id":"urn:example:A","$ref":"urn:example:B"},"B":{"$id":"urn:example:B","type":"string"}},
                "bindings":{"b":{"operation":"o","source":"s","data":{"$ref":"#/schemas/opaque"}}}
            },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o","uses":uses}]}});
            let result = PullResult::from_json(json(&value.to_string())).unwrap();
            let checked =
                result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
            if valid {
                assert!(checked.is_ok(), "{checked:?}");
            } else {
                assert!(
                    matches!(
                        checked,
                        Err(PullValidationError::Relationship(
                            RelationshipError::UsesClosure
                        ))
                    ),
                    "{checked:?}"
                );
            }
        }
    }
}

#[test]
fn proposal_schema_references_and_their_uses_are_closed() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{"o":{}},"schemas":{"S":true},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"o","source":"s"}}}"#,
        ),
        "s",
    );
    for schema_proposal in [true, false] {
        for (uses, valid) in [
            (vec![], false),
            (vec!["schemas/A"], false),
            (vec!["schemas/A", "schemas/B"], true),
        ] {
            let mut proposal = serde_json::json!({"uses":uses});
            proposal[if schema_proposal { "schema" } else { "input" }] =
                serde_json::json!({"$ref":"#/schemas/A"});
            let mut existing =
                serde_json::json!({"status":"stale","change":"dangerous","proposal":proposal});
            existing[if schema_proposal { "schema" } else { "binding" }] =
                serde_json::json!(if schema_proposal { "S" } else { "b" });
            let entries = if schema_proposal {
                vec![
                    existing,
                    serde_json::json!({"binding":"b","status":"current"}),
                ]
            } else {
                vec![existing]
            };
            let value = serde_json::json!({"additions":{"schemas":{"A":{"$ref":"#/schemas/B"},"B":true}},"report":{"exhaustive":true,"added":[],"existing":entries}});
            let result = PullResult::from_json(json(&value.to_string())).unwrap();
            let checked =
                result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
            assert_eq!(checked.is_ok(), valid, "{checked:?}");
        }
    }
}

#[test]
fn schema_proposal_may_use_an_added_binding_with_the_same_key() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{},"schemas":{"S":true},"sources":{"s":{"kind":"k"}}}"#,
        ),
        "s",
    );
    let result = PullResult::from_json(json(
        r#"{
        "additions":{"operations":{"o":{}},"bindings":{"S":{"operation":"o","source":"s"}}},
        "report":{"exhaustive":true,"added":[{"binding":"S","operation":"o"}],
            "existing":[{"schema":"S","status":"stale","change":"dangerous",
                "proposal":{"schema":false,"uses":["bindings/S","operations/o"]}}]}
    }"#,
    ))
    .unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

#[test]
fn core_reference_closure_follows_existing_entries_to_additions() {
    let request = PullRequest::new(
        checked(
            r##"{"openbindings":"0.2.0","operations":{},"schemas":{"A":{"$ref":"urn:example:B"}},"sources":{"s":{"kind":"k"}}}"##,
        ),
        "s",
    );
    for (uses, valid) in [(vec![], false), (vec!["schemas/B"], true)] {
        let value = serde_json::json!({"additions":{
            "operations":{"o":{"input":{"$ref":"#/schemas/A"}}},
            "schemas":{"B":{"$id":"urn:example:B","type":"string"}},
            "bindings":{"b":{"operation":"o","source":"s"}}
        },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o","uses":uses}]}});
        let result = PullResult::from_json(json(&value.to_string())).unwrap();
        let checked =
            result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
        assert_eq!(checked.is_ok(), valid, "{checked:?}");
    }
}

#[test]
fn an_existing_operation_reference_follows_only_the_named_slot() {
    let request = PullRequest::new(
        checked(
            r##"{"openbindings":"0.2.0","operations":{"existing":{"input":true,"output":{"$ref":"urn:example:B"}}},"sources":{"s":{"kind":"k"}}}"##,
        ),
        "s",
    );
    let result = PullResult::from_json(json(r##"{"additions":{"operations":{"o":{"input":{"$ref":"#/operations/existing/input"}}},"schemas":{"B":{"$id":"urn:example:B","type":"string"}},"bindings":{"b":{"operation":"o","source":"s"}}},"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o"}]}}"##)).unwrap();
    assert!(
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .is_ok()
    );
}

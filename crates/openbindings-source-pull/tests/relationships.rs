use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_source_pull::*;
use std::error::Error;
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
fn wrapper_errors_expose_causes_without_repeating_them() {
    let request_error = PullRequest::from_json(&json(
        r#"{"interface":{"openbindings":"8.0.0"},"source":"private-source"}"#,
    ))
    .unwrap_err();
    let cause = request_error.source().unwrap();
    assert!(cause.is::<openbindings::VersionRefusal>());
    assert_eq!(
        request_error.to_string(),
        "source-pull request version is not supported"
    );
    let RequestError::Version(refusal) = request_error else {
        panic!("version refusal")
    };
    let validation_error = PullValidationError::Version(refusal);
    assert!(
        validation_error
            .source()
            .unwrap()
            .is::<openbindings::VersionRefusal>()
    );
    assert_eq!(
        validation_error.to_string(),
        "source-pull prospective document version is not supported"
    );

    let original = Document::parse_with_limits(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#,
        openbindings::JsonLimits {
            max_nodes: 20,
            ..Default::default()
        },
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let request = PullRequest::new(original, "s");
    let result = PullResult::from_json(json(
        r#"{"additions":{"schemas":{"Large":{"enum":[0,1,2,3,4,5,6,7,8,9]}}},"report":{"exhaustive":true,"added":[]}}"#,
    )).unwrap();
    let error = result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "source-pull validation could not build the prospective document"
    );
    let edit = error
        .source()
        .unwrap()
        .downcast_ref::<openbindings::EditError>()
        .unwrap();
    assert_eq!(edit.code(), "node-limit");
    assert!(edit.source().unwrap().is::<openbindings::InputError>());
    // Display is the fixed layer text; the SDK constant naming the field stays
    // in the variant (and its Debug) for explicit inspection.
    assert_eq!(
        RequestError::Shape("source").to_string(),
        "source-pull request has an invalid shape"
    );
}

#[test]
fn independent_74_design_controls_match_the_current_rules() {
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
fn request_decoding_refuses_repeated_context_names_at_every_depth() {
    for context in [
        r#"{"PRIVATE":1,"\u0050RIVATE":2}"#,
        r#"{"n":{"PRIVATE":1,"PRIVATE":2}}"#,
        r#"{"a":[{"PRIVATE":1,"PRIVATE":2}]}"#,
    ] {
        let request = json(&format!(
            r#"{{"interface":{{"openbindings":"0.2.0","operations":{{}},"sources":{{"s":{{"kind":"example@1"}}}}}},"source":"s","context":{context}}}"#
        ));
        let error = PullRequest::from_json(&request).unwrap_err();
        assert!(matches!(error, RequestError::Shape("duplicate-member")));
        assert!(!format!("{error} {error:?}").contains("PRIVATE"));
    }
}

#[test]
fn request_and_response_boundaries_refuse_before_prospective_work() {
    assert_eq!(PullLimits::default().max_relationship_work, None);
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
            max_relationship_work: Some(0),
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
    let result = PullResult::from_json(json(r#"{"additions":{"operations":{"a":{},"b":{}},"schemas":{"S":true},"bindings":{"a":{"source":"s","operation":"a","data":{"needs":"b"}},"b":{"source":"s","operation":"b","data":{"needs":"S"}}}},"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a","uses":["bindings/b"]},{"binding":"b","operation":"b","uses":["schemas/S"]}]}}"#)).unwrap();
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
fn additions_connected_by_schema_references_pass_without_uses() {
    let request = PullRequest::new(
        checked(r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}}}"#),
        "s",
    );
    for reference in ["#/schemas/A", "urn:example:A"] {
        let value = serde_json::json!({"additions":{
            "operations":{"o":{"input":{"$ref":reference}}},
            "schemas":{"A":{"$id":"urn:example:A","$ref":"urn:example:B"},"B":{"$id":"urn:example:B","type":"string"}},
            "bindings":{"b":{"operation":"o","source":"s"}}
        },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o"}]}});
        let result = PullResult::from_json(json(&value.to_string())).unwrap();
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
    }
}

#[test]
fn binding_and_schema_proposals_refer_to_additions_without_uses() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{"o":{}},"schemas":{"S":true},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"o","source":"s"}}}"#,
        ),
        "s",
    );
    for schema_proposal in [true, false] {
        let mut proposal = serde_json::json!({});
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
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
    }
}

#[test]
fn schema_proposal_unknown_uses_has_no_relationship_meaning() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{},"schemas":{"S":true},"sources":{"s":{"kind":"k"}}}"#,
        ),
        "s",
    );
    for uses in [
        serde_json::json!(42),
        serde_json::json!(["bindings/S", "missing", "missing"]),
    ] {
        let value = serde_json::json!({"additions":{},"report":{"exhaustive":true,"added":[],"existing":[{
            "schema":"S","status":"stale","change":"dangerous","proposal":{"schema":false,"uses":uses}
        }]}});
        let result = PullResult::from_json(json(&value.to_string())).unwrap();
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
    }
}

#[test]
fn a_reference_through_an_existing_schema_needs_no_uses() {
    let request = PullRequest::new(
        checked(
            r##"{"openbindings":"0.2.0","operations":{},"schemas":{"A":{"$ref":"urn:example:B"}},"sources":{"s":{"kind":"k"}}}"##,
        ),
        "s",
    );
    let value = serde_json::json!({"additions":{
        "operations":{"o":{"input":{"$ref":"#/schemas/A"}}},
        "schemas":{"B":{"$id":"urn:example:B","type":"string"}},
        "bindings":{"b":{"operation":"o","source":"s"}}
    },"report":{"exhaustive":true,"added":[{"binding":"b","operation":"o"}]}});
    let result = PullResult::from_json(json(&value.to_string())).unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

fn data_need_fixture(proposal: bool) -> (PullRequest, serde_json::Value) {
    let mut document =
        serde_json::json!({"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}}});
    let mut value = serde_json::json!({"additions":{"schemas":{"ApiError":true}},"report":{"exhaustive":true,"added":[]}});
    if proposal {
        document["operations"] = serde_json::json!({"o":{}});
        document["bindings"] = serde_json::json!({"b":{"source":"s","operation":"o"}});
        value["report"]["existing"] = serde_json::json!([{"binding":"b","status":"stale","change":"dangerous","proposal":{"data":{"errors":"ApiError"}}}]);
    } else {
        value["additions"]["operations"] = serde_json::json!({"o":{}});
        value["additions"]["bindings"] =
            serde_json::json!({"b":{"source":"s","operation":"o","data":{"errors":"ApiError"}}});
        value["report"]["added"] = serde_json::json!([{"binding":"b","operation":"o"}]);
    }
    (PullRequest::new(checked(&document.to_string()), "s"), value)
}
fn needs_entry(value: &mut serde_json::Value, proposal: bool) -> &mut serde_json::Value {
    if proposal {
        &mut value["report"]["existing"][0]["proposal"]
    } else {
        &mut value["report"]["added"][0]
    }
}

#[test]
fn binding_data_uses_are_checked_for_added_entries_and_proposals() {
    for proposal in [false, true] {
        let (request, mut value) = data_need_fixture(proposal);
        needs_entry(&mut value, proposal)["uses"] = serde_json::json!(["schemas/ApiError"]);
        let result = PullResult::from_json(json(&value.to_string())).unwrap();
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
        for label in [
            "schemas/Missing",
            "bindings/b",
            "#/schemas/ApiError",
            "other/ApiError",
        ] {
            let mut invalid = value.clone();
            needs_entry(&mut invalid, proposal)["uses"] = serde_json::json!([label]);
            let result = PullResult::from_json(json(&invalid.to_string())).unwrap();
            assert!(
                matches!(
                    result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false)),
                    Err(PullValidationError::Relationship(RelationshipError::Uses))
                ),
                "{label}"
            );
        }
        needs_entry(&mut value, proposal)["uses"] =
            serde_json::json!(["schemas/ApiError", "schemas/ApiError"]);
        assert!(matches!(
            PullResult::from_json(json(&value.to_string())),
            Err(CarrierError::DuplicateUse)
        ));
    }
}

#[test]
fn published_valid_reports_pass_relationship_and_prospective_checks() {
    let pack = json(include_str!("fixtures/pull-report.json"));
    let mut passed = 0;
    for case in pack.get("tests").unwrap().elements().unwrap() {
        if case.get("schemaValid").unwrap().as_bool() != Some(true) {
            continue;
        }
        let request = PullRequest::new(
            checked(case.get("interface").unwrap().text()),
            case.get("source").unwrap().as_str().unwrap(),
        );
        let value = json(&format!(
            r#"{{"additions":{},"report":{}}}"#,
            case.get("additions").unwrap().text(),
            case.get("report").unwrap().text()
        ));
        let result = PullResult::from_json(value).unwrap();
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap_or_else(|error| {
                panic!("{}: {error:?}", case.get("description").unwrap().text())
            });
        passed += 1;
    }
    assert_eq!(passed, 27);
}

#[test]
fn an_existing_operation_input_reference_passes_without_uses() {
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

#[test]
fn binding_references_name_existing_input_schemas_and_operation_slots() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{"o":{"aliases":["alias"],"input":true,"output":false},"empty":{}},"schemas":{"S":true},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"o","source":"s"}}}"#,
        ),
        "s",
    );
    for (labels, valid) in [
        (
            vec!["schemas/S", "operations/o/input", "operations/o/output"],
            true,
        ),
        (vec!["schemas/missing"], false),
        (vec!["operations/alias/input"], false),
        (vec!["operations/empty/input"], false),
        (vec!["operations/o"], false),
        (vec!["operations/o/other"], false),
        (vec!["bindings/b"], false),
        (vec!["S"], false),
    ] {
        let result = PullResult::from_json(json(
            &serde_json::json!({
                "additions": {}, "report": {"exhaustive":true,"added":[],
                    "existing":[{"binding":"b","status":"current","references":labels}]}
            })
            .to_string(),
        ))
        .unwrap();
        let checked =
            result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
        if valid {
            assert!(checked.is_ok());
        } else {
            assert!(matches!(
                checked,
                Err(PullValidationError::Relationship(
                    RelationshipError::ReferenceTarget
                ))
            ));
        }
    }
}

#[test]
fn omitted_labels_name_input_entries_by_primary_key() {
    let request = PullRequest::new(
        checked(
            r#"{"openbindings":"0.2.0","operations":{"o":{"aliases":["alias"],"input":true,"output":false},"empty":{}},"schemas":{"S":true},"sources":{"s":{"kind":"k"}}}"#,
        ),
        "s",
    );
    for (field, labels, valid) in [
        ("existing", vec!["operations/o"], true),
        ("existing", vec!["schemas/S"], true),
        ("existing", vec!["operations/o/input"], true),
        ("existing", vec!["operations/alias"], false),
        ("existing", vec!["schemas/missing"], false),
        ("existing", vec!["bindings/b"], false),
        ("alsoIdentifies", vec!["operations/o"], true),
        ("alsoIdentifies", vec!["operations/alias"], false),
        ("alsoIdentifies", vec!["operations/o/input"], false),
        ("alsoIdentifies", vec!["schemas/S"], false),
        (
            "through",
            vec!["schemas/S", "operations/o/input", "operations/o/output"],
            true,
        ),
        ("through", vec!["operations/o"], false),
        ("through", vec!["operations/empty/input"], false),
        ("through", vec!["operations/alias/input"], false),
        ("through", vec!["schemas/missing"], false),
    ] {
        let mut omitted = serde_json::json!({"status":"conflict","reasonCode":"shape","message":"", "existing":"operations/o","offered":{}});
        omitted[field] = if field == "existing" {
            serde_json::json!(labels[0])
        } else {
            serde_json::json!(labels)
        };
        let result = PullResult::from_json(json(
            &serde_json::json!({
                "additions":{},"report":{"exhaustive":true,"added":[],"omitted":[omitted]}
            })
            .to_string(),
        ))
        .unwrap();
        let checked =
            result.validate_for(&request, &PullLimits::default(), &AtomicBool::new(false));
        if valid {
            assert!(checked.is_ok(), "{field}");
        } else {
            assert!(
                matches!(
                    checked,
                    Err(PullValidationError::Relationship(
                        RelationshipError::OmittedTarget
                    ))
                ),
                "{field}"
            );
        }
    }
}

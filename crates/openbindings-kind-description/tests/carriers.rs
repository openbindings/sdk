use openbindings::{JsonKind, JsonValue};
use openbindings_kind_description::*;
use serde_json::Value;

fn exact(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn json(value: &Value) -> JsonValue {
    exact(&value.to_string())
}

#[test]
fn all_pinned_carrier_cases_keep_their_published_answers() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/kind-descriptor.json")).unwrap();
    let cases = fixture["tests"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    let mut mismatches = Vec::new();
    for case in cases {
        let valid = match case["operation"].as_str().unwrap() {
            "listKinds" => case["output"].as_array().is_some_and(|values| {
                values
                    .iter()
                    .all(|value| KindSummary::from_json(json(value)).is_ok())
            }),
            "describeKind" => {
                DescribeKindRequest::from_json(json(&case["input"])).is_ok()
                    && KindDescription::from_json(json(&case["output"])).is_ok()
            }
            "checkSource" => {
                CheckSourceRequest::from_json(json(&case["input"])).is_ok()
                    && CheckedSource::from_json(json(&case["output"])).is_ok()
            }
            _ => panic!("unrecognized published operation"),
        };
        if valid != case["schemaValid"].as_bool().unwrap() {
            mismatches.push(case["description"].to_string());
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn open_output_members_and_exact_schema_data_survive_without_reconstruction() {
    let text = r#"{"kind":"example.k@1","description":"kind","supports":[],"sourceData":{"const":9007199254740993},"bindingData":false,"future":{"n":1e999999,"unit":"\ud800"}}"#;
    let description = KindDescription::from_json(exact(text)).unwrap();
    assert_eq!(description.value().value().text(), text);
    assert_eq!(
        description
            .source_data()
            .unwrap()
            .get("const")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    assert_eq!(
        description.binding_data().unwrap().view().as_bool(),
        Some(false)
    );
    assert!(description.check().is_none());
    assert_eq!(description.supports().len(), 0);
    let retained = description.source_data().unwrap();
    drop(description);
    assert_eq!(retained.text(), r#"{"const":9007199254740993}"#);
}

#[test]
fn source_data_absence_null_and_runtime_context_remain_distinct() {
    let absent = CheckSourceRequest::from_json(exact(r#"{"kind":"example.k@1"}"#)).unwrap();
    assert!(absent.data().is_none());
    assert!(absent.context().is_none());
    let text =
        r#"{"kind":"example.k@1","data":null,"context":{"token":9007199254740993,"a":1,"a":2}}"#;
    let request = CheckSourceRequest::from_json(exact(text)).unwrap();
    assert_eq!(request.data().unwrap().kind(), JsonKind::Null);
    assert_eq!(request.value().value().text(), text);
    assert_eq!(
        request
            .context()
            .unwrap()
            .value()
            .value()
            .get("token")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    assert!(
        request
            .context()
            .unwrap()
            .value()
            .value()
            .has_duplicate_names()
    );
    assert_eq!(
        CheckedSource::from_json(exact(text)).unwrap_err(),
        CarrierError::UnexpectedMember
    );
    let source = r#"{"kind":"example.k@1","data":null,"x-proof":{"n":9007199254740993}}"#;
    assert_eq!(
        CheckedSource::from_json(exact(source))
            .unwrap()
            .value()
            .value()
            .text(),
        source
    );
}

#[test]
fn support_is_per_operation_and_repeated_decoded_keys_are_rejected() {
    let summary = KindSummary::from_json(exact(r#"{"kind":"example.k@1","supports":["vendor.operation","openbindings.binding-invoker.invokeBinding"]}"#)).unwrap();
    assert_eq!(
        summary
            .supports()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "vendor.operation",
            "openbindings.binding-invoker.invokeBinding"
        ]
    );
    assert_eq!(
        KindSummary::from_json(exact(r#"{"kind":"example.k@1","supports":["x","\u0078"]}"#))
            .unwrap_err(),
        CarrierError::DuplicateSupport
    );
    assert_eq!(
        KindSummary::from_json(exact(r#"{"kind":"example.k@1","supports":[""]}"#)).unwrap_err(),
        CarrierError::EmptyString("supports")
    );
}

#[test]
fn outer_ambiguity_is_rejected_without_interpreting_opaque_nested_values() {
    assert_eq!(
        DescribeKindRequest::from_json(exact(r#"{"kind":"a","\u006bind":"b"}"#)).unwrap_err(),
        CarrierError::DuplicateMember
    );
    assert_eq!(KindDescription::from_json(exact(r#"{"kind":"k","description":"d","supports":[],"check":{"description":"d","executes":false,"executes":true}}"#)).unwrap_err(), CarrierError::DuplicateMember);
    let data = r#"{"kind":"k","data":{"duplicate":1,"duplicate":2,"properties":{"kind":false}}}"#;
    assert_eq!(
        CheckedSource::from_json(exact(data))
            .unwrap()
            .value()
            .value()
            .text(),
        data
    );
}

#[test]
fn check_disclosure_is_owned_and_is_not_permission() {
    let description = KindDescription::from_json(exact(r#"{"kind":"k","description":"d","supports":["openbindings.kind-descriptor.checkSource"],"check":{"description":"starts a named program","executes":true,"future":null}}"#)).unwrap();
    let check = description.check().unwrap();
    drop(description);
    assert!(check.executes());
    assert_eq!(check.description().as_str(), Some("starts a named program"));
    assert_eq!(
        check.value().value().get("future").unwrap().kind(),
        JsonKind::Null
    );
    assert!(format!("{check:?}").contains("CheckDescription"));
    assert!(!format!("{check:?}").contains("starts"));
}

#[test]
fn explicit_check_claims_and_disclosure_cannot_contradict_each_other() {
    for text in [
        r#"{"kind":"k","description":"d","supports":["openbindings.kind-descriptor.checkSource"]}"#,
        r#"{"kind":"k","description":"d","supports":[],"check":{"description":"reads","executes":false}}"#,
    ] {
        assert_eq!(
            KindDescription::from_json(exact(text)).unwrap_err(),
            CarrierError::InconsistentCheckSupport
        );
    }
    // Absence of a canonical key makes no negative claim about a local operation.
    assert!(KindDescription::from_json(exact(r#"{"kind":"k","description":"d","supports":["vendor.check"],"check":{"description":"reads","executes":false}}"#)).is_ok());
}

#[test]
fn check_rejection_preserves_exact_message_data_and_reserved_challenge_boundary() {
    let text = r#"{"code":"SOURCE_INVALID","message":"bad \ud800","data":{"n":9007199254740993}}"#;
    let rejection = CheckRejection::from_json(exact(text)).unwrap();
    assert_eq!(rejection.value().value().text(), text);
    assert_eq!(rejection.code().as_str(), Some("SOURCE_INVALID"));
    assert_eq!(rejection.message().utf16_units().last(), Some(0xd800));
    assert_eq!(
        rejection.data().unwrap().get("n").unwrap().number_text(),
        Some("9007199254740993")
    );
    assert!(!format!("{rejection:?}").contains("SOURCE_INVALID"));
    assert_eq!(
        CheckRejection::from_json(exact(
            r#"{"code":"CONTEXT_REQUIRED","message":"not a challenge"}"#
        ))
        .unwrap_err(),
        CarrierError::ReservedCode
    );
    let omitted = CheckRejection::from_json(exact(r#"{"code":"E","message":"failed"}"#)).unwrap();
    let null =
        CheckRejection::from_json(exact(r#"{"code":"E","message":"failed","data":null}"#)).unwrap();
    assert!(omitted.data().is_none());
    assert_eq!(null.data().unwrap().kind(), JsonKind::Null);
}

#[test]
fn exact_extension_prefixes_do_not_claim_document_unicode_conformance() {
    let text = r#"{"kind":"k","x-\ud800":{}}"#;
    let source = CheckedSource::from_json(exact(text)).unwrap();
    assert_eq!(source.value().value().text(), text);
    assert_eq!(
        CheckedSource::from_json(exact(r#"{"kind":"k","\ud800x-":{}}"#)).unwrap_err(),
        CarrierError::UnexpectedMember
    );
    let document = openbindings::Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"sources":{{"s":{text}}}}}"#
    ))
    .unwrap();
    assert_eq!(
        document.validate().unwrap().report().evidence["OBI-01"],
        openbindings::Evidence::Violated
    );
}

#[test]
fn pull_and_invocation_disclosures_match_exact_operation_claims() {
    for (field, operation) in [
        ("pull", "openbindings.source-puller.pullSource"),
        ("invoke", "openbindings.binding-invoker.invokeBinding"),
    ] {
        for (supports, disclosure, valid) in [
            (
                vec![operation],
                Some(serde_json::json!({"description":"Reads the named source.","executes":false})),
                true,
            ),
            (vec![operation], None, false),
            (
                vec![operation],
                Some(serde_json::json!({"description":"Reads."})),
                false,
            ),
            (vec!["future.operation"], None, true),
            (
                vec!["future.operation"],
                Some(serde_json::json!({"description":"Reads.","executes":false})),
                false,
            ),
        ] {
            let mut value = serde_json::json!({"kind":"k","description":"d","supports":supports});
            if let Some(disclosure) = disclosure {
                value[field] = disclosure;
            }
            let admitted = KindDescription::from_json(json(&value));
            assert_eq!(admitted.is_ok(), valid, "{value}");
            if valid && supports == [operation] {
                let admitted = admitted.unwrap();
                if field == "pull" {
                    assert!(!admitted.pull().unwrap().executes());
                } else {
                    assert!(!admitted.invoke().unwrap().executes());
                }
            }
        }
    }
}

#[test]
fn a_field_name_several_carriers_share_says_which_one() {
    let missing = |field| CarrierError::MissingField(field);
    let description = |nested: &str| {
        format!(
            r#"{{"kind":"example.k@1","supports":["vendor.operation"],"sourceData":true,"bindingData":true{nested}}}"#
        )
    };
    let root = KindDescription::from_json(exact(&description(""))).unwrap_err();
    let nested = KindDescription::from_json(exact(&description(
        r#","description":"d","check":{"executes":false}"#,
    )))
    .unwrap_err();
    assert_eq!(root, missing("description"));
    assert_eq!(nested, missing("check/description"));
    assert_eq!(nested.to_string(), "the carrier requires check/description");
    let text = exact(r#"{"executes":false}"#);
    assert_eq!(
        PullDescription::from_json(text.clone()).unwrap_err(),
        missing("pull/description")
    );
    assert_eq!(
        InvokeDescription::from_json(text).unwrap_err(),
        missing("invoke/description")
    );
    assert_eq!(
        CheckDescription::from_json(exact(r#"{"description":"d"}"#)).unwrap_err(),
        missing("check/executes")
    );
    assert_eq!(
        CheckDescription::from_json(exact(r#"{"description":"","executes":false}"#)).unwrap_err(),
        CarrierError::EmptyString("check/description")
    );
    assert_eq!(
        PullDescription::from_json(exact(r#"{"description":"d","executes":1}"#)).unwrap_err(),
        CarrierError::FieldType("pull/executes")
    );
}

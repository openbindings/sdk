use openbindings::{JsonKind, JsonValue};
use openbindings_context::{ChallengeShapeError as Error, Context, ContextChallenge};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

#[test]
fn every_pinned_context_challenge_case_has_its_original_shape_answer() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cases.json")).unwrap();
    assert_eq!(corpus["format"], "openbindings.context-challenge-cases@1");
    let cases = corpus["tests"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    for case in cases {
        let raw = json(&case["challenge"].to_string());
        let actual = ContextChallenge::try_from(raw.clone());
        assert_eq!(
            actual.is_ok(),
            case["schemaValid"].as_bool().unwrap(),
            "{}",
            case["description"]
        );
        if let Ok(challenge) = actual {
            assert_eq!(challenge.value().value().text(), raw.text());
        }
    }
}

#[test]
fn checked_fields_use_decoded_names_and_reject_ambiguous_carriers() {
    let cases = [
        ("null", Error::ExpectedObject),
        (r#"{"schema":{}}"#, Error::MissingMessage),
        (r#"{"message":1,"schema":{}}"#, Error::ExpectedMessageString),
        (r#"{"message":"","schema":{}}"#, Error::EmptyMessage),
        (r#"{"message":"why"}"#, Error::MissingSchema),
        (
            r#"{"message":"why","schema":true}"#,
            Error::ExpectedSchemaObject,
        ),
        (
            r#"{"message":"a","\u006dessage":"b","schema":{}}"#,
            Error::DuplicateMember,
        ),
        (
            r#"{"message":"why","schema":{},"\u0073chema":{}}"#,
            Error::DuplicateMember,
        ),
        (
            r#"{"message":"why","schema":{},"x-extra":1}"#,
            Error::UnexpectedMember,
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(
            ContextChallenge::try_from(json(text)).unwrap_err(),
            expected
        );
    }
    let challenge =
        ContextChallenge::try_from(json(r#"{"\u006dessage":"why","\u0073chema":{}}"#)).unwrap();
    assert_eq!(challenge.message().as_str(), Some("why"));
}

#[test]
fn exact_opaque_values_and_schema_owners_survive_the_envelope() {
    let text =
        r#"{"secret":9007199254740993,"large":1e99999,"null":null,"secret":2,"\ud800":"\udfff"}"#;
    let context = Context::try_from(json(text)).unwrap();
    assert_eq!(context.value().value().text(), text);
    assert!(context.value().member("secret").is_err());
    assert_eq!(
        context.value().member("null").unwrap().unwrap().kind(),
        JsonKind::Null
    );
    assert!(context.value().member("absent").unwrap().is_none());
    let retained = context.clone().into_value();
    drop(context);
    assert_eq!(retained.value().text(), text);
    let text = r#"{"message":"\ud800","schema":{"type":42,"type":43,"const":1e99999,"oauth2":{"opaque":"\udfff"}}}"#;
    let challenge = ContextChallenge::try_from(json(text)).unwrap();
    assert_eq!(challenge.message().as_str(), None);
    assert_eq!(
        challenge.message().utf16_units().collect::<Vec<_>>(),
        [0xd800]
    );
    let schema = challenge.schema();
    let envelope = challenge.value().clone();
    drop(challenge);
    assert!(schema.member("type").is_err());
    assert_eq!(schema.member("const").unwrap().unwrap().text(), "1e99999");
    assert_eq!(envelope.value().text(), text);
}

#[test]
fn diagnostic_formatting_contains_no_authored_data() {
    let context = Context::try_from(json(r#"{"PRIVATE_CONTEXT":"SECRET"}"#)).unwrap();
    let challenge = ContextChallenge::try_from(json(
        r#"{"message":"PRIVATE_MESSAGE","schema":{"PRIVATE_SCHEMA":"SECRET"}}"#,
    ))
    .unwrap();
    let error = ContextChallenge::try_from(json(
        r#"{"message":"PRIVATE_MESSAGE","schema":{},"PRIVATE_NAME":"SECRET"}"#,
    ))
    .unwrap_err();
    for formatted in [
        format!("{context:?}"),
        format!("{challenge:?}"),
        format!("{error:?}"),
        error.to_string(),
    ] {
        assert!(!formatted.contains("PRIVATE"));
        assert!(!formatted.contains("SECRET"));
    }
}

#[test]
fn applied_context_authority_is_separate_from_package_version() {
    assert_eq!(openbindings_context::CONTEXT_VERSION, "0.1");
    assert_eq!(
        openbindings_context::APPLIED_INTERFACE_REVISION,
        "3e3dc453a93b611a8e9ea1813ec63a2216830f45"
    );
}

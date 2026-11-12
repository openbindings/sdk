//! Current document/key requests and shared context replace legacy copied carriers.
use openbindings::{Document, JsonLimits, JsonValue};
use openbindings_invocation::{
    BindingRequest, BoundaryFailureKind, ContextChallenge, InvocationError, OperationRequest,
};
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn request(binding_data: &str, source_data: &str, context: &str) -> JsonValue {
    json(&format!(
        r#"{{"interface":{{"openbindings":"0.2.0","operations":{{"echo":{{}}}},"sources":{{"s":{{"kind":"fixture"{source_data}}}}},"bindings":{{"b":{{"operation":"echo","source":"s"{binding_data}}}}}}},"binding":"b"{context}}}"#
    ))
}
#[test]
fn owned_document_request_preserves_exact_payloads_and_owners() {
    let original = request(
        r#", "data":{"n":-0.00e+2,"unit":"\ud800\udc00"},"x-binding":[1e-10000,9007199254740993]"#,
        r#", "data":{"n":9007199254740993,"large":1e9999},"x-source":"\ud800\udc00""#,
        r#", "context":{"secret":null,"opaque":false}"#,
    );
    let proof = Document::from_json(original.get("interface").unwrap().to_owned())
        .validate()
        .unwrap();
    assert!(proof.conformant().is_some(), "{:?}", proof.report());
    let input = BindingRequest::from_json(&original).unwrap();
    let retained = input.interface.source("s").unwrap();
    let wire = input.to_json().unwrap();
    let again = BindingRequest::from_json(&wire).unwrap();
    assert_eq!(
        again.interface.value().text(),
        input.interface.value().text()
    );
    assert_eq!(
        again.context.as_ref().unwrap().value().value().text(),
        r#"{"secret":null,"opaque":false}"#
    );
    assert_eq!(serde_json::to_string(&input).unwrap(), wire.text());
    assert_eq!(
        input.interface.binding("b").unwrap().data().unwrap().text(),
        r#"{"n":-0.00e+2,"unit":"\ud800\udc00"}"#
    );
    drop((original, input, wire, again));
    assert_eq!(
        retained.data().unwrap().text(),
        r#"{"n":9007199254740993,"large":1e9999}"#
    );
}
#[test]
fn data_and_context_absence_are_distinct_from_null_and_empty_object() {
    for data in ["", r#", "data":null"#] {
        for context in ["", r#", "context":{}"#] {
            let input = BindingRequest::from_json(&request(data, data, context)).unwrap();
            let wire = input.to_json().unwrap();
            assert_eq!(
                wire.at("/interface/bindings/b/data").is_some(),
                !data.is_empty()
            );
            assert_eq!(
                wire.at("/interface/sources/s/data").is_some(),
                !data.is_empty()
            );
            assert_eq!(wire.get("context").is_some(), !context.is_empty());
            assert!(wire.at("/interface/sources/s/content").is_none());
        }
    }
}
#[test]
fn requests_reject_closed_shape_violations_and_require_document_conformance() {
    let good = request("", "", "");
    let doc = good.get("interface").unwrap().text();
    for payload in [
        "null".into(),
        "{}".into(),
        format!(r#"{{"interface":{doc}}}"#),
        format!(r#"{{"interface":{doc},"binding":"b","binding":"b"}}"#),
        format!(r#"{{"interface":{doc},"binding":"b","source":{{}}}}"#),
        format!(r#"{{"interface":{doc},"binding":"b","context":null}}"#),
        format!(r#"{{"interface":{doc},"binding":""}}"#),
        r#"{"interface":{"openbindings":"0.2.0","unknown":0},"binding":"b"}"#.into(),
    ] {
        assert!(
            BindingRequest::from_json(&json(&payload)).is_err(),
            "{payload}"
        );
    }
    for fields in [
        r#", "content":null"#,
        r#", "source":"s""#,
        r#", "selector":"old""#,
    ] {
        assert!(BindingRequest::from_json(&request(fields, "", "")).is_err());
    }
}
#[test]
fn document_preference_is_read_by_core_without_a_second_invocation_parser() {
    for (token, expected) in [
        ("-9007199254740991", Some(-9_007_199_254_740_991)),
        ("900719925474099100e-2", Some(9_007_199_254_740_991)),
        ("9.007199254740991e15", Some(9_007_199_254_740_991)),
        ("1.23000e2", Some(123)),
        ("12300e-2", Some(123)),
        ("-0.000e99999999999999999999999999", Some(0)),
        ("0e-99999999999999999999999999", Some(0)),
        ("1.1", None),
        ("9007199254740991.0000000001", None),
        ("9007199254740992", None),
        ("-9007199254740992", None),
        ("1e100000", None),
        ("1e999999999999999999999999", None),
        ("1e-999999999999999999999999", None),
        ("1e-100000", None),
        ("null", None),
        ("true", None),
        (r#""1""#, None),
    ] {
        let input =
            BindingRequest::from_json(&request(&format!(r#", "preference":{token}"#), "", ""));
        match expected {
            Some(expected) => assert_eq!(
                input.unwrap().interface.binding("b").unwrap().preference(),
                Some(expected)
            ),
            None => assert!(input.is_err(), "{token}"),
        }
    }
}
#[test]
fn current_challenges_use_only_message_and_object_schema() {
    let value = json(
        r#"{"message":"Supply context","schema":{"oneOf":[{"title":"A","required":["a"]}],"x-opaque":{"x":1,"x":2}}}"#,
    );
    let challenge = ContextChallenge::try_from(value.clone()).unwrap();
    let error = InvocationError::challenge(challenge);
    assert_eq!(error.data().unwrap().text(), value.text());
    assert_eq!(
        error.context_required().unwrap().value().value().text(),
        value.text()
    );
    for value in [
        r#"{"message":"","schema":{}}"#,
        r#"{"message":"x","schema":true}"#,
        r#"{"message":"x","schema":{},"extra":null}"#,
        r#"{"target":"old","alternatives":[{"requirements":[]}]}"#,
    ] {
        assert!(InvocationError::new("CONTEXT_REQUIRED", Some(json(value))).is_err());
    }
}
#[test]
fn portable_errors_preserve_unknown_codes_and_absent_null_exact_data() {
    for data in [
        None,
        Some(json("null")),
        Some(json(r#"{"n":9007199254740993,"n":1e9999,"unit":"\ud800"}"#)),
    ] {
        let error = InvocationError::new("private/application-failure", data.clone()).unwrap();
        let encoded = error.to_json().unwrap();
        assert_eq!(
            encoded.view().len(),
            Some(if data.is_some() { 2 } else { 1 })
        );
        assert_eq!(
            encoded.get("data").map(|v| v.text()),
            data.as_ref().map(JsonValue::text)
        );
        let decoded = InvocationError::from_json(&encoded).unwrap();
        assert_eq!(decoded.code(), "private/application-failure");
        assert_eq!(
            decoded.data().map(JsonValue::text),
            data.as_ref().map(JsonValue::text)
        );
        assert!(decoded.context_required().is_none());
        assert_eq!(serde_json::to_string(&error).unwrap(), encoded.text());
    }
    for value in [
        r#"{"code":""}"#,
        r#"{"code":null}"#,
        r#"{"data":null}"#,
        r#"{"code":"x","message":"native exception"}"#,
        r#"{"code":"x","code":"y"}"#,
        r#"{"code":"x","data":1,"data":2}"#,
        r#"{"code":"CONTEXT_REQUIRED"}"#,
        r#"{"code":"CONTEXT_REQUIRED","data":null}"#,
        r#"{"code":"CONTEXT_REQUIRED","data":{}}"#,
    ] {
        assert!(
            InvocationError::from_json(&json(value)).is_err(),
            "admitted {value}"
        );
    }
    assert!(InvocationError::new("", None).is_err());
    assert!(InvocationError::new("CONTEXT_REQUIRED", None).is_err());
    assert!(InvocationError::new("CONTEXT_REQUIRED", Some(json("null"))).is_err());
}

#[test]
fn explicit_encoding_limits_bound_bytes_depth_and_nodes() {
    let error = InvocationError::new("extension", Some(json(r#"{"nested":[[1]]}"#))).unwrap();
    for limits in [
        JsonLimits {
            max_bytes: 10,
            ..JsonLimits::default()
        },
        JsonLimits {
            max_depth: 2,
            ..JsonLimits::default()
        },
        JsonLimits {
            max_nodes: 2,
            ..JsonLimits::default()
        },
    ] {
        assert_eq!(
            error.to_json_with_limits(limits).unwrap_err().kind(),
            BoundaryFailureKind::EncodingLimit
        );
    }
    assert!(error.to_json_with_limits(JsonLimits::default()).is_ok());
}

#[test]
fn debug_redacts_current_carriers_and_fixed_admission_paths() {
    const SECRET: &str = "SENTINEL-CREDENTIAL-DO-NOT-LOG";
    let payload = request("", "", &format!(r#", "context":{{"{SECRET}":"{SECRET}"}}"#));
    let mut input = BindingRequest::from_json(&payload).unwrap();
    input.binding = SECRET.into();
    let error = InvocationError::new(SECRET, Some(payload.clone())).unwrap();
    let diagnostic = BindingRequest::from_json(&json(&format!(r#"{{"{SECRET}":0}}"#))).unwrap_err();
    let challenge = ContextChallenge::try_from(json(&format!(
        r#"{{"message":"{SECRET}","schema":{{"title":"{SECRET}"}}}}"#
    )))
    .unwrap();
    for text in [
        format!("{input:?}"),
        format!("{input:#?}"),
        format!("{error:?}"),
        format!("{diagnostic:?}"),
        diagnostic.to_string(),
        format!("{challenge:?}"),
    ] {
        assert!(!text.contains(SECRET), "{text}");
        assert!(text.len() < 256);
    }
    assert_eq!(error.code(), SECRET);
    assert!(error.to_json().unwrap().text().contains(SECRET));
    let document = Document::from_json(payload.get("interface").unwrap().to_owned());
    assert!(document.validate().unwrap().conformant().is_some());
}

#[test]
fn request_decoding_rejects_duplicate_and_unpaired_document_data() {
    for data in [r#", "data":{"n":1,"n":2}"#, r#", "data":"\ud800""#] {
        let value = request(data, "", "");
        assert_eq!(
            BindingRequest::from_json(&value).unwrap_err().kind(),
            BoundaryFailureKind::InvalidDocument
        );
        let proof = Document::from_json(value.get("interface").unwrap().to_owned())
            .validate()
            .unwrap();
        assert_eq!(
            proof.report().evidence["OBI-01"],
            openbindings::Evidence::Violated
        );
    }
}

#[test]
fn request_decoding_refuses_repeated_context_names_at_every_depth() {
    for context in [
        r#"{"PRIVATE":1,"\u0050RIVATE":2}"#,
        r#"{"n":{"PRIVATE":1,"PRIVATE":2}}"#,
        r#"{"a":[{"PRIVATE":1,"PRIVATE":2}]}"#,
    ] {
        let value = request("", "", &format!(r#", "context":{context}"#));
        for error in [
            BindingRequest::from_json(&value).unwrap_err(),
            OperationRequest::from_json(&value).unwrap_err(),
        ] {
            assert_eq!(error.kind(), BoundaryFailureKind::DuplicateMember);
            assert_eq!(error.path(), "/context");
            assert!(!format!("{error} {error:?}").contains("PRIVATE"));
        }
    }
}

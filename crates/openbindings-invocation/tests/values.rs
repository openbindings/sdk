use openbindings::{Binding, JsonKind, JsonLimits, JsonValue, Preference, Source};
use openbindings_invocation::{
    BindingInvocationInput, BindingRequest, BindingValue, ContextChallenge, InvocationError,
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

fn challenge(
    requirement: &str,
) -> Result<ContextChallenge, openbindings_invocation::BoundaryError> {
    ContextChallenge::new(json(&format!(
        r#"{{"target":"","alternatives":[{{"requirements":[{requirement}]}}]}}"#
    )))
}

#[test]
fn empty_binding_and_optional_presence_are_distinct() {
    let empty = BindingValue::from_json(&json("{}")).unwrap();
    assert!(empty.operation.is_none());
    assert!(empty.source.is_none());
    assert!(empty.content.is_none());
    assert_eq!(empty.to_json().unwrap().text(), "{}");
    assert_eq!(BindingValue::default().to_json().unwrap().text(), "{}");

    let present = BindingValue::from_json(&json(
        r#"{"operation":"","source":"","content":null,"idempotent":false,"deprecated":false}"#,
    ))
    .unwrap();
    assert_eq!(present.operation.as_deref(), Some(""));
    assert_eq!(present.source.as_deref(), Some(""));
    assert_eq!(present.content.as_ref().unwrap().kind(), JsonKind::Null);
    let wire = present.to_json().unwrap();
    assert_eq!(wire.get("operation").unwrap().as_str(), Some(""));
    assert_eq!(wire.get("content").unwrap().kind(), JsonKind::Null);
    assert_eq!(wire.get("idempotent").unwrap().as_bool(), Some(false));

    let converted = BindingValue::from(&Binding::default());
    assert_eq!(converted.operation.as_deref(), Some(""));
    assert_eq!(converted.source.as_deref(), Some(""));
    assert!(converted.content.is_none());
}

#[test]
fn owned_input_preserves_opaque_exact_values_without_normalization() {
    let original = json(
        r#"{"source":{"kind":"private.Kind/v1","content":{"n":9007199254740993,"n":1e9999},"description":"source description","x-source":"\ud800"},"binding":{"content":{"n":-0.00e+2,"n":"\ud800"},"preference":7,"description":"binding description","x-binding":[1e-10000,9007199254740993]},"context":{"secret":null,"secret":false}}"#,
    );
    let input = BindingInvocationInput::from_json(&original).unwrap();
    assert_eq!(input.source.kind, "private.Kind/v1");
    assert_eq!(
        input.source.content.as_ref().unwrap().text(),
        r#"{"n":9007199254740993,"n":1e9999}"#
    );
    assert_eq!(
        input.binding.content.as_ref().unwrap().text(),
        r#"{"n":-0.00e+2,"n":"\ud800"}"#
    );
    assert_eq!(
        input.source.additional_fields["x-source"].text(),
        r#""\ud800""#
    );
    assert_eq!(
        input.binding.extensions["x-binding"].text(),
        "[1e-10000,9007199254740993]"
    );
    assert_eq!(
        input.context.as_ref().unwrap().text(),
        r#"{"secret":null,"secret":false}"#
    );
    assert_eq!(input.binding.preference.unwrap().get(), 7);
    let again = BindingInvocationInput::from_json(&input.to_json().unwrap()).unwrap();
    assert_eq!(
        again.binding.content.unwrap().text(),
        input.binding.content.as_ref().unwrap().text()
    );
    let serde_wire = serde_json::to_string(&input).unwrap();
    assert_eq!(
        json(&serde_wire).get("context").unwrap().text(),
        input.context.as_ref().unwrap().text()
    );
    let retained = input.source.content.unwrap();
    drop(original);
    assert_eq!(retained.get("n").unwrap().text(), "9007199254740993");
}

#[test]
fn source_content_and_context_absence_survive_reencoding() {
    for (suffix, present) in [("", false), (",\"context\":{}", true)] {
        let input = BindingInvocationInput::from_json(&json(&format!(
            r#"{{"source":{{"kind":"x"}},"binding":{{}}{suffix}}}"#
        )))
        .unwrap();
        assert!(input.source.content.is_none());
        assert_eq!(input.context.is_some(), present);
        let encoded = input.to_json().unwrap();
        assert_eq!(encoded.get("context").is_some(), present);
        assert!(encoded.at("/source/content").is_none());
    }
    let input = BindingInvocationInput::from_json(&json(
        r#"{"source":{"kind":"x","content":null},"binding":{}}"#,
    ))
    .unwrap();
    assert_eq!(
        input
            .to_json()
            .unwrap()
            .at("/source/content")
            .unwrap()
            .kind(),
        JsonKind::Null
    );
}

#[test]
fn interpreted_carriers_reject_malformed_duplicate_and_unknown_members() {
    for input in [
        "null",
        "{}",
        r#"{"source":{"kind":"x"}}"#,
        r#"{"source":{"kind":"x"},"binding":{},"extra":0}"#,
        r#"{"source":{"kind":"x"},"binding":{},"binding":{}}"#,
        r#"{"source":[],"binding":{}}"#,
        r#"{"source":{},"binding":{}}"#,
        r#"{"source":{"kind":""},"binding":{}}"#,
        r#"{"source":{"kind":null},"binding":{}}"#,
        r#"{"source":{"kind":"x","kind":"y"},"binding":{}}"#,
        r#"{"source":{"kind":"x","location":"legacy"},"binding":{}}"#,
        r#"{"source":{"kind":"x","description":false},"binding":{}}"#,
        r#"{"source":{"kind":"x","x-e":1,"x-e":2},"binding":{}}"#,
        r#"{"source":{"kind":"x"},"binding":null}"#,
        r#"{"source":{"kind":"x"},"binding":{"operation":false}}"#,
        r#"{"source":{"kind":"x"},"binding":{"source":null}}"#,
        r#"{"source":{"kind":"x"},"binding":{"selector":"legacy"}}"#,
        r#"{"source":{"kind":"x"},"binding":{"content":1,"\u0063ontent":2}}"#,
        r#"{"source":{"kind":"x"},"binding":{"idempotent":0}}"#,
        r#"{"source":{"kind":"x"},"binding":{"deprecated":"false"}}"#,
        r#"{"source":{"kind":"x"},"binding":{"description":{}}}"#,
        r#"{"source":{"kind":"x"},"binding":{"x-e":1,"x-e":2}}"#,
        r#"{"source":{"kind":"x"},"binding":{},"context":null}"#,
        r#"{"source":{"kind":"x"},"binding":{},"context":[]}"#,
        r#"{"source":{"kind":"\ud800"},"binding":{}}"#,
    ] {
        assert!(
            BindingInvocationInput::from_json(&json(input)).is_err(),
            "admitted {input}"
        );
    }
}

#[test]
fn native_carriers_cannot_hide_illegal_or_shadowed_fields_in_extensions() {
    for name in ["kind", "content", "location", "credential-secret"] {
        let mut source = Source {
            kind: "test".into(),
            ..Source::default()
        };
        source.additional_fields.insert(name.into(), json("null"));
        let input = BindingInvocationInput {
            source,
            binding: BindingValue::default(),
            context: None,
        };
        assert!(input.to_json().is_err());
    }
    for name in ["operation", "content", "source", "preference", "selector"] {
        let mut binding = BindingValue::default();
        binding.extensions.insert(name.into(), json("null"));
        assert!(binding.to_json().is_err());
        assert!(serde_json::to_string(&binding).is_err());
    }
}

#[test]
fn preferences_use_exact_integer_math_and_the_interoperable_range() {
    for (token, expected) in [
        ("-9007199254740991", -9_007_199_254_740_991),
        ("900719925474099100e-2", 9_007_199_254_740_991),
        ("9.007199254740991e15", 9_007_199_254_740_991),
        ("1.23000e2", 123),
        ("12300e-2", 123),
        ("-0.000e99999999999999999999999999", 0),
        ("0e-99999999999999999999999999", 0),
    ] {
        let binding =
            BindingValue::from_json(&json(&format!(r#"{{"preference":{token}}}"#))).unwrap();
        assert_eq!(binding.preference, Preference::new(expected), "{token}");
    }
    for token in [
        "1.1",
        "9007199254740991.0000000001",
        "9007199254740992",
        "-9007199254740992",
        "1e100000",
        "1e-100000",
        "1e999999999999999999999999",
        "1e-999999999999999999999999",
        "null",
        "true",
        "\"1\"",
    ] {
        assert!(
            BindingValue::from_json(&json(&format!(r#"{{"preference":{token}}}"#))).is_err(),
            "{token}"
        );
    }
}

#[test]
fn unknown_requirement_families_and_durable_presence_are_preserved() {
    for flag in ["", ",\"durable\":false", ",\"durable\":true"] {
        let requirement = format!(
            r#"{{"type":"private.approval","name":"consent","opaque":{{"n":9007199254740993,"n":"\ud800"}}{flag}}}"#
        );
        let challenge = challenge(&requirement).unwrap();
        assert_eq!(challenge.target(), "");
        let requirement_value = challenge
            .alternatives()
            .element(0)
            .unwrap()
            .at("/requirements/0")
            .unwrap();
        assert_eq!(requirement_value.text(), requirement);
        assert_eq!(requirement_value.get("durable").is_some(), !flag.is_empty());
        let error = InvocationError::challenge(challenge);
        assert_eq!(error.code(), "CONTEXT_REQUIRED");
        assert_eq!(
            error.context_required().unwrap().alternatives().len(),
            Some(1)
        );
    }
    // Exact unfamiliar fields need not have Unicode-scalar names.
    assert!(challenge(r#"{"type":"extension","\ud800":1}"#).is_ok());
}

#[test]
fn config_challenges_check_pointer_shape_and_admit_boolean_schemas() {
    for path in [
        "",
        "/",
        "/variables/region",
        "/a~1b/~0key",
        "/value",
        "/00",
        "/~01",
    ] {
        for schema in [
            "true",
            "false",
            r#"{"enum":[9007199254740993],"x":1,"x":2}"#,
        ] {
            let requirement = format!(
                r#"{{"type":"config.value","point":"server","path":"{path}","schema":{schema}}}"#
            );
            assert!(challenge(&requirement).is_ok(), "{requirement}");
        }
    }
    for requirement in [
        r#"{"type":"config.value"}"#,
        r#"{"type":"config.value","point":"server"}"#,
        r#"{"type":"config.value","path":""}"#,
        r#"{"type":"config.value","point":false,"path":""}"#,
        r#"{"type":"config.value","point":"server","path":null}"#,
        r#"{"type":"config.value","point":"server","path":"variables"}"#,
        r#"{"type":"config.value","point":"server","path":"/~2"}"#,
        r#"{"type":"config.value","point":"server","path":"/~"}"#,
        r#"{"type":"config.value","point":"server","path":"","schema":null}"#,
        r#"{"type":"config.value","point":"server","path":"","schema":[]}"#,
    ] {
        assert!(challenge(requirement).is_err(), "admitted {requirement}");
    }
}

#[test]
fn challenges_reject_closed_shape_violations_and_ambiguous_requirements() {
    for value in [
        "null",
        r#"{"target":"","alternatives":[]}"#,
        r#"{"target":"","alternatives":[{"requirements":[]}]}"#,
        r#"{"target":"","alternatives":[{"requirements":[{"type":"x"}],"extra":0}]}"#,
        r#"{"target":"","alternatives":[{"requirements":[{"type":"x"}]}],"extra":0}"#,
        r#"{"target":"","target":"other","alternatives":[{"requirements":[{"type":"x"}]}]}"#,
        r#"{"target":false,"alternatives":[{"requirements":[{"type":"x"}]}]}"#,
        r#"{"target":"","alternatives":[{"requirements":[{"type":"x"}],"requirements":[{"type":"y"}]}]}"#,
    ] {
        assert!(
            ContextChallenge::new(json(value)).is_err(),
            "admitted {value}"
        );
    }
    for requirement in [
        "null",
        "{}",
        r#"{"type":""}"#,
        r#"{"type":7}"#,
        r#"{"type":"x","type":"y"}"#,
        r#"{"type":"x","name":""}"#,
        r#"{"type":"x","description":false}"#,
        r#"{"type":"x","durable":null}"#,
        r#"{"type":"x","custom":1,"custom":2}"#,
        r#"{"type":"x","\ud800":1,"\ud800":2}"#,
    ] {
        assert!(challenge(requirement).is_err(), "admitted {requirement}");
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
            error.to_json_with_limits(limits).unwrap_err().code(),
            "encoding-limit"
        );
    }
    assert!(error.to_json_with_limits(JsonLimits::default()).is_ok());
}

#[test]
fn diagnostics_and_debug_do_not_reveal_strings_values_or_field_names() {
    const SECRET: &str = "SENTINEL-CREDENTIAL-DO-NOT-LOG";
    let source = Source {
        kind: SECRET.into(),
        description: Some(SECRET.into()),
        content: Some(json(&format!(r#"{{"{SECRET}":"{SECRET}"}}"#))),
        ..Source::default()
    };
    let mut binding = BindingValue {
        operation: Some(SECRET.into()),
        source: Some(SECRET.into()),
        description: Some(SECRET.into()),
        content: source.content.clone(),
        ..BindingValue::default()
    };
    binding
        .extensions
        .insert(format!("x-{SECRET}"), JsonValue::string(SECRET).unwrap());
    let context = json(&format!(r#"{{"{SECRET}":"{SECRET}"}}"#));
    let request = BindingRequest {
        source: &source,
        binding: &binding,
        context: Some(&context),
    };
    let challenge = ContextChallenge::new(json(&format!(r#"{{"target":"{SECRET}","alternatives":[{{"requirements":[{{"type":"{SECRET}","description":"{SECRET}"}}]}}]}}"#))).unwrap();
    // RO-7: codes are displayed; sensitive values belong in data, not the code.
    let error = InvocationError::new("example.application", Some(context.clone())).unwrap();
    let diagnostic = BindingValue::from_json(&context).unwrap_err();
    let input = BindingInvocationInput {
        source: source.clone(),
        binding: binding.clone(),
        context: Some(context.clone()),
    };
    for text in [
        format!("{binding:?}"),
        format!("{request:?}"),
        format!("{challenge:?}"),
        format!("{error:?}"),
        error.to_string(),
        format!("{diagnostic:?}"),
        diagnostic.to_string(),
        format!("{input:?}"),
    ] {
        assert!(!text.contains(SECRET), "leaked {text}");
        assert!(text.len() < 256);
    }
}

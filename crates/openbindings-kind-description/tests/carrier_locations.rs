use openbindings::{JsonValue, Pointer};
use openbindings_kind_description::{
    CarrierError, CarrierErrorKind as Kind, CheckDescription, CheckRejection, CheckSourceRequest,
    CheckedSource, InvokeDescription, KindDescription, KindSummary, PullDescription,
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn assert_location(error: CarrierError, kind: Kind, pointer: &str) {
    fn is_copy<T: Copy>(_: T) {}
    is_copy(error.kind());
    assert_eq!(error.kind(), kind);
    assert_eq!(error.pointer().and_then(Pointer::as_str), Some(pointer));
    assert_eq!(error.clone(), error);
    assert!(std::error::Error::source(&error).is_none());
    assert!(!format!("{error} {error:?}").contains("PRIVATE_DOCUMENT_VALUE"));
    assert!(!error.to_string().contains(pointer) || pointer.is_empty());
}

#[test]
fn decoder_locations_use_only_fixed_fields_and_support_indices() {
    for (error, kind, pointer) in [
        (KindSummary::from_json(json("null")).unwrap_err(), Kind::ExpectedObject, ""),
        (KindSummary::from_json(json(r#"{"PRIVATE_DOCUMENT_VALUE":1,"PRIVATE_DOCUMENT_VALUE":2}"#)).unwrap_err(), Kind::DuplicateMember, ""),
        (CheckedSource::from_json(json(r#"{"PRIVATE_DOCUMENT_VALUE":null}"#)).unwrap_err(), Kind::UnexpectedMember, ""),
        (KindSummary::from_json(json(r#"{"kind":"PRIVATE_DOCUMENT_VALUE"}"#)).unwrap_err(), Kind::MissingField("supports"), "/supports"),
        (KindSummary::from_json(json(r#"{"kind":"k","supports":null}"#)).unwrap_err(), Kind::FieldType("supports"), "/supports"),
        (KindSummary::from_json(json(r#"{"kind":"k","supports":["a","", "c"]}"#)).unwrap_err(), Kind::EmptyString("supports"), "/supports/1"),
        (KindSummary::from_json(json(r#"{"kind":"k","supports":["a","b", "a"]}"#)).unwrap_err(), Kind::DuplicateSupport, "/supports/2"),
        (CheckRejection::from_json(json(r#"{"code":"CONTEXT_REQUIRED","message":"PRIVATE_DOCUMENT_VALUE"}"#)).unwrap_err(), Kind::ReservedCode, "/code"),
        (KindDescription::from_json(json(r#"{"kind":"k","description":"d","supports":["openbindings.kind-descriptor.checkSource"]}"#)).unwrap_err(), Kind::InconsistentCheckSupport, "/check"),
        (KindDescription::from_json(json(r#"{"kind":"k","description":"d","supports":["openbindings.source-puller.pullSource"]}"#)).unwrap_err(), Kind::InconsistentPullSupport, "/pull"),
        (KindDescription::from_json(json(r#"{"kind":"k","description":"d","supports":["openbindings.binding-invoker.invokeBinding"]}"#)).unwrap_err(), Kind::InconsistentInvokeSupport, "/invoke"),
        (CheckSourceRequest::from_json(json(r#"{"kind":"k","context":{"PRIVATE_DOCUMENT_VALUE":1,"PRIVATE_DOCUMENT_VALUE":2}}"#)).unwrap_err(), Kind::DuplicateMember, "/context"),
    ] { assert_location(error, kind, pointer); }
}

#[test]
fn standalone_and_nested_disclosures_locate_fields_relative_to_the_admitted_carrier() {
    assert_location(
        CheckDescription::from_json(json(r#"{"executes":false}"#)).unwrap_err(),
        Kind::MissingField("check/description"),
        "/description",
    );
    assert_location(KindDescription::from_json(json(r#"{"kind":"k","description":"d","supports":["openbindings.kind-descriptor.checkSource"],"check":{"description":"PRIVATE_DOCUMENT_VALUE"}}"#)).unwrap_err(), Kind::MissingField("check/executes"), "/check/executes");
    assert_location(
        PullDescription::from_json(json(r#"{"description":"d","executes":null}"#)).unwrap_err(),
        Kind::FieldType("pull/executes"),
        "/executes",
    );
    assert_location(
        InvokeDescription::from_json(json(r#"{"description":""}"#)).unwrap_err(),
        Kind::EmptyString("invoke/description"),
        "/description",
    );
    for field in ["pull", "invoke"] {
        let operation = if field == "pull" {
            "openbindings.source-puller.pullSource"
        } else {
            "openbindings.binding-invoker.invokeBinding"
        };
        let text = format!(
            r#"{{"kind":"k","description":"d","supports":["{operation}"],"{field}":{{"description":"d","executes":null}}}}"#
        );
        let error = KindDescription::from_json(json(&text)).unwrap_err();
        assert_eq!(
            error.pointer().and_then(Pointer::as_str),
            Some(format!("/{field}/executes").as_str())
        );
    }
}

#[test]
fn public_category_labels_are_never_formatted_and_conversion_has_no_location() {
    for kind in [
        Kind::MissingField("PRIVATE_CALLER_LABEL"),
        Kind::FieldType("PRIVATE_CALLER_LABEL"),
        Kind::EmptyString("PRIVATE_CALLER_LABEL"),
        Kind::InputLimit,
    ] {
        let error = CarrierError::from(kind);
        assert_eq!(error.kind(), kind);
        assert!(error.pointer().is_none());
        for diagnostic in [
            format!("{kind}"),
            format!("{kind:?}"),
            format!("{error}"),
            format!("{error:?}"),
        ] {
            assert!(!diagnostic.contains("PRIVATE_CALLER_LABEL"));
        }
    }
}

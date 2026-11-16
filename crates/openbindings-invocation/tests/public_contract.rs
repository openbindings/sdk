use openbindings::JsonValue;
use openbindings_invocation::*;

#[test]
fn operation_keys_match_every_pinned_interface_operation() {
    let document = JsonValue::parse(include_str!("fixtures/binding-invoker-0.1.json")).unwrap();
    let operations = document.get("operations").unwrap();
    let keys = [INVOKE_BINDING, PREFLIGHT_BINDING];
    assert_eq!(operations.members().unwrap().len(), keys.len());
    for key in keys {
        assert!(
            operations.get(key).is_some(),
            "operation constant must name a pinned operation"
        );
    }
}

#[test]
fn context_shape_errors_are_available_from_the_role_crate() {
    let context: ContextShapeError =
        Context::try_from(JsonValue::parse("null").unwrap()).unwrap_err();
    assert_eq!(context, ContextShapeError::ExpectedObject);
    let challenge: ChallengeShapeError =
        ContextChallenge::try_from(JsonValue::parse("{}").unwrap()).unwrap_err();
    assert_eq!(challenge, ChallengeShapeError::MissingMessage);
}

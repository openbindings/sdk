use openbindings::JsonValue;
use openbindings_kind_description::{CheckFailure, CheckRejection, ContextChallenge, KindError};
use std::{error::Error, rc::Rc};

fn challenge() -> ContextChallenge {
    ContextChallenge::try_from(
        JsonValue::parse(r#"{"message":"private","schema":{"x-exact":9007199254740993}}"#).unwrap(),
    )
    .unwrap()
}

#[test]
fn check_mapping_consumes_only_the_native_error_without_clone_or_static_bounds() {
    let text = String::from("borrowed error");
    let native = Rc::new(text.as_str());
    let captured = Box::new(7);
    let mapped =
        CheckFailure::Operational(native.clone()).map_error(move |error| (error, captured));
    let CheckFailure::Operational((error, captured)) = mapped else {
        panic!("operational failure")
    };
    assert!(Rc::ptr_eq(&native, &error));
    assert_eq!(*captured, 7);
}

#[test]
fn check_mapping_preserves_exact_challenges_rejections_and_cancellation() {
    let challenge = challenge();
    let value = challenge.value().value().text().to_owned();
    let mapped = CheckFailure::<()>::ContextRequired(challenge)
        .map_error(|_| -> String { panic!("native errors only") });
    let CheckFailure::ContextRequired(challenge) = mapped else {
        panic!("challenge")
    };
    assert_eq!(challenge.value().value().text(), value);
    let rejection = CheckRejection::from_json(JsonValue::parse(r#"{"code":"private","message":"private","data":{"n":9007199254740993,"units":"\ud800"}}"#).unwrap()).unwrap();
    let value = rejection.value().value().text().to_owned();
    let mapped = CheckFailure::<()>::Rejected(rejection)
        .map_error(|_| -> String { panic!("native errors only") });
    let CheckFailure::Rejected(rejection) = mapped else {
        panic!("rejection")
    };
    assert_eq!(rejection.value().value().text(), value);
    assert!(matches!(
        CheckFailure::<()>::Cancelled.map_error(|_| -> String { panic!("native errors only") }),
        CheckFailure::Cancelled
    ));
}

#[test]
fn kind_mapping_preserves_local_failures_and_composes_with_source_check_mapping() {
    for error in [
        KindError::<()>::Cancelled,
        KindError::KindMismatch,
        KindError::LimitExceeded("response-bytes"),
    ] {
        let before = format!("{error:?}");
        let mapped = error.map_error(|_| -> String { panic!("provider failures only") });
        assert_eq!(format!("{mapped:?}"), before);
    }
    let error = KindError::Provider(CheckFailure::Operational(std::io::Error::other("private")));
    let mapped = error.map_error(|failure| failure.map_error(Box::new));
    assert!(
        mapped
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<Box<std::io::Error>>()
            .is_some()
    );
    assert!(!format!("{mapped:?}: {mapped}").contains("private"));
    let mapped = KindError::Provider(CheckFailure::<()>::ContextRequired(challenge()))
        .map_error(|failure| failure.map_error(|_| -> String { panic!("native errors only") }));
    assert!(matches!(
        mapped,
        KindError::Provider(CheckFailure::ContextRequired(_))
    ));
}

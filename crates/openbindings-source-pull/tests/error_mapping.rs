use openbindings::{Document, Edit, JsonValue};
use openbindings_source_pull::{ContextChallenge, PullError, PullFailure, PullValidationError};
use std::{error::Error, rc::Rc};

#[test]
fn native_mapping_consumes_a_move_only_closure_and_preserves_borrowed_errors() {
    let text = String::from("borrowed failure");
    let native = Rc::new(text.as_str());
    let captured = Box::new(7);
    let mapped = PullError::Provider(PullFailure::Operational(native.clone()))
        .map_error(move |error| (error, captured));
    let PullError::Provider(PullFailure::Operational((error, captured))) = mapped else {
        panic!("native error")
    };
    assert!(Rc::ptr_eq(&native, &error));
    assert_eq!(*captured, 7);
    let mapped = PullFailure::Operational(std::io::Error::other("private")).map_error(Box::new);
    assert!(
        mapped
            .source()
            .unwrap()
            .downcast_ref::<Box<std::io::Error>>()
            .is_some()
    );
    assert!(!format!("{mapped:?}: {mapped}").contains("private"));
}

#[test]
fn challenges_and_both_cancellation_layers_pass_through_without_mapping() {
    let challenge = ContextChallenge::try_from(
        JsonValue::parse(r#"{"message":"private","schema":{"x-exact":9007199254740993}}"#).unwrap(),
    )
    .unwrap();
    let value = challenge.value().value().text().to_owned();
    let mapped = PullError::Provider(PullFailure::<()>::ContextRequired(challenge))
        .map_error(|_| -> String { panic!("native errors only") });
    let PullError::Provider(PullFailure::ContextRequired(challenge)) = mapped else {
        panic!("challenge")
    };
    assert_eq!(challenge.value().value().text(), value);
    assert!(matches!(
        PullFailure::<()>::Cancelled.map_error(|_| -> String { panic!("native errors only") }),
        PullFailure::Cancelled
    ));
    assert!(matches!(
        PullError::<()>::Provider(PullFailure::Cancelled)
            .map_error(|_| -> String { panic!("native errors only") }),
        PullError::Provider(PullFailure::Cancelled)
    ));
    assert!(matches!(
        PullError::<()>::Cancelled.map_error(|_| -> String { panic!("native errors only") }),
        PullError::Cancelled
    ));
}

#[test]
fn local_validation_payloads_are_moved_without_reclassification() {
    for validation in [
        PullValidationError::MissingSource,
        PullValidationError::Cancelled,
        PullValidationError::LimitExceeded("response-bytes"),
    ] {
        let before = format!("{validation:?}");
        let mapped = PullError::<()>::Validation(validation)
            .map_error(|_| -> String { panic!("native errors only") });
        let PullError::Validation(validation) = mapped else {
            panic!("validation")
        };
        assert_eq!(format!("{validation:?}"), before);
    }
}

#[test]
fn validation_mapping_preserves_document_evidence_and_error_causes() {
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"bindings":{"b":{"operation":"missing","source":"missing"}}}"#).unwrap();
    let evidence = document.validate().unwrap();
    assert!(evidence.conformant().is_none());
    let mapped = PullError::<()>::Validation(PullValidationError::Prospective(evidence))
        .map_error(|_| -> String { panic!("native errors only") });
    let PullError::Validation(PullValidationError::Prospective(evidence)) = mapped else {
        panic!("prospective evidence")
    };
    assert_eq!(
        evidence.document().unwrap().as_bytes().as_ptr(),
        document.as_bytes().as_ptr()
    );

    let edit = document.edit(&[Edit::remove(&["absent"])]).unwrap_err();
    let code = edit.code();
    let mapped = PullError::<std::io::Error>::Validation(PullValidationError::Edit(edit))
        .map_error(Box::new);
    let PullError::Validation(PullValidationError::Edit(edit)) = mapped else {
        panic!("edit cause")
    };
    assert_eq!(edit.code(), code);
}

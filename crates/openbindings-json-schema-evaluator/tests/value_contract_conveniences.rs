use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, value_contracts};
use std::{error::Error, sync::Arc};

fn document(schema: &str) -> Conformant<Document> {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"lookup":{{"aliases":["find"],"input":{schema}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .expect("conformant fixture")
}

fn contract(schema: &str) -> ValueContract {
    value_contracts(&document(schema))
        .prepare("find", Direction::Input)
        .into_result()
        .unwrap()
}

#[test]
fn default_context_matches_explicit_policy_and_retains_ready_work() {
    for (schema, input) in [
        (r#"{"type":"integer"}"#, "7"),
        (r#"{"type":"integer"}"#, r#""seven""#),
        (r#"{"$ref":"https://example.invalid/missing"}"#, "7"),
        (r#"{"pattern":"x"}"#, r#""x""#),
    ] {
        let proof = document(schema);
        let default = value_contracts(&proof);
        let explicit =
            proof.value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
        let ready = default
            .prepare("find", Direction::Input)
            .into_result()
            .unwrap();
        let expected = explicit
            .prepare("find", Direction::Input)
            .into_result()
            .unwrap();
        drop(default);
        drop(explicit);
        drop(proof);
        assert_eq!(
            serde_json::to_value(ready.validate_json(input).unwrap()).unwrap(),
            serde_json::to_value(expected.validate(&JsonValue::parse(input).unwrap())).unwrap(),
        );
    }
    let ValueOutcome::Fails {
        message,
        problems,
        problems_complete,
    } = contract("false").validate_json("null").unwrap()
    else {
        panic!("false schema must fail");
    };
    assert_eq!(message, "value does not satisfy the contract");
    assert!(problems.is_empty());
    assert!(!problems_complete);

    // The default pattern byte budget is applied before compilation.
    let pattern = "x".repeat(1_048_577);
    let proof = document(&serde_json::json!({"pattern": pattern}).to_string());
    let error = value_contracts(&proof)
        .prepare("find", Direction::Input)
        .into_result()
        .unwrap_err();
    assert!(
        matches!(error, ValueContractPreparationError::NoVerdict { detail }
        if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "pattern-byte-limit")
    );
}

#[test]
fn json_validation_returns_all_four_outcomes_after_admission() {
    assert!(matches!(
        contract("true").validate_json(b"7").unwrap(),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contract("false").validate_json("7").unwrap(),
        ValueOutcome::Fails { .. }
    ));
    let undefined = contract(r##"{"$id":"https://example.invalid/root","$ref":"#missing"}"##);
    assert!(
        matches!(undefined.validate_json("7").unwrap(), ValueOutcome::Undefined { detail }
        if detail.code == "static-reference-defect")
    );
    let missing = contract(r#"{"$ref":"https://example.invalid/missing"}"#);
    assert!(
        matches!(missing.validate_json("7").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::ResourceUnavailable)
    );
}

#[test]
fn json_validation_preserves_duplicate_members_and_exact_numbers() {
    for text in [r#"{"a":1,"a":2}"#, r#"{"nested":{"a":1,"\u0061":2}}"#] {
        assert!(
            matches!(contract("true").validate_json(text).unwrap(), ValueOutcome::NoVerdict { detail }
            if detail.code == "duplicate-instance-members")
        );
    }
    for (schema, satisfies, fails) in [
        (
            r#"{"const":9007199254740993}"#,
            "9007199254740993",
            "9007199254740992",
        ),
        (
            r#"{"minimum":0.10000000000000000001}"#,
            "0.10000000000000000001",
            "0.1",
        ),
        (r#"{"const":1e400}"#, "10e399", "9e399"),
    ] {
        let ready = contract(schema);
        assert!(matches!(
            ready.validate_json(satisfies).unwrap(),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            ready.validate_json(fails).unwrap(),
            ValueOutcome::Fails { .. }
        ));
    }
}

#[test]
fn json_validation_returns_original_admission_errors() {
    let ready = contract("true");
    let deep = format!("{}0{}", "[".repeat(10_001), "]".repeat(10_001));
    for (bytes, kind) in [
        (&b"\xff"[..], InputErrorKind::InvalidUtf8),
        (&b"\xef\xbb\xbfnull"[..], InputErrorKind::ByteOrderMark),
        (&b"{"[..], InputErrorKind::Syntax),
        (&b"1 2"[..], InputErrorKind::Syntax),
        (deep.as_bytes(), InputErrorKind::Limit),
    ] {
        let expected = JsonValue::parse(bytes).unwrap_err();
        assert_eq!(expected.kind, kind);
        assert_eq!(ready.validate_json(bytes).unwrap_err(), expected);
    }
    assert!(matches!(
        ready.validate_json("null").unwrap(),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn preparation_result_preserves_selection_reasons_and_payloads() {
    let context = value_contracts(&document("false"));
    let absent = context
        .prepare("find", Direction::Output)
        .into_result()
        .unwrap_err();
    assert!(matches!(
        absent,
        ValueContractPreparationError::NoValueContract
    ));
    assert_eq!(absent.to_string(), "operation has no value contract");
    assert!(absent.source().is_none());
    let missing = context
        .prepare("missing", Direction::Input)
        .into_result()
        .unwrap_err();
    assert!(matches!(
        missing,
        ValueContractPreparationError::OperationMissing
    ));
    assert_eq!(missing.to_string(), "operation is missing");
    assert!(missing.source().is_none());
    let candidates = vec!["a".to_owned(), "b".to_owned()];
    let ambiguous = ValueContractPreparation::OperationAmbiguous {
        candidates: candidates.clone(),
    }
    .into_result()
    .unwrap_err();
    assert!(
        matches!(&ambiguous, ValueContractPreparationError::OperationAmbiguous { candidates: actual }
        if actual == &candidates)
    );
    assert_eq!(ambiguous.to_string(), "operation is ambiguous");
    assert!(ambiguous.source().is_none());
    let ready = context
        .prepare("find", Direction::Input)
        .into_result()
        .unwrap();
    assert!(matches!(
        ready.validate_json("null").unwrap(),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn preparation_result_preserves_every_refusal_and_error_source() {
    for reason in [
        NoVerdictReason::UnsupportedCapability,
        NoVerdictReason::ConservativePreparation,
        NoVerdictReason::ResourceUnavailable,
        NoVerdictReason::LimitExceeded,
        NoVerdictReason::Cancelled,
        NoVerdictReason::EvaluatorFailure,
    ] {
        let location = SchemaLocation {
            resource: Some("https://example.invalid/schema".into()),
            pointer: "/input".into(),
        };
        let detail = NoVerdict {
            reason,
            code: "custom-code".into(),
            message: "custom message".into(),
            location: Some(location.clone()),
        };
        let error = ValueContractPreparation::NoVerdict { detail }
            .into_result()
            .unwrap_err();
        let ValueContractPreparationError::NoVerdict { detail } = &error else {
            panic!("refusal lost")
        };
        assert_eq!(detail.reason, reason);
        assert_eq!(detail.code, "custom-code");
        assert_eq!(detail.message, "custom message");
        assert_eq!(detail.location.as_ref(), Some(&location));
        let source = error.source().unwrap().downcast_ref::<NoVerdict>().unwrap();
        assert!(std::ptr::eq(source, detail));
        assert_eq!(
            error.to_string(),
            "value contract preparation refused: custom message"
        );
    }
}

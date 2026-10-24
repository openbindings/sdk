use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
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

/// Prepare the input contract selected by its alias with the default evaluator.
fn prepare(schema: &str) -> PreparedValueContract {
    document(schema)
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
        .unwrap()
}

#[test]
fn default_preparation_matches_explicit_policy_and_retains_ready_work() {
    for (schema, input) in [
        (r#"{"type":"integer"}"#, "7"),
        (r#"{"type":"integer"}"#, r#""seven""#),
        (r#"{"pattern":"x"}"#, r#""x""#),
    ] {
        let proof = document(schema);
        let operation = proof.resolve_operation("find").expect("declared alias");
        let contract = operation.input().expect("declared input");
        let ready = contract.prepare(&DefaultEvaluator::new()).unwrap();
        let expected = contract
            .prepare_with(&DefaultEvaluator::new(), &ResourceSet::default())
            .unwrap();
        drop(operation);
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
    } = prepare("false").validate_json("null").unwrap()
    else {
        panic!("false schema must fail");
    };
    assert_eq!(message, "value does not satisfy the value contract");
    assert!(problems.is_empty());
    assert!(!problems_complete);

    // The default pattern byte budget is applied before compilation.
    let pattern = "x".repeat(1_048_577);
    let proof = document(&serde_json::json!({"pattern": pattern}).to_string());
    let error = proof
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
        .unwrap_err();
    assert!(error.reason == NoVerdictReason::LimitExceeded && error.code == "pattern-byte-limit");
}

// An evaluator whose every value has an undefined result.
struct Undefined;
impl SchemaEvaluator for Undefined {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Undefined))
    }
}
impl PreparedSchema for Undefined {
    fn validate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Undefined {
            detail: UndefinedResult::new("fixture-undefined", "every value is undefined"),
        }
    }
}
#[test]
fn json_validation_returns_all_four_outcomes_after_admission() {
    assert!(matches!(
        prepare("true").validate_json(b"7").unwrap(),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepare("false").validate_json("7").unwrap(),
        ValueOutcome::Fails { .. }
    ));
    let capped = DefaultEvaluator::with_limits(openbindings_json_schema_evaluator::Limits {
        evaluation_steps: Some(0),
        ..Default::default()
    });
    let proof = document(r#"{"items":{"type":"integer"}}"#);
    let operation = proof.resolve_operation("find").expect("declared alias");
    let input = operation.input().expect("declared input");
    let limited = input.prepare(&capped).unwrap();
    assert!(
        matches!(limited.validate_json("[7]").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::LimitExceeded)
    );
    let undefined = input.prepare(&Undefined).unwrap();
    assert!(
        matches!(undefined.validate_json("7").unwrap(), ValueOutcome::Undefined { detail }
        if detail.code == "fixture-undefined")
    );
    // A value reaching a missing resource gives no verdict.
    let missing = document(r#"{"$ref":"https://example.invalid/missing"}"#)
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
        .unwrap();
    assert!(
        matches!(missing.validate_json("7").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::ResourceUnavailable)
    );
}

#[test]
fn json_validation_preserves_duplicate_members_and_exact_numbers() {
    for text in [r#"{"a":1,"a":2}"#, r#"{"nested":{"a":1,"\u0061":2}}"#] {
        assert!(
            matches!(prepare("true").validate_json(text).unwrap(), ValueOutcome::NoVerdict { detail }
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
        let ready = prepare(schema);
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
    let ready = prepare("true");
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
    // A limit names parsing with limits and then validating, since this call
    // takes none.
    assert_eq!(
        ready.validate_json(&deep).unwrap_err().to_string(),
        "the input nests deeper than the admitted depth limit at byte 10000; parse the value \
         with JsonValue::parse_with_limits and a larger JsonLimits::max_depth, then validate \
         it (parseJson with a larger limits.maxDepth in TypeScript)"
    );
    assert!(matches!(
        ready.validate_json("null").unwrap(),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn selection_distinguishes_absent_contracts_missing_and_ambiguous_operations() {
    let proof = document("false");
    let operation = proof.resolve_operation("find").expect("declared alias");
    assert!(operation.output().is_none());
    assert!(proof.resolve_operation("missing").is_none());
    // Conformance excludes ambiguity; a plain document still reports its candidates.
    let plain = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["shared"]},"b":{"aliases":["shared"]}}}"#,
    )
    .unwrap();
    assert!(plain.validate().unwrap().conformant().is_none());
    let candidates = vec!["a".to_owned(), "b".to_owned()];
    assert!(
        matches!(plain.resolve_operation("shared").unwrap(), OperationSelection::Ambiguous { candidates: actual }
        if actual == candidates)
    );
    let ready = operation
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
        .unwrap();
    assert!(matches!(
        ready.validate_json("null").unwrap(),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn preparation_refusal_is_the_original_record_and_propagates_as_an_error() {
    for reason in [
        NoVerdictReason::UnsupportedCapability,
        NoVerdictReason::ConservativePreparation,
        NoVerdictReason::ResourceUnavailable,
        NoVerdictReason::LimitExceeded,
        NoVerdictReason::Cancelled,
        NoVerdictReason::EvaluatorFailure,
    ] {
        let location = SchemaLocation::new(
            Some("https://example.invalid/schema".into()),
            "/input".parse().unwrap(),
        );
        let detail =
            NoVerdict::new(reason, "custom-code", "custom message").with_location(location.clone());
        let proof = document("true");
        let operation = proof.resolve_operation("find").expect("declared alias");
        let contract = operation.input().expect("declared input");
        let error = contract.prepare(&Refuse(detail)).unwrap_err();
        assert_eq!(error.reason, reason);
        assert_eq!(error.code, "custom-code");
        assert_eq!(error.message, "custom message");
        assert_eq!(error.location.as_ref(), Some(&location));
        // Propagating the refusal as an error keeps the original record.
        let propagated = (|| -> Result<PreparedValueContract, Box<dyn Error>> {
            Ok(contract.prepare(&Refuse(error.clone()))?)
        })()
        .unwrap_err();
        let detail = propagated.downcast_ref::<NoVerdict>().unwrap();
        assert_eq!(detail.reason, reason);
        assert_eq!(detail.code, "custom-code");
        assert_eq!(detail.message, "custom message");
        assert_eq!(detail.location.as_ref(), Some(&location));
        assert!(propagated.source().is_none());
        assert_eq!(propagated.to_string(), "custom message");
    }
}

/// Refuse every preparation with one caller-chosen record.
struct Refuse(NoVerdict);
impl SchemaEvaluator for Refuse {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Err(self.0.clone())
    }
}

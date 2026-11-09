use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::error::Error;

fn document(schema: &str) -> ConformantDocument {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"lookup":{{"aliases":["find"],"input":{schema}}}}}}}"#
    ))
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .expect("conformant fixture")
}

/// Prepare the input schema selected by its alias with the default evaluator.
fn prepare(schema: &str) -> PreparedSchema<openbindings_json_schema_evaluator::DefaultValidator> {
    document(schema)
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
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
        let ready = contract
            .prepare(
                &DefaultEvaluator::new(),
                &openbindings::ExternalSchemas::default(),
                &openbindings::PreparationLimits::default(),
            )
            .unwrap();
        let expected = contract
            .prepare(
                &DefaultEvaluator::new(),
                &ExternalSchemas::default(),
                &openbindings::PreparationLimits::default(),
            )
            .unwrap();
        drop(operation);
        drop(proof);
        assert_eq!(
            serde_json::to_value(ready.evaluate_json(input).unwrap()).unwrap(),
            serde_json::to_value(expected.evaluate(&JsonValue::parse(input).unwrap())).unwrap(),
        );
    }
    let ValueOutcome::Fails {
        message,
        problems,
        problems_complete,
    } = prepare("false").evaluate_json("null").unwrap()
    else {
        panic!("false schema must fail");
    };
    assert_eq!(message, "value does not satisfy the schema");
    assert!(problems.is_empty());
    assert!(!problems_complete);

    // Preparation retains a local source-limit interpretation. Only a string
    // reaching that pattern requires its unavailable matching program.
    let pattern = "x".repeat(1_048_577);
    let proof = document(&serde_json::json!({"pattern": pattern}).to_string());
    let limited = proof
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        limited.evaluate_json("null").unwrap(),
        ValueOutcome::Satisfies
    ));
    assert!(
        matches!(limited.evaluate_json("\"x\"").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "pattern-byte-limit")
    );
}

// An evaluator whose every value has an undefined result.
struct Undefined;
impl SchemaEvaluator for Undefined {
    type Validator = Undefined;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Ok(Undefined)
    }
}
impl SchemaValidator for Undefined {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Undefined {
            detail: UndefinedResult::new("fixture-undefined", "every value is undefined"),
        }
    }
}
#[test]
fn json_validation_returns_all_four_outcomes_after_admission() {
    assert!(matches!(
        prepare("true").evaluate_json(b"7").unwrap(),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepare("false").evaluate_json("7").unwrap(),
        ValueOutcome::Fails { .. }
    ));
    let capped = DefaultEvaluator::with_limits(openbindings_json_schema_evaluator::Limits {
        evaluation_sdk_work_units: Some(0),
        ..Default::default()
    });
    let proof = document(r#"{"items":{"type":"integer"}}"#);
    let operation = proof.resolve_operation("find").expect("declared alias");
    let input = operation.input().expect("declared input");
    let limited = input
        .prepare(
            &capped,
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(limited.evaluate_json("[7]").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::LimitExceeded)
    );
    let undefined = input
        .prepare(
            &Undefined,
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(undefined.evaluate_json("7").unwrap(), ValueOutcome::Undefined { detail }
        if detail.code == "fixture-undefined")
    );
    // A value reaching a missing resource gives no verdict.
    let missing = document(r#"{"$ref":"https://example.invalid/missing"}"#)
        .resolve_operation("find")
        .expect("declared alias")
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(
        matches!(missing.evaluate_json("7").unwrap(), ValueOutcome::NoVerdict { detail }
        if detail.reason == NoVerdictReason::ResourceUnavailable)
    );
}

#[test]
fn json_validation_preserves_duplicate_members_and_exact_numbers() {
    for text in [r#"{"a":1,"a":2}"#, r#"{"nested":{"a":1,"\u0061":2}}"#] {
        assert!(
            matches!(prepare("true").evaluate_json(text).unwrap(), ValueOutcome::NoVerdict { detail }
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
            ready.evaluate_json(satisfies).unwrap(),
            ValueOutcome::Satisfies
        ));
        assert!(matches!(
            ready.evaluate_json(fails).unwrap(),
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
        assert_eq!(expected.kind(), kind);
        assert_eq!(ready.evaluate_json(bytes).unwrap_err(), expected);
    }
    // A limit names parsing with limits and then validating, since this call
    // takes none.
    assert_eq!(
        ready.evaluate_json(&deep).unwrap_err().to_string(),
        "the input nests deeper than the admitted depth limit at byte 10000; parse the value \
         with JsonValue::parse_with_limits and a larger JsonLimits::max_depth, then validate \
         it (parseJson with a larger limits.maxDepth in TypeScript)"
    );
    assert!(matches!(
        ready.evaluate_json("null").unwrap(),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn selection_distinguishes_absent_contracts_missing_and_ambiguous_operations() {
    let proof = document("false");
    let operation = proof.resolve_operation("find").expect("declared alias");
    assert!(operation.output().is_none());
    assert!(proof.resolve_operation("missing").is_none());
    // Conformance excludes ambiguity; exact draft declarations remain inspectable.
    let plain = Document::parse(
        r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["shared"]},"b":{"aliases":["shared"]}}}"#,
    )
    .unwrap();
    assert!(plain.validate().unwrap().conformant().is_none());
    assert_eq!(
        plain.validate().unwrap().report().evidence["OBI-05"],
        Evidence::Violated
    );
    for key in ["a", "b"] {
        assert_eq!(
            plain
                .value()
                .get("operations")
                .unwrap()
                .get(key)
                .unwrap()
                .get("aliases")
                .unwrap()
                .text(),
            r#"["shared"]"#
        );
    }
    let ready = operation
        .input()
        .expect("declared input")
        .prepare(
            &DefaultEvaluator::new(),
            &openbindings::ExternalSchemas::default(),
            &openbindings::PreparationLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        ready.evaluate_json("null").unwrap(),
        ValueOutcome::Fails { .. }
    ));
}

#[test]
fn preparation_refusal_is_the_original_record_and_propagates_as_an_error() {
    for kind in [
        PreparationFailureKind::UnsupportedCapability {
            capability: "custom".into(),
        },
        PreparationFailureKind::Resolution {
            cause: ResolutionFailureKind::ResourceUnavailable {
                uri: "https://example.invalid/missing".into(),
            },
        },
        PreparationFailureKind::Limit {
            resource: "custom".into(),
        },
        PreparationFailureKind::Cancelled,
        PreparationFailureKind::InvalidExternalSchema,
        PreparationFailureKind::Implementation {
            message: "custom message".into(),
        },
    ] {
        let location = SchemaOrigin::new(
            Some("https://example.invalid/schema".into()),
            "/input".parse().unwrap(),
        );
        let expected = PreparationFailure::new(kind).with_origin(location);
        let proof = document("true");
        let operation = proof.resolve_operation("find").unwrap();
        let contract = operation.input().unwrap();
        let error = contract
            .prepare(
                &Refuse(expected.clone()),
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
            )
            .unwrap_err();
        assert_eq!(error, expected);
        let propagated = (|| -> Result<PreparedSchema<Undefined>, Box<dyn Error>> {
            Ok(contract.prepare(
                &Refuse(error.clone()),
                &ExternalSchemas::default(),
                &PreparationLimits::default(),
            )?)
        })()
        .unwrap_err();
        assert_eq!(
            propagated.downcast_ref::<PreparationFailure>().unwrap(),
            &expected
        );
        assert!(propagated.source().is_none());
    }
}

/// Refuse every preparation with one caller-chosen record.
struct Refuse(PreparationFailure);
impl SchemaEvaluator for Refuse {
    type Validator = Undefined;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Err(self.0.clone())
    }
}

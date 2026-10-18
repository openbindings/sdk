use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

/// A prepared value contract checked with one detail per call: the cheap check or an explanation.
struct Checked {
    contract: PreparedValueContract,
    detail: Detail,
}
impl Checked {
    fn validate(&self, value: &JsonValue) -> ValueOutcome {
        match self.detail {
            Detail::Outcome => self.contract.validate(value),
            _ => self.contract.explain(value),
        }
    }
    fn validate_json(&self, input: &str) -> Result<ValueOutcome, InputError> {
        Ok(self.validate(&JsonValue::parse(input)?))
    }
}
fn prepare(schema: &str, mode: Detail, resources: ResourceSet) -> Checked {
    let document = Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap();
    let validation = document.validate().unwrap();
    let proof = validation.conformant().expect("conformant schema");
    let operation = proof.resolve_operation("op").expect("declared operation");
    let contract = operation.input().expect("declared input");
    match contract.prepare_with(&DefaultEvaluator::new(), &resources) {
        Ok(contract) => Checked {
            contract,
            detail: mode,
        },
        Err(detail) => panic!("{detail:?}"),
    }
}

#[test]
fn every_numeric_family_uses_exact_values_in_both_detail_modes() {
    let cases = [
        (r#"{"type":"integer"}"#, "1.0", true),
        (r#"{"type":"integer"}"#, "1e400", true),
        (r#"{"type":"integer"}"#, "1e-200000", false),
        (r#"{"type":"integer"}"#, "-0e999999999999999999999999", true),
        (r#"{"type":"integer"}"#, "1.000000000000000000001", false),
        (r#"{"type":["integer","string"]}"#, "1e-400", false),
        (r#"{"maximum":9007199254740992}"#, "9007199254740993", false),
        (r#"{"minimum":1e400}"#, "9e399", false),
        (r#"{"exclusiveMinimum":1.0}"#, "1", false),
        (r#"{"exclusiveMaximum":1e400}"#, "9e399", true),
        (r#"{"maximum":-0}"#, "0e99999999999999999999999", true),
        (r#"{"multipleOf":0.1}"#, "0.3", true),
        (r#"{"multipleOf":0.1}"#, "0.30000000000000004", false),
        (r#"{"multipleOf":1e-400}"#, "2e-400", true),
        (
            r#"{"const":{"n":[1e400,-0,9007199254740993]}}"#,
            r#"{"n":[10e399,0.0,9.007199254740993e15]}"#,
            true,
        ),
        (r#"{"const":9007199254740993}"#, "9007199254740992", false),
        (r#"{"enum":[{"n":1.0},1e400]}"#, "10e399", true),
        (
            r#"{"uniqueItems":true}"#,
            r#"[{"n":1e400},{"n":10e399}]"#,
            false,
        ),
        (
            r#"{"uniqueItems":true}"#,
            "[-0,0e9999999999999999999999999]",
            false,
        ),
        (
            r#"{"uniqueItems":true}"#,
            "[9007199254740992,9007199254740993]",
            true,
        ),
        (r#"{"minItems":2.0}"#, "[1]", false),
        (r#"{"maxItems":1e400}"#, "[1,2]", true),
        (r#"{"minProperties":1e400}"#, "{}", false),
        (r#"{"maxProperties":-0.0}"#, "{\"x\":1}", false),
        (r#"{"minLength":2}"#, "\"😀\"", false),
        (r#"{"maxLength":1.0}"#, "\"😀\"", true),
        (r#"{"contains":true,"minContains":2.0}"#, "[1]", false),
        (r#"{"contains":true,"minContains":1e400}"#, "[1]", false),
        (
            r#"{"contains":true,"maxContains":1e400,"unevaluatedItems":false}"#,
            "[1,2]",
            true,
        ),
        (
            r#"{"contains":{"type":"integer"},"maxContains":1e400,"unevaluatedItems":false}"#,
            "[1,\"x\"]",
            false,
        ),
        (
            r#"{"contains":true,"maxContains":-0,"minContains":0}"#,
            "[1]",
            false,
        ),
    ];
    for mode in [Detail::Outcome, Detail::Explain] {
        for (schema, input, expected) in cases {
            let contract = prepare(schema, mode, ResourceSet::default());
            let outcome = contract.validate(&JsonValue::parse(input).unwrap());
            match &outcome {
                ValueOutcome::Satisfies => assert!(expected, "{schema}: {input}"),
                ValueOutcome::Fails {
                    problems,
                    problems_complete,
                    ..
                } => {
                    assert!(!expected, "{schema}: {input}");
                    assert!(!problems_complete);
                    if mode == Detail::Outcome {
                        assert!(problems.is_empty());
                    }
                    for problem in problems {
                        assert!(
                            !problem.message.contains(input),
                            "instance in message: {problem:?}"
                        );
                    }
                }
                other => panic!("{schema}: {input}: {other:?}"),
            }
        }
    }
}

#[test]
fn explained_facts_use_original_tokens_and_never_numeric_handles() {
    let huge = format!("1e{}", "9".repeat(100_000));
    for bound in ["1e400", "9007199254740993", "-0.00", huge.as_str()] {
        let schema = format!(r#"{{"exclusiveMinimum":{bound}}}"#);
        let contract = prepare(&schema, Detail::Explain, ResourceSet::default());
        let ValueOutcome::Fails { problems, .. } =
            contract.validate(&JsonValue::parse(bound).unwrap())
        else {
            panic!()
        };
        assert_eq!(problems.len(), 1);
        let problem = &problems[0];
        assert_eq!(problem.code, "exclusiveMinimum");
        assert_eq!(
            problem.schema.pointer,
            "/operations/op/input/exclusiveMinimum"
        );
        assert!(
            matches!(&problem.details, Some(ValueProblemDetails::NumericBound { bound: token }) if token == bound)
        );
    }
    let schema = format!(r#"{{"enum":[1e400,{huge},9007199254740993,-0.00]}}"#);
    let contract = prepare(&schema, Detail::Explain, ResourceSet::default());
    let ValueOutcome::Fails { problems, .. } =
        contract.validate(&JsonValue::parse("-1e400").unwrap())
    else {
        panic!()
    };
    assert!(
        matches!(&problems[0].details, Some(ValueProblemDetails::Enum { choices })
        if choices == &["1e400", huge.as_str(), "9007199254740993", "-0.00"])
    );
}

#[test]
fn substituted_resource_nodes_keep_distinct_original_provenance() {
    let resource = JsonValue::parse(r#"{"$id":"https://exact.test/root","$defs":{"a":{"$anchor":"a","maximum":1e400},"b":{"$id":"nested","maximum":2e400}}}"#).unwrap();
    for mode in [Detail::Outcome, Detail::Explain] {
        for (reference, pointer, bound) in [
            ("https://exact.test/root#a", "/$defs/a/maximum", "1e400"),
            (
                "https://exact.test/root#/%24defs/b",
                "/$defs/b/maximum",
                "2e400",
            ),
            ("https://exact.test/nested", "/$defs/b/maximum", "2e400"),
        ] {
            let resources = ResourceSet::new([SchemaResource {
                uri: "https://exact.test/root".into(),
                document: resource.clone(),
            }])
            .unwrap();
            let contract = prepare(&format!(r#"{{"$ref":"{reference}"}}"#), mode, resources);
            let ValueOutcome::Fails { problems, .. } =
                contract.validate(&JsonValue::parse("3e400").unwrap())
            else {
                panic!()
            };
            if mode == Detail::Explain {
                let location = &problems[0].schema;
                assert_eq!(
                    location.resource.as_deref(),
                    Some("https://exact.test/root")
                );
                assert_eq!(location.pointer, pointer);
                assert!(
                    matches!(&problems[0].details, Some(ValueProblemDetails::NumericBound { bound: token }) if token == bound)
                );
            }
        }
    }
}

#[test]
fn compact_exponents_public_equality_and_arithmetic_refusal_remain_separate() {
    let huge = format!("1e{}", "9".repeat(100_000));
    for (left, right) in [
        ("1e400", "10e399"),
        ("-0", "0.00"),
        ("9007199254740993", "9.007199254740993e15"),
    ] {
        assert_eq!(
            JsonValue::parse(left)
                .unwrap()
                .semantic_eq(&JsonValue::parse(right).unwrap()),
            Some(true)
        );
    }
    for mode in [Detail::Outcome, Detail::Explain] {
        for token in [huge.as_str(), &"7".repeat(4097)] {
            let contract = prepare(r#"{"multipleOf":3}"#, mode, ResourceSet::default());
            let ValueOutcome::NoVerdict { detail } =
                contract.validate(&JsonValue::parse(token).unwrap())
            else {
                panic!()
            };
            assert_eq!(detail.code, "numeric-arithmetic-limit");
            let contract = prepare(r#"{"minimum":0}"#, mode, ResourceSet::default());
            assert!(matches!(
                contract.validate(&JsonValue::parse(token).unwrap()),
                ValueOutcome::Satisfies
            ));
        }
    }
}

#[test]
fn integer_fast_path_edges_preserve_exact_bounds_and_equality() {
    // Pair ordinary i64 tokens with decimal/exponent spellings and immediate
    // neighbours outside i64. The expected order is exact, never f64-derived.
    let tokens = [
        "-9223372036854775809",
        "-9223372036854775808",
        "-9223372036854775807",
        "-9007199254740993",
        "-1",
        "-0",
        "0",
        "1",
        "9007199254740993",
        "9223372036854775806",
        "9223372036854775807",
        "9223372036854775808",
    ];
    for mode in [Detail::Outcome, Detail::Explain] {
        for (i, bound) in tokens.iter().enumerate() {
            for spelling in [
                bound.to_string(),
                format!("{bound}.0"),
                format!("{bound}e0"),
            ] {
                for keyword in [
                    "minimum",
                    "maximum",
                    "exclusiveMinimum",
                    "exclusiveMaximum",
                    "const",
                ] {
                    let contract = prepare(
                        &format!(r#"{{"{keyword}":{spelling}}}"#),
                        mode,
                        ResourceSet::default(),
                    );
                    for (j, input) in tokens.iter().enumerate() {
                        let same = i == j || (i == 5 && j == 6) || (i == 6 && j == 5);
                        let expected = match keyword {
                            "minimum" => j > i || same,
                            "maximum" => j < i || same,
                            "exclusiveMinimum" => j > i && !same,
                            "exclusiveMaximum" => j < i && !same,
                            "const" => same,
                            _ => unreachable!(),
                        };
                        for value in [
                            input.to_string(),
                            format!("{input}.0"),
                            format!("{input}e0"),
                        ] {
                            let outcome = contract.validate_json(&value).unwrap();
                            assert!(
                                matches!(
                                    outcome,
                                    ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
                                ),
                                "{keyword} {spelling}: {value}: {outcome:?}"
                            );
                            assert_eq!(
                                matches!(outcome, ValueOutcome::Satisfies),
                                expected,
                                "{keyword} {spelling}: {value}"
                            );
                        }
                    }
                }
            }
        }
        for (divisor, input, expected) in [
            ("1", "-9223372036854775808", true),
            ("2", "-9223372036854775808", true),
            ("3", "-9223372036854775808", false),
            ("9223372036854775807", "9223372036854775807", true),
            ("9223372036854775807", "-9223372036854775808", false),
            ("9223372036854775808", "-9223372036854775808", true),
            ("2.0", "9223372036854775808", true),
            ("2", "9223372036854775808.0", true),
            ("2", "-0", true),
            ("2", "1e1", true),
            ("2", "2.0000000000000000001", false),
        ] {
            let contract = prepare(
                &format!(r#"{{"multipleOf":{divisor}}}"#),
                mode,
                ResourceSet::default(),
            );
            let outcome = contract.validate_json(input).unwrap();
            assert!(matches!(
                outcome,
                ValueOutcome::Satisfies | ValueOutcome::Fails { .. }
            ));
            assert_eq!(
                matches!(outcome, ValueOutcome::Satisfies),
                expected,
                "{divisor}: {input}"
            );
        }
        let integer = prepare(r#"{"type":"integer"}"#, mode, ResourceSet::default());
        for token in tokens.into_iter().map(str::to_owned).chain([
            "9".repeat(10_000),
            "-0".into(),
            "-0.000e-99999".into(),
            "1e999999".into(),
        ]) {
            assert!(
                matches!(
                    integer.validate_json(&token).unwrap(),
                    ValueOutcome::Satisfies
                ),
                "{token}"
            );
        }
        for token in ["1e-99999", "-0.00000000001", "9223372036854775807.1"] {
            assert!(matches!(
                integer.validate_json(token).unwrap(),
                ValueOutcome::Fails { .. }
            ));
        }
        let unique = prepare(r#"{"uniqueItems":true}"#, mode, ResourceSet::default());
        for (input, expected) in [
            (
                "[-9223372036854775808,9223372036854775807,9223372036854775808]",
                true,
            ),
            ("[1,2,3,9223372036854775807,9223372036854775807.0]", false),
            ("[-0,0e999999]", false),
            ("[9223372036854775808,9223372036854775809]", true),
        ] {
            assert_eq!(
                matches!(
                    unique.validate_json(input).unwrap(),
                    ValueOutcome::Satisfies
                ),
                expected
            );
        }
    }
}

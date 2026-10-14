use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Diagnostics};
use std::sync::Arc;

fn prepare(schema: &str, mode: Diagnostics, resources: ResourceSet) -> ValueContract {
    let document = ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap();
    let validation = document.validate().unwrap();
    let context = validation
        .validated()
        .expect("conformant schema")
        .value_contracts(
            Arc::new(DefaultEvaluator::new().with_diagnostics(mode)),
            resources,
        );
    match context.prepare("op", Direction::Input) {
        ValueContractPreparation::Ready(contract) => contract,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_numeric_family_uses_exact_values_in_both_diagnostic_modes() {
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
    for mode in [Diagnostics::Coarse, Diagnostics::Upstream] {
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
                    if mode == Diagnostics::Coarse {
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
fn upstream_facts_use_original_tokens_and_never_numeric_handles() {
    let huge = format!("1e{}", "9".repeat(100_000));
    for bound in ["1e400", "9007199254740993", "-0.00", huge.as_str()] {
        let schema = format!(r#"{{"exclusiveMinimum":{bound}}}"#);
        let contract = prepare(&schema, Diagnostics::Upstream, ResourceSet::default());
        let ValueOutcome::Fails { problems, .. } =
            contract.validate(&JsonValue::parse(bound).unwrap())
        else {
            panic!()
        };
        assert_eq!(problems.len(), 1);
        let problem = &problems[0];
        assert_eq!(problem.code, "exclusiveMinimum");
        assert_eq!(
            problem.schema_location.as_ref().unwrap().pointer,
            "/operations/op/input/exclusiveMinimum"
        );
        assert!(
            matches!(&problem.details, Some(ValueProblemDetails::NumericBound { bound: token }) if token == bound)
        );
    }
    let schema = format!(r#"{{"enum":[1e400,{huge},9007199254740993,-0.00]}}"#);
    let contract = prepare(&schema, Diagnostics::Upstream, ResourceSet::default());
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
    for mode in [Diagnostics::Coarse, Diagnostics::Upstream] {
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
            if mode == Diagnostics::Upstream {
                let location = problems[0].schema_location.as_ref().unwrap();
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
    for mode in [Diagnostics::Coarse, Diagnostics::Upstream] {
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

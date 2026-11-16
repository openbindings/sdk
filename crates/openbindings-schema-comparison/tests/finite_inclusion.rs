use openbindings::{ExternalSchemas, JsonValue, ValueOutcome, validate_document};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use openbindings_schema_comparison::*;

fn input(text: &str) -> SchemaInput {
    let value = JsonValue::parse(text).unwrap();
    SchemaInput::new(value.clone(), value)
}
fn check(flow: &str, receiver: &str, expected: Verdict) {
    for (direction, target, candidate) in [
        (Direction::Input, flow, receiver),
        (Direction::Output, receiver, flow),
    ] {
        let result = compare(
            direction,
            &input(target),
            &input(candidate),
            &ComparisonLimits::default(),
        )
        .unwrap();
        assert_eq!(
            result.verdict(),
            expected,
            "{direction:?}: {flow} -> {receiver}"
        );
    }
}

#[test]
fn finite_literal_proofs_accept_exact_nested_values_and_union_alternatives() {
    let cases = [
        (
            r#"[1,1.0,9007199254740993]"#,
            r#"{"type":"integer","minimum":1,"maximum":9007199254740993}"#,
        ),
        (
            r#"[1e1000,10e999]"#,
            r#"{"type":"integer","minimum":1e1000,"maximum":1e1000}"#,
        ),
        (
            r#"[-0.0,0e-999999999999999999]"#,
            r#"{"type":"integer","minimum":0,"maximum":0}"#,
        ),
        (
            r#"["😀","é"]"#,
            r#"{"type":"string","minLength":1,"maxLength":1}"#,
        ),
        (r#"["e\u0301"]"#, r#"{"minLength":2,"maxLength":2}"#),
        (
            r#"[1,"x",null]"#,
            r#"{"anyOf":[{"type":"integer"},{"const":"x"},{"type":"null"}]}"#,
        ),
        (
            r#"[{"a":[9007199254740993],"b":true}]"#,
            r#"{"required":["a"],"properties":{"a":{"items":{"const":9007199254740993},"minItems":1,"maxItems":1}},"additionalProperties":{"type":"boolean"}}"#,
        ),
        (r#"[{}]"#, r#"{"properties":{"unused":{"pattern":"x"}}}"#),
        (r#"[[]]"#, r#"{"items":false,"maxItems":0}"#),
        (
            r#"["x"]"#,
            r#"{"$defs":{"unused":{"pattern":"x"}},"type":"string"}"#,
        ),
        (
            r#"["x"]"#,
            r#"{"anyOf":[{"pattern":"never"},{"type":"string"}]}"#,
        ),
        (
            r#"["x"]"#,
            r#"{"anyOf":[{"oneOf":[true,true]},{"type":"string"}]}"#,
        ),
        (
            r#"["x"]"#,
            r#"{"anyOf":[{"allOf":[{"pattern":"never"},{}]},{"type":"string"}]}"#,
        ),
        (
            r#"[1]"#,
            r#"{"minLength":100,"items":false,"required":["absent"]}"#,
        ),
    ];
    for (values, receiver) in cases {
        check(
            &format!(r#"{{"enum":{values}}}"#),
            receiver,
            Verdict::Compatible,
        );
        // Independently check every claimed literal with the public evaluator.
        let doc =
            format!(r#"{{"openbindings":"0.2.0","operations":{{"test":{{"input":{receiver}}}}}}}"#);
        let checked = validate_document(doc.as_bytes()).unwrap();
        let proof = checked.conformant().unwrap();
        let prepared = proof
            .resolve_operation("test")
            .unwrap()
            .input()
            .unwrap()
            .prepare(&DefaultEvaluator::new(), &ExternalSchemas::default())
            .unwrap();
        for value in JsonValue::parse(values).unwrap().view().elements().unwrap() {
            assert!(
                matches!(
                    prepared.evaluate(&value.to_owned()),
                    ValueOutcome::Satisfies
                ),
                "{receiver}: {}",
                value.text()
            );
        }
    }
}

#[test]
fn finite_literal_refusals_preserve_guards_and_boundaries() {
    for (flow, receiver) in [
        (
            r#"{"const":9007199254740993}"#,
            r#"{"maximum":9007199254740992}"#,
        ),
        (
            r#"{"const":1.0000000000000000001}"#,
            r#"{"type":"integer"}"#,
        ),
        (r#"{"const":"😀"}"#, r#"{"minLength":2}"#),
        (r#"{"const":"e\u0301"}"#, r#"{"maxLength":1}"#),
        (r#"{"const":1}"#, r#"{"exclusiveMinimum":1}"#),
        (r#"{"const":1}"#, r#"{"exclusiveMaximum":1}"#),
        (
            r#"{"const":{"a":[1,2]}}"#,
            r#"{"properties":{"a":{"items":{"const":1}}}}"#,
        ),
        (
            r#"{"const":{"a":1,"b":"x"}}"#,
            r#"{"properties":{"a":{}},"additionalProperties":{"type":"integer"}}"#,
        ),
        (r#"{"const":{}}"#, r#"{"required":["a"]}"#),
        (
            r#"{"const":"x"}"#,
            r#"{"anyOf":[{"pattern":"x"},{"type":"number"}]}"#,
        ),
        (
            r#"{"const":"x"}"#,
            r#"{"anyOf":[{"type":"string"}],"maxLength":0}"#,
        ),
        (r#"{"const":1,"type":"string"}"#, r#"{"const":"x"}"#),
        (r#"{"enum":[1,2],"maximum":1}"#, r#"{"const":1}"#),
        (r#"{"const":1,"enum":[2]}"#, r#"{"pattern":"x"}"#),
        (r#"{"const":1,"enum":[2],"pattern":"x"}"#, r#"{}"#),
        (r#"{"const":1,"enum":[2]}"#, r#"{"oneOf":[true]}"#),
        (
            r#"{"const":1,"enum":[2]}"#,
            r#"{"allOf":[{"pattern":"x"},{}]}"#,
        ),
    ] {
        check(flow, receiver, Verdict::Indeterminate);
    }
    check(
        r#"{"const":1,"enum":[2]}"#,
        r#"{"type":"string"}"#,
        Verdict::Compatible,
    );
    check(
        r#"{"const":1,"enum":[1.0,2]}"#,
        r#"{"const":1}"#,
        Verdict::Compatible,
    );
    check(
        r#"{"const":9007199254740993,"enum":[9007199254740992]}"#,
        r#"{"type":"null"}"#,
        Verdict::Compatible,
    );
    check(
        r#"{"enum":[1,2],"maxLength":0}"#,
        r#"{"const":1}"#,
        Verdict::Incompatible,
    );
}

#[test]
fn finite_proofs_refuse_foreign_receivers_and_preserve_literal_data() {
    let flow = input(r#"{"const":"x"}"#);
    let foreign = input(r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"string"}"#);
    assert_eq!(
        compare(
            Direction::Input,
            &flow,
            &foreign,
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Indeterminate
    );
    check(
        r#"{"const":{"anyOf":[1,2]}}"#,
        r#"{"const":{"anyOf":[2,1]}}"#,
        Verdict::Incompatible,
    );
}

#[test]
fn empty_schema_retention_strips_annotations_but_keeps_other_context() {
    for empty in [
        "false",
        r#"{"not":{}}"#,
        r#"{"description":"a","not":{"title":"b"}}"#,
        r#"{"not":true}"#,
    ] {
        let left = input(&format!(
            r#"{{"$defs":{{"d":false}},"properties":{{"p":{empty}}}}}"#
        ));
        let right = input(r#"{"properties":{"p":false}}"#);
        assert!(identical(Some(&left), Some(&right), &ComparisonLimits::default()).unwrap());
    }
    let left = input(r#"{"$defs":{"d":{"pattern":"x"}},"properties":{"p":false}}"#);
    let right = input(r#"{"properties":{"p":false}}"#);
    assert!(!identical(Some(&left), Some(&right), &ComparisonLimits::default()).unwrap());
}

#[test]
fn literal_proof_work_is_cancellable_and_refusal_leaves_inputs_reusable() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CancelAt {
        calls: AtomicUsize,
        at: usize,
    }
    impl openbindings::WorkControl for CancelAt {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.at
        }
    }
    let flow = input(r#"{"const":{"a":[9007199254740993,"😀"]}}"#);
    let receiver = input(
        r#"{"properties":{"a":{"items":{"anyOf":[{"type":"integer","minimum":9007199254740993},{"type":"string","maxLength":1}]}}},"required":["a"]}"#,
    );
    // Complete lazy indexing before measuring the checkpoints of reusable inputs.
    assert_eq!(
        compare(
            Direction::Input,
            &flow,
            &receiver,
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
    let baseline = CancelAt {
        calls: AtomicUsize::new(0),
        at: usize::MAX,
    };
    assert_eq!(
        compare_with_control(
            Direction::Input,
            &flow,
            &receiver,
            &ComparisonLimits::default(),
            &baseline
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
    for at in 0..baseline.calls.load(Ordering::Relaxed) {
        let cancel = CancelAt {
            calls: AtomicUsize::new(0),
            at,
        };
        assert_eq!(
            compare_with_control(
                Direction::Input,
                &flow,
                &receiver,
                &ComparisonLimits::default(),
                &cancel
            )
            .unwrap_err()
            .kind(),
            ComparisonFailureKind::Cancelled
        );
    }
    assert_eq!(
        compare(
            Direction::Input,
            &flow,
            &receiver,
            &ComparisonLimits {
                max_work: Some(0),
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind(),
        ComparisonFailureKind::LimitExceeded("work")
    );
    assert_eq!(
        compare(
            Direction::Input,
            &flow,
            &receiver,
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
}

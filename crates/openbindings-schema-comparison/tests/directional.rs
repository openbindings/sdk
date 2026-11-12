use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_schema_comparison::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn schema(text: &str) -> SchemaInput {
    let value = JsonValue::parse(text).unwrap();
    SchemaInput::new(value.clone(), value)
}
fn doc(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
#[test]
fn reasons_keep_deciding_keywords_nested_paths_and_exact_values() {
    for (direction, target, candidate, reason) in [
        (
            Direction::Input,
            r#"{"const":2}"#,
            r#"{"enum":[1]}"#,
            "enum: target value 2 not in candidate enum",
        ),
        (
            Direction::Output,
            r#"{"const":2}"#,
            r#"{"enum":[1]}"#,
            "const: candidate value 1 does not match target const 2",
        ),
        (
            Direction::Input,
            r#"{"type":"array","items":{"enum":[2,1]}}"#,
            r#"{"type":"array","items":{"enum":[3]}}"#,
            "items: enum: target value 2 not in candidate enum",
        ),
        (
            Direction::Input,
            r#"{"type":"object","properties":{"z":{"const":1},"a":{"const":2}}}"#,
            r#"{"type":"object","properties":{"z":{"const":2},"a":{"const":3}}}"#,
            "properties[\"a\"]: const: candidate const 3 does not match target const 2",
        ),
        (
            Direction::Input,
            r#"{"type":"object"}"#,
            r#"{"type":"object","required":["z","a"]}"#,
            "required: candidate requires \"a\" but target does not",
        ),
        (
            Direction::Input,
            r#"{"type":"number","minimum":0,"exclusiveMinimum":-1}"#,
            r#"{"type":"number","exclusiveMinimum":0}"#,
            "minimum: candidate minimum exclusive 0 is greater than target minimum 0",
        ),
        (
            Direction::Output,
            r#"{"type":"object","additionalProperties":false}"#,
            r#"{"type":"object","properties":{"a":true},"additionalProperties":false}"#,
            "properties[\"a\"]: candidate property not allowed by target",
        ),
        (
            Direction::Input,
            r#"{"anyOf":[{"const":1},{"const":2}]}"#,
            r#"{"oneOf":[{"const":1}]}"#,
            "anyOf: target variant 1 has no compatible candidate variant",
        ),
        (
            Direction::Output,
            r#"{"anyOf":[{"const":1}]}"#,
            r#"{"oneOf":[{"const":1},{"const":2}]}"#,
            "oneOf: candidate variant 1 has no compatible target variant",
        ),
        (
            Direction::Input,
            r#"{"const":{"a":[9007199254740993,"\ud800"]}}"#,
            r#"{"enum":[null]}"#,
            "enum: target value {\"a\":[9007199254740993,\"\\ud800\"]} not in candidate enum",
        ),
    ] {
        let result = compare(
            direction,
            &schema(target),
            &schema(candidate),
            &ComparisonLimits::default(),
        )
        .unwrap();
        assert_eq!(result.verdict(), Verdict::Incompatible);
        assert_eq!(result.reason(), Some(reason));
    }
}
#[test]
fn union_search_preserves_uncertainty_but_a_definitive_match_decides() {
    let target = schema(r#"{"anyOf":[{"type":"string","pattern":"secret"}]}"#);
    for (candidate, expected) in [
        (r#"{"anyOf":[{"type":"string"}]}"#, Verdict::Indeterminate),
        (
            r#"{"anyOf":[{"type":"string"},{"type":"string","pattern":"secret"}]}"#,
            Verdict::Compatible,
        ),
    ] {
        assert_eq!(
            compare(
                Direction::Input,
                &target,
                &schema(candidate),
                &ComparisonLimits::default()
            )
            .unwrap()
            .verdict(),
            expected
        );
    }
}
#[test]
fn interface_reports_sorted_slots_missing_and_failures_without_discarding_operations() {
    let required = doc(
        r#"{"openbindings":"0.2.0","operations":{"z":{},"b":{"aliases":["old"],"output":{"allOf":[{"type":"string"},{"type":"integer"}]},"input":{"type":"number"}},"a":{"output":{"pattern":"secret"},"input":true}}}"#,
    );
    let provided = doc(
        r#"{"openbindings":"0.2.0","operations":{"old":{"output":true,"input":{"type":"string"}},"a":{"output":true}}}"#,
    );
    let report = check_interface(
        &required,
        &provided,
        &|_| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(
        report
            .operations()
            .iter()
            .map(OperationComparison::required)
            .collect::<Vec<_>>(),
        ["a", "b", "z"]
    );
    assert_eq!(report.operations()[1].provided(), Some("old"));
    assert!(report.operations()[0].input().is_none());
    assert_eq!(
        report
            .issues()
            .map(|i| (i.operation(), i.kind()))
            .collect::<Vec<_>>(),
        [
            ("a", InterfaceIssueKind::OutputIndeterminate),
            ("b", InterfaceIssueKind::OutputFailure),
            ("b", InterfaceIssueKind::InputIncompatible),
            ("z", InterfaceIssueKind::Missing),
        ]
    );
    assert_eq!(summary(&report), Err(ComparisonFailure::UnsatisfiableMerge));
    assert_eq!(
        report.issues().nth(1).unwrap().failure(),
        Some(ComparisonFailure::UnsatisfiableMerge)
    );
    let opposite = check_interface(
        &provided,
        &required,
        &|_| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert!(
        opposite
            .operations()
            .iter()
            .any(|o| o.required() == "old" && o.provided() == Some("b"))
    );
    assert_eq!(
        Verdict::collapse([
            Verdict::Incompatible,
            Verdict::Compatible,
            Verdict::Indeterminate
        ]),
        Verdict::Indeterminate
    );
    assert_eq!(Verdict::collapse([]), Verdict::Compatible);
}
#[test]
fn public_diagnostic_formatting_does_not_disclose_report_data() {
    let required =
        doc(r#"{"openbindings":"0.2.0","operations":{"sentinel":{"input":{"const":"sentinel"}}}}"#);
    let provided =
        doc(r#"{"openbindings":"0.2.0","operations":{"sentinel":{"input":{"const":"other"}}}}"#);
    let report = check_interface(
        &required,
        &provided,
        &|_| Some(OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    let result = report.operations()[0].input().unwrap().as_ref().unwrap();
    assert!(result.reason().unwrap().contains("sentinel"));
    for text in [
        format!("{result}"),
        format!("{result:?}"),
        format!("{report:?}"),
        format!("{:?}", report.operations()[0]),
        format!("{:?}", report.issues().next().unwrap()),
    ] {
        assert!(!text.contains("sentinel"));
        assert!(!text.contains("other"));
    }
}
#[test]
fn wide_enum_search_and_later_interface_slots_share_the_finite_allowance() {
    let values = (0..1000).map(|i| i.to_string()).collect::<Vec<_>>();
    let t = format!("{{\"enum\":[{}]}}", values.join(","));
    let c = format!(
        "{{\"enum\":[{}]}}",
        values.into_iter().rev().collect::<Vec<_>>().join(",")
    );
    let limits = ComparisonLimits {
        max_work: 200_000,
        ..Default::default()
    };
    let (target, candidate) = (schema(&t), schema(&c));
    assert_eq!(
        identical(Some(&target), Some(&candidate), &limits),
        Ok(false)
    );
    assert!(matches!(
        compare(Direction::Input, &target, &candidate, &limits),
        Err(ComparisonFailure::LimitExceeded(_))
    ));
    let required = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{"a":{{"output":{c},"input":{t}}},"b":{{"input":true}}}}}}"#
    ));
    let provided = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{"a":{{"output":{t},"input":{c}}},"b":{{"input":true}}}}}}"#
    ));
    let report = check_interface(
        &required,
        &provided,
        &|_| Some(OperationRole::Realized),
        &limits,
    )
    .unwrap();
    assert_eq!(report.operations().len(), 2);
    let failure = report.operations()[0]
        .output()
        .unwrap()
        .as_ref()
        .unwrap_err();
    assert!(matches!(failure, ComparisonFailure::LimitExceeded(_)));
    assert_eq!(report.operations()[0].input().unwrap(), &Err(*failure));
    assert_eq!(report.operations()[1].input().unwrap(), &Err(*failure));
}
struct Control {
    checks: AtomicUsize,
    cancel: usize,
}
impl openbindings::WorkControl for Control {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.cancel
    }
}
#[test]
fn controlled_directional_calls_refuse_resources_and_cancel_without_a_verdict() {
    let target = schema(r#"{"anyOf":[{"type":"number","minimum":1},{"type":"string"}]}"#);
    let candidate = schema(r#"{"anyOf":[{"type":"string"},{"type":"number","minimum":0}]}"#);
    compare(
        Direction::Input,
        &target,
        &candidate,
        &ComparisonLimits::default(),
    )
    .unwrap();
    let baseline = Control {
        checks: AtomicUsize::new(0),
        cancel: usize::MAX,
    };
    assert_eq!(
        compare_with_control(
            Direction::Input,
            &target,
            &candidate,
            &ComparisonLimits::default(),
            &baseline
        )
        .unwrap()
        .verdict(),
        Verdict::Compatible
    );
    let count = baseline.checks.load(Ordering::Relaxed);
    for cancel in [0, count / 2, count - 1] {
        assert_eq!(
            compare_with_control(
                Direction::Input,
                &target,
                &candidate,
                &ComparisonLimits::default(),
                &Control {
                    checks: AtomicUsize::new(0),
                    cancel
                }
            ),
            Err(ComparisonFailure::Cancelled)
        );
    }
    for limits in [
        ComparisonLimits {
            max_work: 1,
            ..Default::default()
        },
        ComparisonLimits {
            max_allocation_bytes: 1,
            ..Default::default()
        },
        ComparisonLimits {
            max_depth: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            compare(Direction::Input, &target, &candidate, &limits),
            Err(ComparisonFailure::LimitExceeded(_))
        ));
    }
    assert_eq!(
        compare_with_control(
            Direction::Input,
            &target,
            &candidate,
            &ComparisonLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(ComparisonFailure::Cancelled)
    );
}

fn summary(report: &InterfaceComparison) -> Result<Verdict, ComparisonFailure> {
    let mut verdicts = Vec::new();
    for operation in report.operations() {
        if operation.is_missing() {
            verdicts.push(Verdict::Incompatible);
        }
        for slot in [operation.output(), operation.input()]
            .into_iter()
            .flatten()
        {
            verdicts.push(slot.as_ref().map_err(|e| *e)?.verdict());
        }
    }
    Ok(Verdict::collapse(verdicts))
}

#[test]
fn every_alias_match_is_checked_in_each_selected_role() {
    let provided = doc(
        r#"{"openbindings":"0.2.0","operations":{"b":{"aliases":["second"],"input":{"type":"number"},"output":{"type":"number"}},"a":{"aliases":["first"],"input":{"type":"number"},"output":{"type":"number"}}}}"#,
    );
    for aliases in [r#"["second","first"]"#, r#"["first","second"]"#] {
        let required = doc(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"z":{{}},"contract":{{"aliases":{aliases},"input":{{"type":"integer"}},"output":{{"type":"integer"}}}},"skip":{{}}}}}}"#
        ));
        let report = check_interface(
            &required,
            &provided,
            &|operation| match operation.key() {
                "contract" => Some(OperationRole::Both),
                "z" => Some(OperationRole::Consumed),
                _ => None,
            },
            &ComparisonLimits::default(),
        )
        .unwrap();
        assert_eq!(
            report
                .operations()
                .iter()
                .map(|o| (o.required(), o.provided(), o.role()))
                .collect::<Vec<_>>(),
            [
                ("contract", Some("a"), OperationRole::Realized),
                ("contract", Some("a"), OperationRole::Consumed),
                ("contract", Some("b"), OperationRole::Realized),
                ("contract", Some("b"), OperationRole::Consumed),
                ("z", None, OperationRole::Consumed),
            ]
        );
        for operation in &report.operations()[..4] {
            let realized = operation.role() == OperationRole::Realized;
            assert_eq!(
                operation.input().unwrap().as_ref().unwrap().verdict(),
                if realized {
                    Verdict::Compatible
                } else {
                    Verdict::Incompatible
                }
            );
            assert_eq!(
                operation.output().unwrap().as_ref().unwrap().verdict(),
                if realized {
                    Verdict::Incompatible
                } else {
                    Verdict::Compatible
                }
            );
        }
    }
    let required = doc(
        r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["first"],"input":{"type":"number"}}}}"#,
    );
    assert_eq!(
        check_interface(
            &required,
            &provided,
            &|_| Some(OperationRole::Realized),
            &ComparisonLimits::default()
        )
        .unwrap()
        .operations()
        .len(),
        1
    );
}

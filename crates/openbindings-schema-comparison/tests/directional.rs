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
        &|_, _| Some(OperationRole::Realized),
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
        &|_, _| Some(OperationRole::Realized),
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
        &|_, _| Some(OperationRole::Realized),
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
        max_work: Some(200_000),
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
        &|_, _| Some(OperationRole::Realized),
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
    assert_eq!(ComparisonLimits::default().max_work, None);
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
            max_work: Some(1),
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

#[test]
fn interface_memory_allows_sequential_slots_but_refuses_one_large_slot() {
    let literal = "x".repeat(8192);
    let contract = serde_json::json!({"enum":[literal]});
    let operations: serde_json::Map<_, _> = (0..32)
        .map(|i| (format!("o{i}"), serde_json::json!({"input":contract})))
        .collect();
    let document =
        doc(&serde_json::json!({"openbindings":"0.2.0","operations":operations}).to_string());
    let limits = ComparisonLimits {
        max_allocation_bytes: 128 * 1024,
        ..Default::default()
    };
    let report = check_interface(
        &document,
        &document,
        &|_, _| Some(OperationRole::Realized),
        &limits,
    )
    .unwrap();
    assert_eq!(report.operations().len(), 32);
    assert_eq!(report.issues().count(), 0);
    let wide = schema(&serde_json::json!({"enum":vec![literal; 32]}).to_string());
    assert_eq!(
        compare(Direction::Input, &wide, &wide, &limits),
        Err(ComparisonFailure::LimitExceeded("allocation-bytes"))
    );
}

#[test]
fn interface_memory_keeps_retained_report_reasons_charged() {
    let document = |letter: &str| {
        let operations: serde_json::Map<_, _> = (0..64)
            .map(|i| {
                (
                    format!("o{i:02}"),
                    serde_json::json!({"input":{"const":letter.repeat(512)}}),
                )
            })
            .collect();
        doc(&serde_json::json!({"openbindings":"0.2.0","operations":operations}).to_string())
    };
    let report = check_interface(
        &document("a"),
        &document("b"),
        &|_, _| Some(OperationRole::Realized),
        &ComparisonLimits {
            max_allocation_bytes: 64 * 1024,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        report.operations()[0]
            .input()
            .unwrap()
            .as_ref()
            .unwrap()
            .verdict(),
        Verdict::Incompatible
    );
    assert_eq!(
        report.operations().last().unwrap().input(),
        Some(&Err(ComparisonFailure::LimitExceeded("allocation-bytes")))
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
            &|operation, _| match operation.key() {
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
                ("contract", Some("a"), Some(OperationRole::Realized)),
                ("contract", Some("a"), Some(OperationRole::Consumed)),
                ("contract", Some("b"), Some(OperationRole::Realized)),
                ("contract", Some("b"), Some(OperationRole::Consumed)),
                ("z", None, None),
            ]
        );
        for operation in &report.operations()[..4] {
            let realized = operation.role() == Some(OperationRole::Realized);
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
            &|_, _| Some(OperationRole::Realized),
            &ComparisonLimits::default()
        )
        .unwrap()
        .operations()
        .len(),
        1
    );
}

#[test]
fn roles_are_selected_once_per_distinct_match_and_once_when_missing() {
    let required = doc(
        r#"{"openbindings":"0.2.0","operations":{"wide":{"aliases":["narrow"],"input":{"type":"integer"}},"gone":{}}}"#,
    );
    let provided = doc(
        r#"{"openbindings":"0.2.0","operations":{"wide":{"input":{"type":"number"}},"narrow":{"input":{"type":"integer","minimum":0}}}}"#,
    );
    let calls = std::cell::RefCell::new(Vec::new());
    let report = check_interface(
        &required,
        &provided,
        &|required, provided| {
            calls.borrow_mut().push((
                required.key().to_owned(),
                provided.map(|p| p.key().to_owned()),
            ));
            Some(match provided.map(|p| p.key()) {
                Some("wide") => OperationRole::Realized,
                Some("narrow") => OperationRole::Consumed,
                _ => OperationRole::Both,
            })
        },
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert_eq!(calls.borrow().len(), 3);
    assert_eq!(
        report
            .operations()
            .iter()
            .map(|o| (o.required(), o.provided(), o.role()))
            .collect::<Vec<_>>(),
        [
            ("gone", None, None),
            ("wide", Some("narrow"), Some(OperationRole::Consumed)),
            ("wide", Some("wide"), Some(OperationRole::Realized)),
        ]
    );
    assert_eq!(
        report.issues().map(|i| i.kind()).collect::<Vec<_>>(),
        [InterfaceIssueKind::Missing]
    );
    for operation in &report.operations()[1..] {
        assert_eq!(
            operation.input().unwrap().as_ref().unwrap().verdict(),
            Verdict::Compatible
        );
    }
    let excluded = check_interface(
        &required,
        &provided,
        &|_, matched| matched.map(|_| OperationRole::Realized),
        &ComparisonLimits::default(),
    )
    .unwrap();
    assert!(excluded.operations().iter().all(|o| !o.is_missing()));
}

#[test]
fn interface_cancellation_including_the_final_slot_returns_no_report() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Control {
        calls: AtomicUsize,
        cancel_at: usize,
    }
    impl openbindings::WorkControl for Control {
        fn is_cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed) >= self.cancel_at
        }
    }
    let document =
        doc(r#"{"openbindings":"0.2.0","operations":{"a":{"input":{"type":"string"}}}}"#);
    let run = |control: &Control| {
        check_interface_with_control(
            &document,
            &document,
            &|_, _| Some(OperationRole::Realized),
            &ComparisonLimits::default(),
            control,
        )
    };
    let counter = Control {
        calls: AtomicUsize::new(0),
        cancel_at: usize::MAX,
    };
    assert!(run(&counter).is_ok());
    for cancel_at in 0..counter.calls.load(Ordering::Relaxed) {
        let control = Control {
            calls: AtomicUsize::new(0),
            cancel_at,
        };
        assert!(matches!(run(&control), Err(ComparisonFailure::Cancelled)));
    }
}

#[test]
fn retained_definitions_do_not_make_the_parent_outside_the_profile() {
    let root = openbindings::JsonValue::parse(r#"{"target":{"type":"object","$defs":{},"properties":{"p":{"pattern":"^x"}}},"candidate":{"type":"object","$defs":{},"properties":{"p":{"pattern":"^x"}},"required":["p"]}}"#).unwrap();
    let context = ComparisonContext::new(root);
    let target = context.schema(context.document().get("target").unwrap().to_owned());
    let candidate = context.schema(context.document().get("candidate").unwrap().to_owned());
    assert_eq!(
        compare(
            Direction::Input,
            &target,
            &candidate,
            &ComparisonLimits::default()
        )
        .unwrap()
        .verdict(),
        Verdict::Incompatible
    );
}

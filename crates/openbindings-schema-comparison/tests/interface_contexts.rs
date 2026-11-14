use openbindings::{ConformantDocument, Document, JsonValue};
use openbindings_schema_comparison::*;
use std::sync::atomic::AtomicBool;

fn doc(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
fn context(document: &ConformantDocument) -> ComparisonContext {
    ComparisonContext::new(document.document().value().clone())
}

#[test]
fn interface_uses_each_callers_base_resources_and_relative_references_in_both_entry_points() {
    let document = doc(
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://schema.test/start"},"output":{"$ref":"https://schema.test/start"}}},"schemas":{"Narrow":{"type":"integer"},"Wide":{"type":"number"}}}"#,
    );
    let required = context(&document)
        .with_base("https://required.test/root")
        .with_resource(
            "https://schema.test/start",
            JsonValue::parse(r#"{"$ref":"row"}"#).unwrap(),
        )
        .with_resource(
            "https://schema.test/row",
            JsonValue::parse(r#"{"$ref":"https://required.test/root#/schemas/Narrow"}"#).unwrap(),
        );
    let provided = context(&document)
        .with_base("https://provided.test/root")
        .with_resource(
            "https://schema.test/start",
            JsonValue::parse(r#"{"$ref":"row"}"#).unwrap(),
        )
        .with_resource(
            "https://schema.test/row",
            JsonValue::parse(r#"{"$ref":"https://provided.test/root#/schemas/Wide"}"#).unwrap(),
        );
    let role = |_: &_, _: Option<&_>| Some(OperationRole::Both);
    let limits = ComparisonLimits::default();
    for report in [
        check_interface(&document, &document, &required, &provided, &role, &limits).unwrap(),
        check_interface_with_control(
            &document,
            &document,
            &required,
            &provided,
            &role,
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap(),
    ] {
        let operations = report.operations();
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0].role(), Some(OperationRole::Realized));
        assert_eq!(
            operations[0].input().unwrap().as_ref().unwrap().verdict(),
            Verdict::Compatible
        );
        assert_eq!(
            operations[0].output().unwrap().as_ref().unwrap().verdict(),
            Verdict::Incompatible
        );
        assert_eq!(operations[1].role(), Some(OperationRole::Consumed));
        assert_eq!(
            operations[1].input().unwrap().as_ref().unwrap().verdict(),
            Verdict::Incompatible
        );
        assert_eq!(
            operations[1].output().unwrap().as_ref().unwrap().verdict(),
            Verdict::Compatible
        );
    }
}

#[test]
fn root_guard_rejects_equal_but_unproved_roots_on_either_side_before_callbacks() {
    let text = r#"{"openbindings":"0.2.0","operations":{}}"#;
    let document = doc(text);
    let correct = context(&document);
    let other = ComparisonContext::new(JsonValue::parse(text).unwrap());
    let role = |_: &_, _: Option<&_>| panic!("mismatched roots must not invoke the callback");
    let limits = ComparisonLimits::default();
    for (required, provided, side) in [
        (&other, &correct, ComparisonSide::Required),
        (&correct, &other, ComparisonSide::Provided),
    ] {
        for error in [
            check_interface(&document, &document, required, provided, &role, &limits).unwrap_err(),
            check_interface_with_control(
                &document,
                &document,
                required,
                provided,
                &role,
                &limits,
                &AtomicBool::new(false),
            )
            .unwrap_err(),
        ] {
            assert_eq!(error.kind(), ComparisonFailureKind::ContextMismatch);
            assert_eq!(error.side(), Some(side));
            assert!(error.origin().is_none());
        }
    }
    let cancelled = check_interface_with_control(
        &document,
        &document,
        &other,
        &other,
        &role,
        &limits,
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert_eq!(cancelled.kind(), ComparisonFailureKind::Cancelled);
    assert!(cancelled.side().is_none());
}

#[test]
fn supplied_context_fold_keeps_one_sticky_work_allowance() {
    let operations = (0..20)
        .map(|i| format!(r#""op{i:02}":{{"input":{{"$ref":"https://fetch.test/schema"}}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let document = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{{operations}}}}}"#
    ));
    let configured = context(&document).with_resource(
        "https://fetch.test/schema",
        JsonValue::parse(r#"{"type":"string"}"#).unwrap(),
    );
    let role = |_: &_, _: Option<&_>| Some(OperationRole::Realized);
    // Warm the caller-owned index so this tests the fold's allowance alone.
    check_interface(
        &document,
        &document,
        &configured,
        &configured,
        &role,
        &ComparisonLimits::default(),
    )
    .unwrap();
    let mut witnessed = false;
    for work in (1000..100_000).step_by(1000) {
        let limits = ComparisonLimits {
            max_work: Some(work),
            ..Default::default()
        };
        let Ok(report) = check_interface(
            &document,
            &document,
            &configured,
            &configured,
            &role,
            &limits,
        ) else {
            continue;
        };
        let slots = report
            .operations()
            .iter()
            .map(|op| op.input().unwrap())
            .collect::<Vec<_>>();
        if slots[0].is_ok() && slots.last().unwrap().is_err() {
            let mut failed = false;
            for slot in slots {
                match slot {
                    Ok(result) => {
                        assert!(!failed);
                        assert_eq!(result.verdict(), Verdict::Compatible);
                    }
                    Err(error) => {
                        failed = true;
                        assert_eq!(error.kind(), ComparisonFailureKind::LimitExceeded("work"));
                    }
                }
            }
            witnessed = true;
            break;
        }
    }
    assert!(witnessed, "a single fold must share work across slots");
}

#[test]
fn retained_failure_locations_remain_charged_after_each_slot() {
    let name = "s".repeat(4096);
    let invalid = JsonValue::parse(format!(
        r##"{{"properties":{{"{name}":{{"$ref":"#/missing"}}}}}}"##
    ))
    .unwrap();
    let operations = (0..100)
        .map(|i| format!(r#""op{i:03}":{{"input":{{"$ref":"https://fetch.test/schema"}}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let document = doc(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{{operations}}}}}"#
    ));
    let configured = context(&document).with_resource("https://fetch.test/schema", invalid);
    let limits = ComparisonLimits {
        max_allocation_bytes: 256 * 1024,
        ..Default::default()
    };
    let report = check_interface(
        &document,
        &document,
        &configured,
        &configured,
        &|_, _| Some(OperationRole::Realized),
        &limits,
    )
    .unwrap();
    let errors = report
        .issues()
        .map(|issue| issue.failure().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 100);
    assert_eq!(errors[0].kind(), ComparisonFailureKind::MissingReference);
    let mut limited = false;
    for error in errors {
        if error.kind() == ComparisonFailureKind::LimitExceeded("allocation-bytes") {
            limited = true;
            assert!(error.origin().is_none());
            assert!(error.side().is_none());
        } else {
            assert!(!limited);
            assert_eq!(error.kind(), ComparisonFailureKind::MissingReference);
            assert!(
                error
                    .origin()
                    .unwrap()
                    .pointer
                    .as_str()
                    .unwrap()
                    .contains(&name)
            );
        }
    }
    assert!(
        limited,
        "retained error pointers must exhaust the allowance"
    );
}

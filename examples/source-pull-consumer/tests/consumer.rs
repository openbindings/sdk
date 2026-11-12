use openbindings::{ConformantDocument, Document, Edit, JsonRef, JsonValue};
use openbindings_source_pull::*;
use source_pull_consumer::{Embedded, Fixed, using_the_result::keep_part};
use std::{
    future::Future,
    sync::atomic::AtomicBool,
    task::{Context as TaskContext, Poll, Waker},
};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn field(value: JsonRef<'_>, key: &str) -> JsonValue {
    value.get(key).unwrap().to_owned()
}
fn document(text: &str) -> ConformantDocument {
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
}
fn complete<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("local fixture did not finish after its explicit yield");
}

#[test]
fn default_checked_pull_completes_a_ten_thousand_item_catalog() {
    let items = (0..10_000)
        .map(|i| format!(r#"{{"key":"unit{i}","operation":{{"input":true,"output":true}},"data":{{"item":"unit{i}"}}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let original = document(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"sources":{{"catalog":{{"kind":"example.embedded@1","data":{{"items":[{items}]}}}}}}}}"#
    ));
    let request = PullRequest::new(original.clone(), "catalog");
    let result =
        complete((Embedded).pull_source(request.clone(), CancellationToken::new())).unwrap();
    assert_eq!(result.report().added().len(), 10_000);
    assert!(result.report().is_complete());
    let adopted = result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(adopted.operations().len(), 10_000);
    assert_eq!(adopted.value().get("bindings").unwrap().len(), Some(10_000));
    assert!(original.operations().is_empty());
    assert!(original.value().get("bindings").is_none());
    let insufficient = PullLimits {
        max_relationship_work: 20_000_000,
        ..PullLimits::default()
    };
    assert!(matches!(
        result.validate_for(&request, &insufficient, &AtomicBool::new(false)),
        Err(PullValidationError::LimitExceeded("relationship-work"))
    ));
}

#[test]
fn schema_annotations_remain_the_authors_under_profile_identity() {
    for (current, offered, expected) in [
        (
            r#"{"type":"string","title":"old"}"#,
            r#"{"type":"string","title":"new"}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"type":"string","title":"old"}"#,
            r#"{"title":"old","type":"string"}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"minimum":1}"#,
            r#"{"minimum":1.0}"#,
            ExistingStatus::Current,
        ),
        (
            r#"{"type":"number","title":"old"}"#,
            r#"{"type":"string","title":"new"}"#,
            ExistingStatus::Stale,
        ),
    ] {
        let original = document(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"a":{{"input":{current}}}}},"sources":{{"catalog":{{"kind":"example.embedded@1","data":{{"items":[{{"key":"a","operation":{{"input":{offered}}},"data":{{"item":"a"}}}}]}}}}}},"bindings":{{"a":{{"operation":"a","source":"catalog","data":{{"item":"a"}}}}}}}}"#
        ));
        let result = complete((Embedded).pull_source(
            PullRequest::new(original, "catalog"),
            CancellationToken::new(),
        ))
        .unwrap();
        let entry = result.report().existing().unwrap().next().unwrap();
        assert_eq!(entry.status(), expected);
        if expected == ExistingStatus::Stale {
            let Some(Proposal::Binding(proposal)) = entry.proposal() else {
                panic!("binding proposal")
            };
            assert_eq!(
                proposal.input().unwrap().get("title").unwrap().as_str(),
                Some("new")
            );
        }
    }
}

#[test]
fn no_data_kind_projects_before_destination_identity_and_does_not_adapt_to_conflicts() {
    let client = Fixed;
    for (operations, expected, complete_pull) in [
        ("{}", "new", true),
        (
            r#"{"echo":{"input":{"allOf":[true,{"type":"string"}]},"output":{"type":"string"},"description":"author-owned"}}"#,
            "reused",
            true,
        ),
        (
            r#"{"localEcho":{"aliases":["echo"],"input":{"type":"string"},"output":{"type":"string"}}}"#,
            "reused",
            true,
        ),
        (
            r#"{"echo":{"input":{"type":"integer"},"output":{"type":"string"}}}"#,
            "conflict",
            true,
        ),
        (r#"{"echo":{"output":{"type":"string"}}}"#, "conflict", true),
    ] {
        let original = document(&format!(
            r#"{{"openbindings":"0.2.0","operations":{operations},"sources":{{"fixed":{{"kind":"example.fixed@1"}}}}}}"#
        ));
        let result = complete(client.pull_source(
            PullRequest::new(original.clone(), "fixed"),
            CancellationToken::new(),
        ))
        .unwrap();
        assert_eq!(result.report().is_complete(), complete_pull);
        match expected {
            "new" => {
                assert!(!result.report().added().next().unwrap().reused());
                assert!(
                    result
                        .additions()
                        .operations()
                        .unwrap()
                        .value()
                        .get("echo")
                        .is_some()
                );
                let adopted = result
                    .validate_for(
                        &PullRequest::new(original.clone(), "fixed"),
                        &PullLimits::default(),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let repeated = complete(
                    client
                        .pull_source(PullRequest::new(adopted, "fixed"), CancellationToken::new()),
                )
                .unwrap();
                assert_eq!(repeated.report().added().len(), 0);
                assert_eq!(
                    repeated
                        .report()
                        .existing()
                        .unwrap()
                        .next()
                        .unwrap()
                        .status(),
                    ExistingStatus::Current
                );
            }
            "reused" => {
                let added = result.report().added().next().unwrap();
                assert!(added.reused());
                let operation = original.resolve_operation("echo").unwrap();
                assert_eq!(added.operation().as_str(), Some(operation.key()));
                assert_eq!(
                    result
                        .additions()
                        .bindings()
                        .unwrap()
                        .value()
                        .get("echo.fixed")
                        .unwrap()
                        .get("operation")
                        .unwrap()
                        .as_str(),
                    Some(operation.key())
                );
                result
                    .validate_for(
                        &PullRequest::new(original.clone(), "fixed"),
                        &PullLimits::default(),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert_eq!(
                    result
                        .additions()
                        .operations()
                        .unwrap()
                        .value()
                        .view()
                        .len(),
                    Some(0)
                );
            }
            _ => {
                assert_eq!(result.report().added().len(), 0);
                assert_eq!(
                    result.report().omitted().unwrap().next().unwrap().status(),
                    OmittedStatus::Conflict
                );
            }
        }
        assert!(original.value().get("bindings").is_none());
        assert!(
            original
                .value()
                .get("sources")
                .unwrap()
                .get("fixed")
                .unwrap()
                .get("data")
                .is_none()
        );
    }
}

#[test]
fn offered_aliases_reuse_one_operation_and_report_multiple_matches_as_conflicts() {
    for (aliases, expected, also) in [
        (r#"["localEcho"]"#, "localEcho", None),
        (r#"["echo","localEcho"]"#, "localEcho", None),
        (r#"["echo","other"]"#, "localEcho", Some("second")),
        (r#"["other","echo"]"#, "second", Some("localEcho")),
    ] {
        let original = document(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"localEcho":{{"aliases":["echo"],"input":true}},"second":{{"aliases":["other"],"input":true}}}},"sources":{{"catalog":{{"kind":"example.embedded@1","data":{{"items":[{{"key":"remote","operation":{{"aliases":{aliases},"input":true}},"data":{{"item":"remote"}}}}]}}}}}}}}"#
        ));
        let request = PullRequest::new(original, "catalog");
        let result =
            complete(Embedded.pull_source(request.clone(), CancellationToken::new())).unwrap();
        if let Some(also) = also {
            assert_eq!(result.report().added().len(), 0);
            let omitted = result.report().omitted().unwrap().next().unwrap();
            assert_eq!(omitted.status(), OmittedStatus::Conflict);
            assert_eq!(
                omitted.existing().unwrap().as_str(),
                Some(format!("operations/{expected}").as_str())
            );
            assert_eq!(
                omitted
                    .also_identifies()
                    .unwrap()
                    .map(|label| label.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                [format!("operations/{also}")]
            );
        } else {
            let added = result.report().added().next().unwrap();
            assert!(added.reused());
            assert_eq!(added.operation().as_str(), Some(expected));
            assert_eq!(
                result
                    .additions()
                    .bindings()
                    .unwrap()
                    .value()
                    .get("remote.catalog")
                    .unwrap()
                    .get("operation")
                    .unwrap()
                    .as_str(),
                Some(expected)
            );
        }
        result
            .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
            .unwrap();
    }
}

#[test]
fn embedded_kind_reports_stale_current_unmatched_and_partial_without_overwriting_authorship() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{"a":{"input":{"type":"integer"},"description":"keep"},"b":{"input":true},"c":{}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"partial":true,"items":[{"key":"a","operation":{"input":{"type":"string"}},"data":{"item":"a","revision":"v2"}},{"key":"b","operation":{"input":true},"data":{"item":"b"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","preference":2,"idempotent":true,"data":{"item":"a","revision":"v1"}},"b":{"operation":"b","source":"catalog","data":{"item":"b"}},"c":{"operation":"c","source":"catalog","data":{"item":"gone"}}}}"#,
    );
    let request = PullRequest::new(original.clone(), "catalog");
    let result =
        complete((Embedded).pull_source(request.clone(), CancellationToken::new())).unwrap();
    assert!(!result.report().is_complete());
    let entries = result.report().existing().unwrap().collect::<Vec<_>>();
    assert_eq!(
        entries.iter().map(|e| e.status()).collect::<Vec<_>>(),
        [
            ExistingStatus::Stale,
            ExistingStatus::Current,
            ExistingStatus::Unmatched
        ]
    );
    let Some(Proposal::Binding(proposal)) = entries[0].proposal() else {
        panic!("binding proposal")
    };
    assert!(proposal.value().value().get("description").is_none());
    // The caller chooses to apply only this proposal, explicitly copying its
    // permitted fields and revalidating. Pulling itself has no such effect.
    let edited = original
        .document()
        .edit(&[
            Edit::set(&["bindings", "a", "data"], proposal.data().unwrap()),
            Edit::set(&["operations", "a", "input"], proposal.input().unwrap()),
        ])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let operation = edited.value().get("operations").unwrap().get("a").unwrap();
    {
        let name = "description";
        assert_eq!(
            field(operation, name).text(),
            field(
                original
                    .value()
                    .get("operations")
                    .unwrap()
                    .get("a")
                    .unwrap(),
                name
            )
            .text()
        );
    }
    for name in ["preference", "idempotent"] {
        let before = original.value().get("bindings").unwrap().get("a").unwrap();
        let after = edited.value().get("bindings").unwrap().get("a").unwrap();
        assert_eq!(field(before, name).text(), field(after, name).text());
    }
    assert_eq!(
        original
            .value()
            .get("operations")
            .unwrap()
            .get("a")
            .unwrap()
            .get("input")
            .unwrap()
            .get("type")
            .unwrap()
            .as_str(),
        Some("integer")
    );
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
}

#[test]
fn a_caller_can_keep_part_of_a_pull_by_following_schema_references() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example.catalog@1"}}}"#,
    );
    let request = PullRequest::new(original.clone(), "s");
    let result = PullResult::from_json(json(r##"{"additions":{"operations":{"a":{"input":{"$ref":"#/schemas/S"}},"b":{"input":{"$ref":"#/schemas/T"}}},"schemas":{"S":{"type":"string"},"T":{"type":"integer"}},"bindings":{"a":{"source":"s","operation":"a"},"b":{"source":"s","operation":"b"}}},"report":{"exhaustive":true,"added":[{"binding":"a","operation":"a"},{"binding":"b","operation":"b"}]}}"##)).unwrap();
    result
        .validate_for(&request, &PullLimits::default(), &AtomicBool::new(false))
        .unwrap();
    // Inspecting or dropping a retained result changes nothing in the original.
    drop(result.clone());
    assert!(original.value().get("schemas").is_none());
    assert_eq!(original.value().get("operations").unwrap().len(), Some(0));
    let additions = result.additions();
    let operation = additions
        .operations()
        .unwrap()
        .value()
        .get("a")
        .unwrap()
        .to_owned();
    // keep_part follows this input reference without requiring a uses label.
    assert_eq!(
        operation
            .get("input")
            .unwrap()
            .get("$ref")
            .unwrap()
            .as_str(),
        Some("#/schemas/S")
    );
    let selected = keep_part(
        &result,
        &request,
        &["a"],
        &PullLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        selected
            .value()
            .get("operations")
            .unwrap()
            .get("b")
            .is_none()
    );
    assert!(selected.value().get("schemas").unwrap().get("T").is_none());
    // Missing the schema reference target is caught by revalidation of this chosen
    // edit; validation of the all-additions snapshot did not authorize any subset.
    let incomplete = original
        .document()
        .edit(&[Edit::set(&["operations", "a"], operation)])
        .unwrap()
        .validate()
        .unwrap();
    assert!(incomplete.conformant().is_none());
}

#[test]
fn comparison_gaps_and_removal_proposals_are_reported() {
    let client = Embedded;
    let comparison = document(
        r##"{"openbindings":"0.2.0","operations":{"a":{"input":true}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{"input":{"$ref":"#/missing"}},"data":{"item":"a"}}]}}}}"##,
    );
    let result = complete(client.pull_source(
        PullRequest::new(comparison, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(
        result.report().omitted().unwrap().next().unwrap().status(),
        OmittedStatus::ImplementationUnsupported
    );
    assert!(!result.report().is_complete());
    let removal = document(
        r#"{"openbindings":"0.2.0","operations":{"a":{"input":true}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{},"data":{"item":"a"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","data":{"item":"a"}}}}"#,
    );
    let result = complete(client.pull_source(
        PullRequest::new(removal.clone(), "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    let entry = result.report().existing().unwrap().next().unwrap();
    assert_eq!(entry.status(), ExistingStatus::Stale);
    let Some(Proposal::Binding(proposal)) = entry.proposal() else {
        panic!("binding proposal")
    };
    assert_eq!(
        proposal
            .remove()
            .unwrap()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["input"]
    );
    let edited = removal
        .document()
        .edit(&[Edit::remove(&["operations", "a", "input"])])
        .unwrap();
    let edited = edited.validate().unwrap().conformant().unwrap();
    let next = complete(client.pull_source(
        PullRequest::new(edited, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(
        next.report().existing().unwrap().next().unwrap().status(),
        ExistingStatus::Current
    );
}

#[test]
fn unchanged_reference_with_unaccounted_target_drift_is_unassessed() {
    let original = document(
        r##"{"openbindings":"0.2.0","operations":{"a":{"input":{"$ref":"#/schemas/S"}}},"schemas":{"S":{"type":"string","minLength":1}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"schemas":{"S":{"type":"string","minLength":2}},"items":[{"key":"a","operation":{"input":{"$ref":"#/schemas/S"}},"data":{"item":"a"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","data":{"item":"a"}}}}"##,
    );
    let result = complete(Embedded.pull_source(
        PullRequest::new(original, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    let entry = result.report().existing().unwrap().next().unwrap();
    assert_eq!(entry.status(), ExistingStatus::Unassessed);
    assert_eq!(
        entry.reason_code().unwrap().as_str(),
        Some("unaccounted-reference-change")
    );
    assert!(entry.proposal().is_none());
    assert!(!result.report().is_complete());
}
#[test]
fn annotation_only_schema_edits_leave_existing_bindings_current() {
    let original = document(
        r#"{"openbindings":"0.2.0","operations":{"a":{"input":{"type":"string","description":"author","default":"old","format":"email"}}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"items":[{"key":"a","operation":{"input":{"type":"string","description":"source","default":"new"}},"data":{"item":"a"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","data":{"item":"a"}}}}"#,
    );
    let result = complete(Embedded.pull_source(
        PullRequest::new(original, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    let entry = result.report().existing().unwrap().next().unwrap();
    assert_eq!(entry.status(), ExistingStatus::Current);
    assert!(entry.proposal().is_none());
    assert!(result.report().is_complete());
}

#[test]
fn accounted_operation_target_drift_is_reported_at_the_target_binding() {
    let original = document(
        r##"{"openbindings":"0.2.0","operations":{"a":{"input":{"$ref":"#/operations/b/input"}},"b":{"input":{"type":"string","minLength":1}}},"sources":{"catalog":{"kind":"example.embedded@1","data":{"operations":{"b":{"input":{"type":"string","minLength":2}}},"items":[{"key":"a","operation":{"input":{"$ref":"#/operations/b/input"}},"data":{"item":"a"}},{"key":"b","operation":{"input":{"type":"string","minLength":2}},"data":{"item":"b"}}]}}},"bindings":{"a":{"operation":"a","source":"catalog","data":{"item":"a"}},"b":{"operation":"b","source":"catalog","data":{"item":"b"}}}}"##,
    );
    let result = complete(Embedded.pull_source(
        PullRequest::new(original, "catalog"),
        CancellationToken::new(),
    ))
    .unwrap();
    let entries = result.report().existing().unwrap().collect::<Vec<_>>();
    assert_eq!(entries[0].status(), ExistingStatus::Current);
    assert_eq!(entries[1].status(), ExistingStatus::Stale);
    assert!(result.report().is_complete());
}

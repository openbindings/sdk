use openbindings::{JsonValue, Pointer};
use openbindings_source_pull::{CarrierError, CarrierErrorKind, PullReport, PullResult};
use std::error::Error;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

#[test]
fn every_report_collection_locates_the_failing_row_directly_and_inside_a_result() {
    let secret = "private producer detail";
    for (collection, valid, invalid, kind) in [
        (
            "added",
            r#"{"binding":"b","operation":"o"}"#,
            r#"{"binding":"private producer detail"}"#,
            CarrierErrorKind::MissingField("operation"),
        ),
        (
            "existing",
            r#"{"binding":"b","status":"current"}"#,
            r#"{"binding":"private producer detail","status":"unassessed","reasonCode":"r"}"#,
            CarrierErrorKind::MissingField("existing/message"),
        ),
        (
            "omitted",
            r#"{"status":"invalid","reasonCode":"r","message":"m"}"#,
            r#"{"status":"invalid","reasonCode":"private producer detail"}"#,
            CarrierErrorKind::MissingField("omitted/message"),
        ),
    ] {
        for index in [0, 1] {
            let rows = if index == 0 {
                invalid.to_owned()
            } else {
                format!("{valid},{invalid}")
            };
            let added = if collection == "added" {
                String::new()
            } else {
                "\"added\":[],".to_owned()
            };
            let report = format!(r#"{{"exhaustive":true,{added}"{collection}":[{rows}]}}"#);
            let direct = PullReport::from_json(json(&report)).unwrap_err();
            let nested =
                PullResult::from_json(json(&format!(r#"{{"additions":{{}},"report":{report}}}"#)))
                    .unwrap_err();
            for (error, prefix) in [(direct, ""), (nested, "/report")] {
                assert_eq!(error.kind(), kind);
                let expected: Pointer = format!("{prefix}/{collection}/{index}").parse().unwrap();
                assert_eq!(error.pointer(), Some(&expected));
                assert_eq!(error.clone(), error);
                assert!(error.source().is_none());
                assert!(!error.to_string().contains(secret));
                assert!(!format!("{error:?}").contains(secret));
            }
        }
    }
}

#[test]
fn nested_loss_and_proposal_failures_still_identify_the_enclosing_row() {
    for (report, path, kind) in [
        (
            r#"{"exhaustive":true,"added":[{"binding":"b","operation":"o","lossy":[{"reasonCode":"r"}]}]}"#,
            "/added/0",
            CarrierErrorKind::MissingField("lossy/message"),
        ),
        (
            r#"{"exhaustive":true,"added":[],"existing":[{"binding":"b","status":"stale","change":"dangerous","proposal":{"description":"private"}}]}"#,
            "/existing/0",
            CarrierErrorKind::ProposalAuthority,
        ),
        (
            r#"{"exhaustive":true,"added":[null]}"#,
            "/added/0",
            CarrierErrorKind::ExpectedObject,
        ),
        (
            r#"{"exhaustive":true,"added":[],"omitted":[{"status":"invalid","status":"private"}]}"#,
            "/omitted/0",
            CarrierErrorKind::DuplicateMember,
        ),
    ] {
        let error = PullReport::from_json(json(report)).unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.pointer().and_then(Pointer::as_str), Some(path));
    }
}

#[test]
fn unrelated_top_level_failures_and_category_construction_are_unlocated() {
    fn is_copy<T: Copy>(_: T) {}
    for text in [
        r#"{}"#,
        r#"{"exhaustive":true,"added":null}"#,
        r#"{"exhaustive":false,"added":[]}"#,
    ] {
        let error = PullReport::from_json(json(text)).unwrap_err();
        is_copy(error.kind());
        assert!(error.pointer().is_none());
        let nested =
            PullResult::from_json(json(&format!(r#"{{"additions":{{}},"report":{text}}}"#)))
                .unwrap_err();
        assert_eq!(nested, error);
    }
    let error = CarrierError::from(CarrierErrorKind::DuplicateUse);
    assert_eq!(error.kind(), CarrierErrorKind::DuplicateUse);
    assert!(error.pointer().is_none());
}

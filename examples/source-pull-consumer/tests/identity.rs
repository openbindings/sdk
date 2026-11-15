//! Reordering offerings must not change which existing binding they identify.
use openbindings::{Document, Edit, JsonValue};
use openbindings_source_pull::{CancellationToken, ExistingStatus, PullRequest, SourcePullerExt};
use source_pull_consumer::Embedded;

#[tokio::test]
async fn insertion_before_existing_offerings_adds_only_the_new_identity() {
    let original = Document::parse(
        r#"{
        "openbindings":"0.2.0","operations":{},
        "sources":{"s":{"kind":"example.embedded@1","data":{"items":[
            {"key":"b","operation":{},"data":{"item":"b"}},
            {"key":"c","operation":{},"data":{"item":"c"}}
        ]}}}
    }"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let (_, first) = Embedded
        .pull_source(PullRequest::new(original, "s"), CancellationToken::new())
        .await
        .unwrap();
    let changed = first
        .document()
        .edit(&[Edit::set(
            &["sources", "s", "data", "items"],
            JsonValue::parse(
                r#"[
            {"key":"a","operation":{},"data":{"item":"a"}},
            {"key":"b","operation":{},"data":{"item":"b"}},
            {"key":"c","operation":{},"data":{"item":"c"}}
        ]"#,
            )
            .unwrap(),
        )])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    let (report, second) = Embedded
        .pull_source(PullRequest::new(changed, "s"), CancellationToken::new())
        .await
        .unwrap();
    let additions = report.report().added().collect::<Vec<_>>();
    assert_eq!(additions.len(), 1);
    assert_eq!(additions[0].operation().as_str(), Some("a"));
    let existing = report.report().existing().unwrap().collect::<Vec<_>>();
    assert_eq!(existing.len(), 2);
    assert!(
        existing
            .iter()
            .all(|entry| entry.status() == ExistingStatus::Current)
    );
    assert_eq!(second.bindings().unwrap().len(), 3);
}

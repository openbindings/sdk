//! The source-checker stage linked from the adapter guide.
use openbindings::JsonValue;
use openbindings_kind_description::{
    CancellationToken, CheckFailure, CheckSourceRequest, DescribeKindRequest, KindDescriptorExt,
    KindError, SourceCheckerExt,
};
use source_pull_consumer::KindCatalog;

#[tokio::test]
async fn accepted_entry_excludes_context_and_rejection_has_fixed_text() {
    let checker = KindCatalog::default();
    let description = checker
        .describe_kind(
            DescribeKindRequest::new("example.fixed@1").unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!description.check().unwrap().executes());
    let accepted = checker
        .check_source(
            CheckSourceRequest::from_json(
                JsonValue::parse(r#"{"kind":"example.fixed@1","context":{"approval":true}}"#)
                    .unwrap(),
            )
            .unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let expected = JsonValue::parse(
        r#"{"kind":"example.fixed@1","description":"Checked local fixture source."}"#,
    )
    .unwrap();
    assert_eq!(
        accepted.value().value().to_owned().semantic_eq(&expected),
        Some(true)
    );
    assert!(accepted.value().value().get("context").is_none());

    let rejected = checker
        .check_source(
            CheckSourceRequest::from_json(
                JsonValue::parse(r#"{"kind":"example.embedded@1","data":{"items":false}}"#)
                    .unwrap(),
            )
            .unwrap(),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    let KindError::Provider(CheckFailure::Rejected(rejection)) = rejected else {
        panic!("expected a source rejection");
    };
    assert_eq!(rejection.code().as_str(), Some("example.invalid-source"));
    assert_eq!(
        rejection.message().as_str(),
        Some("The proposed source does not have this local kind's catalog carriage.")
    );
}

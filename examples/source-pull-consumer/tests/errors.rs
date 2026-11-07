use openbindings_schema_comparison::ComparisonFailure;
use source_pull_consumer::CatalogError;
use std::error::Error;

#[test]
fn catalog_errors_display_and_preserve_the_comparison_cause() {
    let source = CatalogError::Source;
    assert!(!source.to_string().is_empty());
    assert!(source.source().is_none());
    let comparison = CatalogError::Comparison(ComparisonFailure::LimitExceeded("work"));
    assert!(comparison.to_string().contains("comparison"));
    assert_eq!(
        comparison
            .source()
            .unwrap()
            .downcast_ref::<ComparisonFailure>(),
        Some(&ComparisonFailure::LimitExceeded("work"))
    );
}

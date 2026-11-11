use openbindings::JsonValue;
use openbindings_schema_comparison::*;
use std::sync::atomic::AtomicBool;
fn input(schema: &str, target: &str) -> SchemaInput {
    SchemaInput::new(
        JsonValue::parse(schema).unwrap(),
        JsonValue::parse(format!(r#"{{"schemas":{{"S":{target}}}}}"#)).unwrap(),
    )
}
#[test]
fn accounted_leaves_keep_siblings_and_containing_conjunctions_unmerged() {
    let a = input(
        r##"{"allOf":[{"$ref":"#/schemas/S","minLength":1},{"type":"string"}]}"##,
        r#"{"type":"integer"}"#,
    );
    let b = input(
        r##"{"allOf":[{"$ref":"#/schemas/S","minLength":1,"description":"author"},{"type":"string"}]}"##,
        r#"{"type":"string"}"#,
    );
    assert_eq!(
        identical(Some(&a), Some(&b), &ComparisonLimits::default()),
        Err(ComparisonFailure::UnsatisfiableMerge)
    );
    assert_eq!(
        identical_with_references(
            Some(&a),
            Some(&b),
            &|r| r.spelling() == "#/schemas/S",
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    let reordered = input(
        r##"{"allOf":[{"type":"string"},{"$ref":"#/schemas/S","minLength":1}]}"##,
        r#"{"type":"string"}"#,
    );
    assert_eq!(
        identical_with_references(
            Some(&a),
            Some(&reordered),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Ok(false)
    );
    let changed = input(
        r##"{"allOf":[{"$ref":"#/schemas/S","minLength":2},{"type":"string"}]}"##,
        r#"{"type":"string"}"#,
    );
    assert_eq!(
        identical_with_references(
            Some(&b),
            Some(&changed),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Ok(false)
    );
}
#[test]
fn unaccounted_target_drift_has_a_second_identity_question() {
    let a = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"type":"string"}"#);
    let b = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"type":"number"}"#);
    assert_eq!(
        identical_with_references(Some(&a), Some(&b), &|_| false, &ComparisonLimits::default()),
        Ok(false)
    );
    assert_eq!(
        identical_with_references(Some(&a), Some(&b), &|_| true, &ComparisonLimits::default()),
        Ok(true)
    );
    let inline = input(r#"{"type":"string"}"#, "true");
    assert_eq!(
        identical_with_references(
            Some(&a),
            Some(&inline),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    assert_eq!(
        identical_with_references(None, Some(&a), &|_| true, &ComparisonLimits::default()),
        Ok(false)
    );
}
#[test]
fn only_same_spelled_accounted_references_can_hide_missing_content_or_cycles() {
    let a = input(r##"{"$ref":"#/schemas/S"}"##, r##"{"$ref":"#/schemas/S"}"##);
    assert_eq!(
        identical_with_references(Some(&a), Some(&a), &|_| true, &ComparisonLimits::default()),
        Ok(true)
    );
    assert_eq!(
        identical_with_references(Some(&a), Some(&a), &|_| false, &ComparisonLimits::default()),
        Err(ComparisonFailure::ReferenceCycle)
    );
    let opaque = input(r#"{"not":{"$ref":"not a uri"}}"#, "true");
    assert_eq!(
        identical_with_references(
            Some(&opaque),
            Some(&opaque),
            &|_| false,
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    let malformed_target = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"$ref":"not a uri"}"#);
    assert_eq!(
        identical_with_references(
            Some(&malformed_target),
            Some(&malformed_target),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    assert_eq!(
        identical_with_references(
            Some(&malformed_target),
            Some(&malformed_target),
            &|_| false,
            &ComparisonLimits::default()
        ),
        Err(ComparisonFailure::InvalidReference)
    );
    let missing = input(r##"{"$ref":"#/missing"}"##, "true");
    assert_eq!(
        identical_with_references(
            Some(&missing),
            Some(&missing),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Ok(true)
    );
    assert_eq!(
        identical_with_references(
            Some(&missing),
            Some(&missing),
            &|_| false,
            &ComparisonLimits::default()
        ),
        Err(ComparisonFailure::MissingReference)
    );
    assert_eq!(
        identical_with_references(
            Some(&a),
            Some(&missing),
            &|_| true,
            &ComparisonLimits::default()
        ),
        Err(ComparisonFailure::ReferenceCycle)
    );
}
#[test]
fn reference_rule_variants_apply_limits_and_cooperative_control() {
    let a = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"type":"string"}"#);
    assert_eq!(
        identical_with_references_and_control(
            Some(&a),
            Some(&a),
            &|_| true,
            &ComparisonLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(ComparisonFailure::Cancelled)
    );
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
            identical_with_references(Some(&a), Some(&a), &|_| true, &limits),
            Err(ComparisonFailure::LimitExceeded(_))
        ));
    }
}

#[test]
fn accounting_receives_the_original_resource_context_without_disclosing_it() {
    let a = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"type":"string"}"#)
        .with_base("https://sentinel.test/root");
    let b = input(r##"{"$ref":"#/schemas/S"}"##, r#"{"type":"number"}"#)
        .with_base("https://sentinel.test/root");
    let accounted = |context: ReferenceContext<'_>| {
        assert_eq!(context.spelling(), "#/schemas/S");
        assert!(context.resource().get("schemas").is_some());
        assert_eq!(
            context.schema().get("$ref").unwrap().as_str(),
            Some(context.spelling())
        );
        assert!(!format!("{context:?}").contains("sentinel"));
        context.base() == Some("https://sentinel.test/root")
    };
    assert_eq!(
        identical_with_references(Some(&a), Some(&b), &accounted, &ComparisonLimits::default()),
        Ok(true)
    );
    let other = b.with_base("https://other.test/root");
    assert_eq!(
        identical_with_references(
            Some(&a),
            Some(&other),
            &accounted,
            &ComparisonLimits::default()
        ),
        Ok(false)
    );
}

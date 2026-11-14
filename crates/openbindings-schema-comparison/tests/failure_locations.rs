use openbindings::{Document, JsonValue};
use openbindings_schema_comparison::*;
use std::sync::atomic::AtomicBool;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn input(text: &str) -> SchemaInput {
    let root = json(text);
    ComparisonContext::new(root.clone()).schema(root)
}
fn failure(input: &SchemaInput) -> ComparisonFailure {
    identical(Some(input), None, &ComparisonLimits::default()).unwrap_err()
}
fn location(error: &ComparisonFailure, side: ComparisonSide, pointer: &str) {
    assert_eq!(error.side(), Some(side));
    assert_eq!(error.origin().unwrap().pointer.as_str(), Some(pointer));
}

#[test]
fn nested_failures_keep_original_locations_and_distinguish_identity_operands() {
    let first = input(r##"{"properties":{"first":{"$ref":"#/missing"}}}"##);
    let second = input(r##"{"properties":{"second":{"$ref":"#/missing"}}}"##);
    let a = failure(&first);
    let b = failure(&second);
    assert_eq!(a.kind(), ComparisonFailureKind::MissingReference);
    assert_eq!(a.kind(), b.kind());
    assert_ne!(a, b);
    location(&a, ComparisonSide::Left, "/properties/first/$ref");
    location(&b, ComparisonSide::Left, "/properties/second/$ref");
    let right = identical(None, Some(&first), &ComparisonLimits::default()).unwrap_err();
    location(&right, ComparisonSide::Right, "/properties/first/$ref");
    assert_ne!(a, right);
    assert_eq!(a.clone(), a);
}

#[test]
fn directional_failures_name_the_target_or_candidate() {
    let bad = input(r#"{"properties":{"nested":{"type":42}}}"#);
    let good = input("true");
    for direction in [Direction::Input, Direction::Output] {
        for (target, candidate, side) in [
            (&bad, &good, ComparisonSide::Target),
            (&good, &bad, ComparisonSide::Candidate),
        ] {
            let error =
                compare(direction, target, candidate, &ComparisonLimits::default()).unwrap_err();
            assert_eq!(error.kind(), ComparisonFailureKind::InvalidSchema);
            location(&error, side, "/properties/nested");
        }
    }
}

#[test]
fn supplied_subtrees_keep_the_retrieval_frame_through_nested_ids_and_references() {
    let carrier = json(
        r##"{"secret-carrier":{"$id":"https://nested.test/base","$defs":{"T":{"$id":"child","properties":{"a/b~c":{"$ref":"missing"}}}},"$ref":"#/$defs/T"}}"##,
    );
    let external = carrier.get("secret-carrier").unwrap().to_owned();
    let root = json(r#"{"$ref":"https://fetch.test/resource"}"#);
    let context =
        ComparisonContext::new(root.clone()).with_resource("https://fetch.test/resource", external);
    let error = failure(&context.schema(root));
    assert_eq!(error.kind(), ComparisonFailureKind::MissingReference);
    location(
        &error,
        ComparisonSide::Left,
        "/$defs/T/properties/a~1b~0c/$ref",
    );
    assert_eq!(
        error.origin().unwrap().source_uri.as_deref(),
        Some("https://fetch.test/resource")
    );
    for formatted in [error.to_string(), format!("{error:?}")] {
        for secret in [
            "secret-carrier",
            "fetch.test",
            "nested.test",
            "a/b~c",
            "missing",
        ] {
            assert!(!formatted.contains(secret));
        }
    }
}

#[test]
fn schema_selection_and_retained_applicators_keep_original_document_pointers() {
    let root = json(r##"{"carrier":{"not":{"properties":{"broken":{"$ref":"#/missing"}}}}}"##);
    let context = ComparisonContext::new(root.clone());
    let error = failure(&context.schema(root.get("carrier").unwrap().to_owned()));
    location(
        &error,
        ComparisonSide::Left,
        "/carrier/not/properties/broken/$ref",
    );
    assert!(error.origin().unwrap().source_uri.is_none());
}

#[test]
fn exact_pointer_units_survive_without_lossy_conversion() {
    let error = failure(&input(
        r##"{"properties":{"\ud800":{"$ref":"#/missing"}}}"##,
    ));
    let pointer = &error.origin().unwrap().pointer;
    assert!(pointer.as_str().is_none());
    let mut expected = "/properties/".encode_utf16().collect::<Vec<_>>();
    expected.push(0xd800);
    expected.extend("/$ref".encode_utf16());
    assert_eq!(pointer.utf16_units().collect::<Vec<_>>(), expected);
}

#[test]
fn reference_inventory_and_callback_failures_keep_location_and_side() {
    let bad = input(r#"{"properties":{"nested":{"$ref":42}}}"#);
    let good = input("true");
    let error = identical_with_references(
        Some(&good),
        Some(&bad),
        &|_| false,
        &ComparisonLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), ComparisonFailureKind::InvalidReference);
    location(&error, ComparisonSide::Right, "/properties/nested");

    let missing = input(r##"{"properties":{"nested":{"$ref":"#/missing"}}}"##);
    let sides = std::cell::RefCell::new(Vec::new());
    assert!(
        identical_with_references(
            Some(&missing),
            Some(&missing),
            &|reference| {
                let error = reference.target().unwrap_err();
                assert_eq!(
                    error.origin().unwrap().pointer.as_str(),
                    Some("/properties/nested/$ref")
                );
                sides.borrow_mut().push(error.side().unwrap());
                true
            },
            &ComparisonLimits::default()
        )
        .unwrap()
    );
    assert!(sides.borrow().contains(&ComparisonSide::Left));
    assert!(sides.borrow().contains(&ComparisonSide::Right));
}

#[test]
fn invalid_referenced_schema_declarations_keep_the_target_location() {
    let root = json(r##"{"$ref":"https://fetch.test/schema#/$defs/bad"}"##);
    let context = ComparisonContext::new(root.clone()).with_resource(
        "https://fetch.test/schema",
        json(r#"{"$defs":{"bad":{"$id":42}}}"#),
    );
    let error = failure(&context.schema(root));
    assert_eq!(error.kind(), ComparisonFailureKind::InvalidReference);
    location(&error, ComparisonSide::Left, "/$defs/bad");
    assert_eq!(
        error.origin().unwrap().source_uri.as_deref(),
        Some("https://fetch.test/schema")
    );
}

#[test]
fn merge_failures_keep_the_original_conjunction() {
    let error = failure(&input(
        r#"{"properties":{"nested":{"allOf":[{"type":"string"},{"type":"number"}]}}}"#,
    ));
    assert_eq!(error.kind(), ComparisonFailureKind::UnsatisfiableMerge);
    location(&error, ComparisonSide::Left, "/properties/nested");
}

#[test]
fn global_refusals_have_no_fabricated_side_or_location() {
    let schema = input("true");
    for error in [
        identical_with_control(
            Some(&schema),
            None,
            &ComparisonLimits::default(),
            &AtomicBool::new(true),
        )
        .unwrap_err(),
        identical(
            Some(&schema),
            None,
            &ComparisonLimits {
                max_work: Some(0),
                ..Default::default()
            },
        )
        .unwrap_err(),
        identical(
            Some(&schema),
            None,
            &ComparisonLimits {
                max_allocation_bytes: 0,
                ..Default::default()
            },
        )
        .unwrap_err(),
    ] {
        assert!(error.origin().is_none());
        assert!(error.side().is_none());
    }
    let context_error = failure(&schema.clone().with_base("relative"));
    assert_eq!(
        context_error.kind(),
        ComparisonFailureKind::InvalidReference
    );
    assert_eq!(context_error.side(), Some(ComparisonSide::Left));
    assert!(context_error.origin().is_none());
}

#[test]
fn interface_failures_keep_required_and_provided_sides_for_both_roles() {
    let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://fetch.test/schema"}}}}"#)
        .unwrap().validate().unwrap().conformant().unwrap();
    let root = document.document().value();
    let good = ComparisonContext::new(root.clone())
        .with_resource("https://fetch.test/schema", json("true"));
    let bad = ComparisonContext::new(root.clone()).with_resource(
        "https://fetch.test/schema",
        json(r#"{"properties":{"nested":{"type":42}}}"#),
    );
    for (required, provided, side) in [
        (&bad, &good, ComparisonSide::Required),
        (&good, &bad, ComparisonSide::Provided),
    ] {
        let report = check_interface(
            &document,
            &document,
            required,
            provided,
            &|_, _| Some(OperationRole::Both),
            &ComparisonLimits::default(),
        )
        .unwrap();
        assert_eq!(report.issues().count(), 2);
        for issue in report.issues() {
            let error = issue.failure().unwrap();
            assert_eq!(error.kind(), ComparisonFailureKind::InvalidSchema);
            location(&error, side, "/properties/nested");
        }
    }
}

#[test]
fn omitted_oversized_origins_are_not_replaced_by_the_referring_schema() {
    let name = "s".repeat(8 * 1024 * 1024);
    let external = json(&format!(
        r#"{{"$defs":{{"{name}":{{"$anchor":"bad","type":42}}}}}}"#
    ));
    let root = json(r#"{"$ref":"https://fetch.test/schema#bad"}"#);
    let selected = ComparisonContext::new(root.clone())
        .with_resource("https://fetch.test/schema", external)
        .schema(root);
    let error = failure(&selected);
    assert_eq!(error.kind(), ComparisonFailureKind::InvalidSchema);
    assert_eq!(error.side(), Some(ComparisonSide::Left));
    assert!(
        error.origin().is_none(),
        "an omitted original coordinate must not become the referring root"
    );
}

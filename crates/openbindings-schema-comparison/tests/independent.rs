use openbindings::JsonValue;
use openbindings_schema_comparison::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn input(side: &serde_json::Value) -> Option<SchemaInput> {
    if side.is_null() {
        return None;
    }
    let schema = json(side["schema_json"].as_str().unwrap());
    let root = side["document_root_json"]
        .as_str()
        .map(json)
        .unwrap_or_else(|| schema.clone());
    let mut input = SchemaInput::new(schema, root);
    if let Some(base) = side["base_uri"].as_str() {
        input = input.with_base(base);
    }
    if let Some(resources) = side["resources"].as_array() {
        for resource in resources {
            input = input.with_resource(
                resource["uri"].as_str().unwrap(),
                json(resource["json"].as_str().unwrap()),
            );
        }
    }
    Some(input)
}
#[test]
fn independently_authored_design_controls_keep_their_expected_classifications() {
    let controls: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/independent.json")).unwrap();
    let cases = controls["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    for case in cases {
        let left = input(&case["left"]);
        let right = input(&case["right"]);
        let actual = identical(left.as_ref(), right.as_ref(), &ComparisonLimits::default());
        let category = match actual {
            Ok(Identity::Equal) => "Equal",
            Ok(Identity::Different) => "Different",
            Err(ComparisonFailure::InvalidSchema) => "Failure(InputShape)",
            Err(
                ComparisonFailure::InvalidReference
                | ComparisonFailure::MissingReference
                | ComparisonFailure::ReferenceCycle
                | ComparisonFailure::UnsupportedReference,
            ) => "Failure(Reference)",
            Err(ComparisonFailure::UnsatisfiableMerge) => "Failure(SchemaMerge)",
            other => panic!("unexpected {}: {other:?}", case["id"]),
        };
        assert_eq!(
            category,
            case["expected"].as_str().unwrap(),
            "{}: {}",
            case["id"],
            case["reason"]
        );
    }
}
struct CancelAfter {
    checks: AtomicUsize,
    limit: usize,
}
impl openbindings::WorkControl for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.limit
    }
}
#[test]
fn every_observed_public_checkpoint_cancels_and_a_fresh_attempt_is_unchanged() {
    let left = SchemaInput::standalone(json(
        r##"{"allOf":[{"properties":{"p":{"$ref":"#/$defs/P"}}},{"properties":{"p":{"minLength":2}}}],"$defs":{"P":{"type":"string"}}}"##,
    ));
    let right = SchemaInput::standalone(json(
        r#"{"properties":{"p":{"type":"string","minLength":2}}}"#,
    ));
    let baseline = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: usize::MAX,
    };
    assert_eq!(
        identical_with_control(
            Some(&left),
            Some(&right),
            &ComparisonLimits::default(),
            &baseline
        ),
        Ok(Identity::Equal)
    );
    let checkpoints = baseline.checks.load(Ordering::Relaxed);
    for limit in 0..checkpoints {
        let control = CancelAfter {
            checks: AtomicUsize::new(0),
            limit,
        };
        assert_eq!(
            identical_with_control(
                Some(&left),
                Some(&right),
                &ComparisonLimits::default(),
                &control
            ),
            Err(ComparisonFailure::Cancelled),
            "checkpoint {limit}/{checkpoints}"
        );
    }
    assert_eq!(
        identical(Some(&left), Some(&right), &ComparisonLimits::default()),
        Ok(Identity::Equal)
    );
}
#[test]
fn exact_and_one_below_resource_allowances_bound_both_sides_together() {
    let left = SchemaInput::standalone(json(
        r#"{"anyOf":[{"const":9007199254740993},{"const":1e100000}]}"#,
    ));
    let right = SchemaInput::standalone(json(
        r#"{"anyOf":[{"const":10e99999},{"const":9007199254740993.0}]}"#,
    ));
    for allocation in [true, false] {
        let mut low = 0;
        let mut high = if allocation {
            ComparisonLimits::default().max_allocation_bytes
        } else {
            ComparisonLimits::default().max_work
        };
        while low < high {
            let middle = low + (high - low) / 2;
            let mut limits = ComparisonLimits::default();
            if allocation {
                limits.max_allocation_bytes = middle;
            } else {
                limits.max_work = middle;
            }
            match identical(Some(&left), Some(&right), &limits) {
                Ok(Identity::Equal) => high = middle,
                Err(ComparisonFailure::LimitExceeded(_)) => low = middle + 1,
                other => panic!("unexpected resource result {other:?}"),
            }
        }
        let mut exact = ComparisonLimits::default();
        if allocation {
            exact.max_allocation_bytes = low;
        } else {
            exact.max_work = low;
        }
        assert_eq!(
            identical(Some(&left), Some(&right), &exact),
            Ok(Identity::Equal)
        );
        if allocation {
            exact.max_allocation_bytes -= 1;
        } else {
            exact.max_work -= 1;
        }
        assert!(matches!(
            identical(Some(&left), Some(&right), &exact),
            Err(ComparisonFailure::LimitExceeded(_))
        ));
    }
}
#[test]
fn source_projection_and_owned_core_schema_consumers_keep_their_own_roots() {
    // This projection only sees source data, not the destination's names/shapes.
    fn project(source: &JsonValue) -> SchemaInput {
        SchemaInput::new(source.get("offered").unwrap().to_owned(), source.clone())
    }
    let offered = project(&json(
        r##"{"offered":{"$ref":"#/defs/Input"},"defs":{"Input":{"type":"integer"}}}"##,
    ));
    let existing = {
        let document=openbindings::validate_document(br##"{"openbindings":"0.2.0","operations":{"read":{"input":{"$ref":"#/schemas/Input"}}},"schemas":{"Input":{"type":"integer"}}}"##).unwrap();
        let proof = document.conformant().unwrap();
        let schema = proof.resolve_operation("read").unwrap().input().unwrap();
        SchemaInput::from(&schema)
    };
    assert_eq!(
        identical(
            Some(&offered),
            Some(&existing),
            &ComparisonLimits::default()
        ),
        Ok(Identity::Equal)
    );
    assert!(
        offered.document().get("openbindings").is_none(),
        "offered input never needs a fabricated OBI"
    );
}

#[test]
fn merge_uses_the_published_keyword_table_without_an_invented_satisfiability_solver() {
    for (merged, explicit) in [
        (
            r#"{"allOf":[{"const":1},{"enum":[2]}]}"#,
            r#"{"const":1,"enum":[2]}"#,
        ),
        (
            r#"{"allOf":[{"type":"number","minimum":2},{"maximum":1}]}"#,
            r#"{"type":"number","minimum":2,"maximum":1}"#,
        ),
        (
            r#"{"allOf":[{"minimum":2},{"maximum":1}]}"#,
            r#"{"minimum":2,"maximum":1}"#,
        ),
    ] {
        let left = SchemaInput::standalone(json(merged));
        let right = SchemaInput::standalone(json(explicit));
        assert_eq!(
            identical(Some(&left), Some(&right), &ComparisonLimits::default()),
            Ok(Identity::Equal)
        );
    }
}
#[test]
fn unresolved_enum_intersection_multiplicity_is_a_failure_while_standalone_values_stay_ordered() {
    let repeated = SchemaInput::standalone(json(r#"{"enum":[1,1.0]}"#));
    let single = SchemaInput::standalone(json(r#"{"enum":[1]}"#));
    assert_eq!(
        identical(
            Some(&repeated),
            Some(&repeated),
            &ComparisonLimits::default()
        ),
        Ok(Identity::Equal)
    );
    assert_eq!(
        identical(Some(&repeated), Some(&single), &ComparisonLimits::default()),
        Ok(Identity::Different)
    );
    let intersection =
        SchemaInput::standalone(json(r#"{"allOf":[{"enum":[1,1.0]},{"enum":[1]}]}"#));
    assert_eq!(
        identical(
            Some(&intersection),
            Some(&single),
            &ComparisonLimits::default()
        ),
        Err(ComparisonFailure::UnsupportedNormalization)
    );
}

#[test]
fn resources_keep_reference_scope_and_outside_dependencies_without_reading_instance_data() {
    for (left, right, expected) in [
        (
            r##"{"$defs":{"P":true},"x-\ud800":true}"##,
            r##"{}"##,
            Identity::Equal,
        ),
        (
            r##"{"$id":"https://example.test/S","$defs":{"P":{"type":"string"}},"properties":{"p":{"$ref":"#/$defs/P"}}}"##,
            r##"{"$id":"https://example.test/S","$defs":{"P":{"type":"string"}},"properties":{"p":{"type":"string"}}}"##,
            Identity::Equal,
        ),
        (
            r##"{"$defs":{"P":{"type":"string"}},"properties":{"p":{"$dynamicRef":"#/$defs/P"}}}"##,
            r##"{"$defs":{"P":{"type":"number"}},"properties":{"p":{"$dynamicRef":"#/$defs/P"}}}"##,
            Identity::Different,
        ),
        (
            r##"{"$defs":{"P":true},"const":{"$id":"relative","$ref":"%"}}"##,
            r##"{"const":{"$id":"relative","$ref":"%"}}"##,
            Identity::Equal,
        ),
        (
            r##"{"$defs":{"P":{"$anchor":"named","type":"string"}},"properties":{"p":{"$ref":"#named"}}}"##,
            r##"{"$defs":{"P":{"$anchor":"named","type":"string"}},"properties":{"p":{"$ref":"#named"}}}"##,
            Identity::Equal,
        ),
    ] {
        let left = SchemaInput::standalone(json(left));
        let right = SchemaInput::standalone(json(right));
        assert_eq!(
            identical(Some(&left), Some(&right), &ComparisonLimits::default()),
            Ok(expected)
        );
    }
    // Each nested resource supplies its own fragment root, independently of the outer one.
    let schema = json(
        r##"{"$id":"https://example.test/outer","$defs":{"P":false,"Inner":{"$id":"inner","$defs":{"P":true},"$ref":"#/$defs/P"}},"$ref":"inner"}"##,
    );
    let same = SchemaInput::standalone(schema);
    assert_eq!(
        identical(Some(&same), Some(&same), &ComparisonLimits::default()),
        Ok(Identity::Equal)
    );
}

#[test]
fn nested_retained_conjunctions_keep_authored_order() {
    let first = r#"{"allOf":[{"properties":{"a":true},"additionalProperties":false},{"properties":{"b":true}}]}"#;
    let second = r#"{"allOf":[{"oneOf":[true,false]},{"type":"string"}]}"#;
    let left = SchemaInput::standalone(json(&format!(r#"{{"allOf":[{first},{second}]}}"#)));
    let right = SchemaInput::standalone(json(&format!(r#"{{"allOf":[{second},{first}]}}"#)));
    assert_eq!(
        identical(Some(&left), Some(&left), &ComparisonLimits::default()),
        Ok(Identity::Equal)
    );
    assert_eq!(
        identical(Some(&left), Some(&right), &ComparisonLimits::default()),
        Ok(Identity::Different)
    );
}

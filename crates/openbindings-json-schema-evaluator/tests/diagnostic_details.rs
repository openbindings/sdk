use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::Arc;

fn prepare(text: &str, resources: ResourceSet, bytes: usize, details: bool) -> PreparedContract {
    let doc = ParsedDocument::parse(text).unwrap();
    let context = doc
        .value_contracts(
            Arc::new(
                DefaultEvaluator::with_limits(Limits {
                    diagnostic_bytes: bytes,
                    ..Limits::default()
                })
                .with_schema_details(details),
            ),
            resources,
        )
        .unwrap();
    let ContractPreparation::Ready(contract) = context.prepare("op", Side::Input) else {
        panic!("not ready")
    };
    contract
}
fn run(schema: &str, input: &str, bytes: usize, details: bool) -> ValueOutcome {
    let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    prepare(&text, ResourceSet::default(), bytes, details)
        .validate(&JsonValue::parse(input).unwrap())
}
fn failure(result: &ValueOutcome) -> (&[ValueProblem], bool) {
    let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = result
    else {
        panic!("{result:?}")
    };
    (problems, *problems_complete)
}
fn string_bytes(problem: &ValueProblem) -> usize {
    fn strings(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::String(s) => s.len(),
            serde_json::Value::Array(a) => a.iter().map(strings).sum(),
            serde_json::Value::Object(o) => o.values().map(strings).sum(),
            _ => 0,
        }
    }
    strings(&serde_json::to_value(problem).unwrap())
}
#[test]
fn exact_facts_private_defaults_and_recovery() {
    let cases = [
        (r#"{"type":"integer"}"#, r#""SECRET-instance""#, "type"),
        (r#"{"required":["SECRET/member~key"]}"#, "{}", "required"),
        (
            r#"{"minimum":9007199254740993}"#,
            "9007199254740992",
            "numeric-bound",
        ),
        (r#"{"maxLength":3}"#, r#""long""#, "size-bound"),
        (
            r#"{"enum":["SECRET-choice",9007199254740993,{"x":1e-999}]}"#,
            "null",
            "enum",
        ),
    ];
    for (schema, input, tag) in cases {
        let ordinary = run(schema, input, 1048576, false);
        let (problems, complete) = failure(&ordinary);
        assert!(complete);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].details.is_none());
        assert!(!problems[0].message.contains("SECRET"));
        let enriched = run(schema, input, 1048576, true);
        let (problems, complete) = failure(&enriched);
        assert!(complete);
        let details = serde_json::to_value(problems[0].details.as_ref().unwrap()).unwrap();
        assert_eq!(details["kind"], tag);
        match problems[0].details.as_ref().unwrap() {
            ValueProblemDetails::Required { member } => {
                assert_eq!(member, "SECRET/member~key");
                assert_eq!(problems[0].instance_pointer, "");
            }
            ValueProblemDetails::NumericBound { bound } => assert_eq!(bound, "9007199254740993"),
            ValueProblemDetails::Enum { choices } => assert_eq!(
                choices,
                &[r#""SECRET-choice""#, "9007199254740993", r#"{"x":1e-999}"#]
            ),
            _ => (),
        }
    }
}
#[test]
fn detail_boundaries_keep_base_and_atomic_facts() {
    let schema = r#"{"enum":["SECRET-choice-that-takes-space",9007199254740993]}"#;
    let full = run(schema, "null", 1048576, true);
    let full_problem = &failure(&full).0[0];
    let size = string_bytes(full_problem);
    let default = run(schema, "null", 1048576, false);
    let base_size = string_bytes(&failure(&default).0[0]);
    for budget in [0, 1, base_size + 8, base_size + 9, size - 1, size, size + 1] {
        let result = run(schema, "null", budget, true);
        let (problems, complete) = failure(&result);
        let retained: usize = problems.iter().map(string_bytes).sum();
        assert!(retained <= budget);
        // 59 envelope + <=160 fixed framing per problem + worst-case 6x
        // escaping and <=3x framing for nonempty exact-token vector elements.
        assert!(
            serde_json::to_string(&result).unwrap().len()
                <= 59 + 160 * problems.len() + 9 * retained
        );
        if budget < base_size + 9 {
            assert!(problems.is_empty());
            assert!(!complete);
        } else if budget < size {
            assert!(matches!(
                problems[0].details,
                Some(ValueProblemDetails::Truncated)
            ));
            assert!(!complete);
        } else {
            assert!(matches!(
                problems[0].details,
                Some(ValueProblemDetails::Enum { .. })
            ));
            assert!(complete);
        }
    }
}
#[test]
fn external_old_snapshot_nested_coordinates_and_escaped_names() {
    let document = r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/schema"}}}}"#;
    let resource = |schema: &str| {
        ResourceSet::new([SchemaResource {
            uri: "https://example.test/schema".into(),
            document: JsonValue::parse(schema).unwrap(),
        }])
        .unwrap()
    };
    let old = prepare(
        document,
        resource(r#"{"allOf":[{"properties":{"a/~":{"minimum":9007199254740993}}}]}"#),
        1048576,
        true,
    );
    let new = prepare(
        document,
        resource(r#"{"properties":{"a/~":{"minimum":7}}}"#),
        1048576,
        true,
    );
    let value = JsonValue::parse(r#"{"a/~":6}"#).unwrap();
    let result = old.validate(&value);
    let problem = &failure(&result).0[0];
    assert_eq!(problem.instance_pointer, "/a~1~0");
    assert_eq!(
        problem.schema_location.as_ref().unwrap().pointer,
        "/allOf/0/properties/a~1~0/minimum"
    );
    assert_eq!(
        problem
            .schema_location
            .as_ref()
            .unwrap()
            .resource
            .as_deref(),
        Some("https://example.test/schema")
    );
    assert!(
        matches!(&problem.details,Some(ValueProblemDetails::NumericBound{bound}) if bound=="9007199254740993")
    );
    let result = new.validate(&value);
    assert!(
        matches!(&failure(&result).0[0].details,Some(ValueProblemDetails::NumericBound{bound}) if bound=="7")
    );
}
#[test]
fn required_scratch_omission_keeps_base_and_does_not_copy_name() {
    let member = "é".repeat(10000);
    let schema = serde_json::json!({"required":[member]}).to_string();
    let doc = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#);
    let contract = prepare(&doc, ResourceSet::default(), 1024, true);
    let (result, usage) = jsonschema::ob_work::diagnostic_metadata(1024, 2048, || {
        contract.validate(&JsonValue::parse("{}").unwrap())
    });
    let (problems, complete) = failure(&result);
    assert_eq!(problems.len(), 1);
    assert!(!complete);
    assert!(matches!(
        problems[0].details,
        Some(ValueProblemDetails::Truncated)
    ));
    assert!(usage.omitted_optional_copies > 0);
    assert!(usage.copied_bytes < 20000);
    assert_eq!(usage.rejected_copies, 0);
}

#[test]
fn every_bound_and_required_specialization_recovers_original_facts() {
    for (keyword, input) in [
        ("minimum", "0"),
        ("maximum", "2"),
        ("exclusiveMinimum", "1"),
        ("exclusiveMaximum", "1"),
        ("minLength", "\"\""),
        ("maxLength", "\"xx\""),
        ("minItems", "[]"),
        ("maxItems", "[0,1]"),
        ("minProperties", "{}"),
        ("maxProperties", "{\"a\":0,\"b\":0}"),
    ] {
        let result = run(&format!("{{\"{keyword}\":1e0}}"), input, 1048576, true);
        let (problems, complete) = failure(&result);
        assert!(complete, "{keyword}");
        assert_eq!(problems[0].code, keyword);
        match problems[0].details.as_ref().unwrap() {
            ValueProblemDetails::NumericBound { bound }
            | ValueProblemDetails::SizeBound { bound } => assert_eq!(bound, "1e0"),
            detail => panic!("{detail:?}"),
        }
    }
    for required in [
        vec!["a"],
        vec!["a", "b"],
        vec!["a", "b", "c"],
        vec!["a", "b", "c", "d"],
    ] {
        for missing in &required {
            let schema = serde_json::json!({"required": required}).to_string();
            let mut input = serde_json::Map::new();
            for member in &required {
                if member != missing {
                    input.insert((*member).to_owned(), serde_json::Value::Null);
                }
            }
            let result = run(
                &schema,
                &serde_json::Value::Object(input).to_string(),
                1048576,
                true,
            );
            let (problems, complete) = failure(&result);
            assert!(complete);
            assert_eq!(problems.len(), 1);
            assert!(
                matches!(&problems[0].details, Some(ValueProblemDetails::Required { member }) if member == missing)
            );
        }
    }
    let unsupported = run(r#"{"const":"private"}"#, "null", 1048576, true);
    let (problems, complete) = failure(&unsupported);
    assert!(complete);
    assert!(problems[0].details.is_none());
}

use crate::planner::BoundsRequest;
use crate::planner::EvaluationBounds;
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::Arc;

fn label(outcome: &ValueOutcome) -> &'static str {
    match outcome {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { detail } => {
            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
            "no-verdict"
        }
    }
}
fn conformant(document: &Document) -> Conformant<Document> {
    document
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
}
/// Prepare operation `probe` input with the default evaluator; preparation must succeed.
fn ready(document: &Document, resources: ResourceSet, limits: Limits) -> PreparedContract {
    match conformant(document)
        .resolve_operation("probe")
        .expect("declared operation")
        .input()
        .expect("declared input")
        .prepare_with(&DefaultEvaluator::with_limits(limits), &resources)
    {
        Ok(contract) => contract,
        Err(detail) => panic!("expected ready: {detail:?}"),
    }
}
#[test]
fn frozen_c22_original_values_and_renamed_operation_keep_their_independent_truths() {
    let original = include_str!("../fixtures/oneof/C22.json");
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/oneof/C22-cases.json")).unwrap();
    // Literal primary/dependency-key rename; contract/value/resource bytes stay exact.
    for op in ["2x.thumbnail", "thumbnail.make"] {
        let mut text = original.replace("\"2x.thumbnail\"", &format!("\"{op}\""));
        if op != "2x.thumbnail" {
            text = text.replace(
                r#""aliases":["thumb.render""#,
                r#""aliases":["2x.thumbnail","thumb.render""#,
            );
        }
        let document = conformant(&Document::parse(text).unwrap());
        let operation = document.resolve_operation(op).expect("declared operation");
        let contract = match operation
            .output()
            .expect("declared output")
            .prepare(&DefaultEvaluator::new())
        {
            Ok(contract) => contract,
            Err(detail) => panic!("expected ready: {detail:?}"),
        };
        for case in cases.as_array().unwrap() {
            let Some(text) = case["valueText"].as_str() else {
                continue;
            };
            let value = JsonValue::parse(text).unwrap();
            let outcome = contract.explain(&value);
            assert_eq!(label(&outcome), case["expected"], "{}", case["caseId"]);
            if let ValueOutcome::Fails {
                problems,
                problems_complete,
                ..
            } = outcome
            {
                assert!(!problems_complete);
                assert!(!problems.is_empty());
                assert!(problems.iter().all(|p| {
                    p.code == "oneOf"
                        && p.instance.pointer.as_ref().unwrap() == ""
                        && p.schema.resource.is_none()
                        && p.schema.pointer == format!("/operations/{op}/output/oneOf")
                }));
            }
        }
    }
}

fn document(schema: &str) -> Document {
    Document::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"probe":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
}
fn resources(entries: &[(&str, &str)]) -> ResourceSet {
    ResourceSet::new(entries.iter().map(|(uri, text)| SchemaResource {
        uri: (*uri).into(),
        document: JsonValue::parse(text).unwrap(),
    }))
    .unwrap()
}
fn evaluate(schema: &str, value: &str) -> ValueOutcome {
    ready(&document(schema), ResourceSet::default(), Limits::default())
        .explain(&JsonValue::parse(value).unwrap())
}
#[test]
fn nested_endpoint_success_does_not_prove_resource_independence() {
    // The authored result is U == V: uniform FF/TT succeeds, mixed FT/TF fails.
    // Merely checking uniform completions would fabricate satisfaction.
    let document = document(
        r#"{"oneOf":[{"oneOf":[{"$ref":"https://sol-independent.invalid/U"},{"$ref":"https://sol-independent.invalid/V"}]},true]}"#,
    );
    let value = JsonValue::parse("null").unwrap();
    for completion in [
        None,
        Some((false, false)),
        Some((false, true)),
        Some((true, false)),
        Some((true, true)),
    ] {
        let supplied = completion.map_or_else(ResourceSet::default, |(u, v)| {
            resources(&[
                (
                    "https://sol-independent.invalid/U",
                    if u { "true" } else { "false" },
                ),
                (
                    "https://sol-independent.invalid/V",
                    if v { "true" } else { "false" },
                ),
            ])
        });
        let contract = ready(&document, supplied, Limits::default());
        let expected = match completion {
            None => "no-verdict",
            Some((u, v)) if u == v => "satisfies",
            Some(_) => "fails",
        };
        assert_eq!(label(&contract.explain(&value)), expected);
    }
}
#[test]
fn frozen_owner_controls_cover_endpoints_siblings_nested_and_original_summaries() {
    let controls: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/oneof/owner-cases.json")).unwrap();
    for case in controls["cases"].as_array().unwrap() {
        let schema = case["schema"].to_string();
        let entries: Vec<_> = case["resources"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.iter())
            .map(|(uri, schema)| (uri.clone(), schema.to_string()))
            .collect();
        let entries: Vec<_> = entries
            .iter()
            .map(|(u, s)| (u.as_str(), s.as_str()))
            .collect();
        let contract = ready(&document(&schema), resources(&entries), Limits::default());
        let outcome =
            contract.explain(&JsonValue::parse(case["valueText"].as_str().unwrap()).unwrap());
        assert_eq!(
            label(&outcome),
            case["expected"],
            "{}: {outcome:?}",
            case["id"]
        );
        if let ValueOutcome::Fails {
            problems,
            problems_complete,
            ..
        } = outcome
        {
            assert!(!problems_complete, "{}", case["id"]);
            assert!(!problems.is_empty());
            if let Some(pointer) = case["summary"].as_str() {
                assert!(
                    problems.iter().any(|p| p.code == "oneOf"
                        && p.instance.pointer.as_ref().unwrap()
                            == case["instancePointer"].as_str().unwrap_or("")
                        && p.schema.pointer == pointer
                        && p.schema.resource.as_deref() == case["summaryResource"].as_str()),
                    "{}: {problems:?}",
                    case["id"]
                );
            }
            if let Some(code) = case["requiredCode"].as_str() {
                assert!(problems.iter().any(|p| p.code == code));
            }
            if let Some(code) = case["forbiddenCode"].as_str() {
                assert!(problems.iter().all(|p| p.code != code));
            }
            for problem in problems {
                assert!(!problem.message.contains("matched"));
                assert!(!problem.message.contains("sdk-bounds"));
                assert!(
                    !problem
                        .schema
                        .pointer
                        .as_str()
                        .unwrap()
                        .contains("/$defs/n")
                );
            }
        }
    }
}
#[test]
fn distinct_hole_truth_tables_are_corroborated_by_all_small_complete_resource_sets() {
    // Independent complete-schema calls establish each possible actual truth.
    // No expected Boolean is calculated by the projector or from endpoint agreement.
    for n in 1..=3u32 {
        for mut code in 0..3usize.pow(n) {
            let mut branches = Vec::new();
            let mut holes = Vec::new();
            for i in 0..n {
                match code % 3 {
                    0 => branches.push("false".into()),
                    1 => branches.push("true".into()),
                    _ => {
                        let uri = format!("https://truth.invalid/u{i}");
                        branches.push(format!(r#"{{"$ref":"{uri}"}}"#));
                        holes.push(uri);
                    }
                }
                code /= 3;
            }
            if holes.is_empty() {
                continue;
            }
            let schema = format!(r#"{{"oneOf":[{}]}}"#, branches.join(","));
            let document = document(&schema);
            let mut actual_truths = std::collections::BTreeSet::new();
            for bits in 0..1usize << holes.len() {
                let entries: Vec<_> = holes
                    .iter()
                    .enumerate()
                    .map(|(i, uri)| {
                        (
                            uri.as_str(),
                            if bits & (1 << i) == 0 {
                                "false"
                            } else {
                                "true"
                            },
                        )
                    })
                    .collect();
                let complete = ready(&document, resources(&entries), Limits::default());
                actual_truths.insert(label(&complete.explain(&JsonValue::null())));
            }
            let expected = if actual_truths.len() == 1 {
                *actual_truths.first().unwrap()
            } else {
                "no-verdict"
            };
            assert_eq!(label(&evaluate(&schema, "null")), expected, "{schema}");
        }
    }
}
#[test]
fn positive_contexts_shared_aliases_and_advancing_recursion_compose() {
    let known = r#"{"oneOf":[{"type":"object","properties":{"p":{"$ref":"https://oneof.invalid/U"}}},{"type":"string"}]}"#;
    for (schema, value, expected) in [
        (format!(r#"{{"allOf":[{known},true]}}"#), "{}", "satisfies"),
        (format!(r#"{{"anyOf":[{known},false]}}"#), "{}", "satisfies"),
        (
            format!(r#"{{"properties":{{"x":{known}}}}}"#),
            r#"{"x":false}"#,
            "fails",
        ),
        (
            format!(r#"{{"patternProperties":{{"^x$":{known}}}}}"#),
            r#"{"x":{}}"#,
            "satisfies",
        ),
        (
            format!(r#"{{"additionalProperties":{known}}}"#),
            r#"{"x":{}}"#,
            "satisfies",
        ),
        (
            format!(r#"{{"propertyNames":{known}}}"#),
            r#"{"x":null}"#,
            "satisfies",
        ),
        (
            format!(r#"{{"dependentSchemas":{{"x":{known}}}}}"#),
            r#"{"x":null}"#,
            "satisfies",
        ),
        (
            format!(r#"{{"prefixItems":[{known}],"items":false}}"#),
            "[{}]",
            "satisfies",
        ),
        (format!(r#"{{"items":{known}}}"#), "[{}]", "satisfies"),
        (
            format!(
                r#"{{"$id":"https://oneof.invalid/root","$defs":{{"s":{known}}},"allOf":[{{"$ref":"https://oneof.invalid/root#/$defs/s"}}],"oneOf":[{{"$ref":"https://oneof.invalid/root#/$defs/s"}},false]}}"#
            ),
            "{}",
            "satisfies",
        ),
    ] {
        assert_eq!(label(&evaluate(&schema, value)), expected, "{schema}");
    }
    let recursive = r##"{"$id":"https://oneof.invalid/tree","oneOf":[{"type":"object","properties":{"next":{"$ref":"#"},"external":{"$ref":"https://oneof.invalid/U"}}},{"type":"null"}]}"##;
    for (value, expected) in [
        ("null", "satisfies"),
        (r#"{"next":{"next":null}}"#, "satisfies"),
        (r#"{"next":false}"#, "fails"),
        (r#"{"next":{"external":1}}"#, "no-verdict"),
    ] {
        assert_eq!(label(&evaluate(recursive, value)), expected);
    }
}
#[test]
fn unsupported_influence_and_known_closure_defects_remain_refusals() {
    for (schema, reason, code) in [
        (
            r#"{"oneOf":[true,{"not":{"$ref":"https://oneof.invalid/U"}}]}"#,
            NoVerdictReason::ResourceUnavailable,
            "resource-unavailable",
        ),
        (
            r#"{"oneOf":[true,{"if":{"$ref":"https://oneof.invalid/U"},"then":false}]}"#,
            NoVerdictReason::ResourceUnavailable,
            "resource-unavailable",
        ),
        (
            r#"{"oneOf":[true,{"contains":{"$ref":"https://oneof.invalid/U"}}]}"#,
            NoVerdictReason::ResourceUnavailable,
            "resource-unavailable",
        ),
        (
            r#"{"oneOf":[true,{"$ref":"https://oneof.invalid/U"}],"unevaluatedProperties":false}"#,
            NoVerdictReason::ResourceUnavailable,
            "resource-unavailable",
        ),
        (
            r#"{"oneOf":[{"$ref":"https://oneof.invalid/U"},{"pattern":"["}]}"#,
            NoVerdictReason::ConservativePreparation,
            "schema-pattern-compilation",
        ),
        (
            r##"{"oneOf":[{"$ref":"https://oneof.invalid/U"},{"$ref":"#/operations/probe/input/title"}],"title":"not a schema"}"##,
            NoVerdictReason::ConservativePreparation,
            "non-schema-target",
        ),
        (
            r##"{"$id":"https://oneof.invalid/root","oneOf":[{"$ref":"https://oneof.invalid/U"},{"$ref":"#"}]}"##,
            NoVerdictReason::ConservativePreparation,
            "in-place-cycle",
        ),
    ] {
        if code == "non-schema-target" {
            let validation = document(schema).validate().unwrap();
            assert_eq!(validation.report().evidence["OBI-12"], Evidence::Violated);
            assert!(validation.conformant().is_none());
            continue;
        }
        let setup = conformant(&document(schema))
            .resolve_operation("probe")
            .expect("declared operation")
            .input()
            .expect("declared input")
            .prepare(&DefaultEvaluator::new());
        let Err(detail) = setup else {
            panic!("{setup:?}")
        };
        assert_eq!(detail.reason, reason, "{schema}");
        assert_eq!(detail.code, code, "{schema}");
    }
}
#[test]
fn explained_privacy_and_partial_work_do_not_fabricate_results() {
    let schema = r#"{"oneOf":[true,true,{"$ref":"https://oneof.invalid/U"}]}"#;
    let value = JsonValue::parse(r#"{"secret":"do-not-render"}"#).unwrap();
    let document = document(schema);
    let regular = ready(&document, ResourceSet::default(), Limits::default());
    let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = regular.explain(&value)
    else {
        panic!()
    };
    assert!(!problems_complete);
    assert_eq!(problems.len(), 1);
    assert!(
        !serde_json::to_string(&problems)
            .unwrap()
            .contains("do-not-render")
    );
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    assert!(
        matches!(regular.explain_with_control(&value, &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
    assert_eq!(label(&regular.explain(&value)), "fails");
    let limited = ready(
        &document,
        ResourceSet::default(),
        Limits {
            evaluation_steps: 0,
            ..Limits::default()
        },
    );
    assert!(
        matches!(limited.explain(&value), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
    );
}

#[test]
fn adapters_receive_unmodified_requests_and_each_bound_compiles_without_the_other_registry() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    struct Capture {
        calls: AtomicUsize,
        bounds: Mutex<Option<EvaluationBounds>>,
    }
    impl SchemaEvaluator for Capture {
        fn prepare(
            &self,
            request: &SchemaRequest,
            control: &dyn WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(request.entry().get("oneOf").is_some());
            assert_eq!(
                request.evaluation_program(control).unwrap_err().reason,
                NoVerdictReason::ResourceUnavailable
            );
            *self.bounds.lock().unwrap() = Some(request.evaluation_bounds(control)?);
            Err(NoVerdict::new(
                NoVerdictReason::EvaluatorFailure,
                "my-adapter",
                "unchanged custom refusal",
            ))
        }
    }
    #[derive(Clone)]
    struct Reject;
    impl jsonschema::Retrieve for Reject {
        fn retrieve(
            &self,
            _: &jsonschema::Uri<String>,
        ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
            panic!("closed programs must not retrieve")
        }
    }
    let custom = Capture {
        calls: AtomicUsize::new(0),
        bounds: Mutex::new(None),
    };
    let proof = conformant(&document(
        r#"{"oneOf":[true,{"$ref":"https://oneof.invalid/U"}]}"#,
    ));
    let operation = proof
        .resolve_operation("probe")
        .expect("declared operation");
    let Err(detail) = operation.input().expect("declared input").prepare(&custom) else {
        panic!()
    };
    assert_eq!(detail.code, "my-adapter");
    assert_eq!(custom.calls.load(Ordering::SeqCst), 1);
    let EvaluationBounds { lower, upper, .. } = custom.bounds.lock().unwrap().take().unwrap();
    drop(operation);
    drop(proof);
    drop(custom);
    for (program, expected) in [(lower, false), (upper, true)] {
        let mut registry = jsonschema::Registry::new().retriever(Reject);
        for resource in &program.resources {
            assert_ne!(resource.uri, "https://oneof.invalid/U");
            registry = registry
                .add(
                    &resource.uri,
                    serde_json::from_str::<serde_json::Value>(resource.document.text()).unwrap(),
                )
                .unwrap();
        }
        let registry = registry.prepare().unwrap();
        let root = registry
            .resolver(jsonschema::Uri::parse(program.entry_uri.clone()).unwrap())
            .lookup("")
            .unwrap();
        let validator = jsonschema::options()
            .with_registry(&registry)
            .with_retriever(Reject)
            .build(root.contents())
            .unwrap();
        assert_eq!(validator.is_valid(&serde_json::Value::Null), expected);
    }
}
#[test]
fn old_partial_and_new_complete_snapshots_remain_independent() {
    let document = document(r#"{"oneOf":[true,{"$ref":"https://oneof.invalid/U"}]}"#);
    let old = ready(&document, ResourceSet::default(), Limits::default());
    let no_match = ready(
        &document,
        resources(&[("https://oneof.invalid/U", "false")]),
        Limits::default(),
    );
    let two_match = ready(
        &document,
        resources(&[("https://oneof.invalid/U", "true")]),
        Limits::default(),
    );
    drop(document);
    for (contract, expected) in [
        (old.clone(), "no-verdict"),
        (no_match, "satisfies"),
        (two_match, "fails"),
    ] {
        assert_eq!(label(&contract.explain(&JsonValue::null())), expected);
    }
    assert_eq!(label(&old.explain(&JsonValue::null())), "no-verdict");
}
#[test]
fn bounded_nested_runtime_and_original_exact_details_retain_separate_policies() {
    let mut nested = r#"{"properties":{"p":{"$ref":"https://oneof.invalid/U"}}}"#.to_owned();
    for _ in 0..6 {
        nested = format!(r#"{{"oneOf":[{nested},false]}}"#);
    }
    let document = document(&nested);
    let finite = ready(
        &document,
        ResourceSet::default(),
        Limits {
            evaluation_steps: 30,
            ..Limits::default()
        },
    );
    assert!(
        matches!(finite.explain(&JsonValue::parse("{}").unwrap()), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
    );
    let schema = r#"{"minimum":0.290000000000000000001,"oneOf":[false,{"properties":{"p":{"$ref":"https://oneof.invalid/U"}}}]}"#;
    let proof = conformant(&self::document(schema));
    let operation = proof
        .resolve_operation("probe")
        .expect("declared operation");
    let Ok(contract) = operation
        .input()
        .expect("declared input")
        .prepare(&DefaultEvaluator::new())
    else {
        panic!()
    };
    let ValueOutcome::Fails { problems, .. } = contract.explain(&JsonValue::parse("0.29").unwrap())
    else {
        panic!()
    };
    assert!(problems.iter().all(|p| p.code == "minimum"));
    let rendered = serde_json::to_string(&problems).unwrap();
    assert!(rendered.contains("0.290000000000000000001"));
    assert!(!rendered.contains("sdk-bounds"));
}

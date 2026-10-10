use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::Arc;

fn label(outcome: &ValueOutcome) -> &'static str {
    match outcome {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::NoVerdict { detail } => {
            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
            "no-verdict"
        }
    }
}
fn ready(
    document: &ParsedDocument,
    op: &str,
    side: Side,
    resources: ResourceSet,
    limits: Limits,
) -> PreparedContract {
    match document
        .value_contracts(Arc::new(DefaultEvaluator::with_limits(limits)), resources)
        .unwrap()
        .prepare(op, side)
    {
        ContractPreparation::Ready(contract) => contract,
        other => panic!("expected ready: {other:?}"),
    }
}
#[test]
fn frozen_c22_original_values_and_renamed_operation_keep_their_independent_truths() {
    let original = include_str!("fixtures/oneof/C22.json");
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/oneof/C22-cases.json")).unwrap();
    // Literal primary/dependency-key rename; contract/value/resource bytes stay exact.
    for op in ["2x.thumbnail", "thumbnail.make"] {
        let mut text = original.replace("\"2x.thumbnail\"", &format!("\"{op}\""));
        if op != "2x.thumbnail" {
            text = text.replace(
                r#""aliases":["thumb.render""#,
                r#""aliases":["2x.thumbnail","thumb.render""#,
            );
        }
        let document = ParsedDocument::parse(text).unwrap();
        let contract = ready(
            &document,
            op,
            Side::Output,
            ResourceSet::default(),
            Limits::default(),
        );
        assert!(matches!(
            contract.resource_completeness(),
            ResourceCompleteness::Incomplete { .. }
        ));
        for case in cases.as_array().unwrap() {
            let Some(text) = case["valueText"].as_str() else {
                continue;
            };
            let value = JsonValue::parse(text).unwrap();
            let outcome = contract.validate(&value);
            assert_eq!(label(&outcome), case["expected"], "{}", case["caseId"]);
            if let ValueOutcome::Fails {
                problems,
                problems_complete,
            } = outcome
            {
                assert!(problems_complete);
                assert!(!problems.is_empty());
                assert!(problems.iter().all(|p| {
                    p.code == "oneOf"
                        && p.instance_pointer.is_empty()
                        && p.schema_location.as_ref().is_some_and(|at| {
                            at.resource.is_none()
                                && at.pointer == format!("/operations/{op}/output/oneOf")
                        })
                }));
            }
        }
    }
}

fn document(schema: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(
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
    ready(
        &document(schema),
        "probe",
        Side::Input,
        ResourceSet::default(),
        Limits::default(),
    )
    .validate(&JsonValue::parse(value).unwrap())
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
        let contract = ready(&document, "probe", Side::Input, supplied, Limits::default());
        assert_eq!(
            matches!(
                contract.resource_completeness(),
                ResourceCompleteness::Complete
            ),
            completion.is_some()
        );
        let expected = match completion {
            None => "no-verdict",
            Some((u, v)) if u == v => "satisfies",
            Some(_) => "fails",
        };
        assert_eq!(label(&contract.validate(&value)), expected);
    }
}
#[test]
fn frozen_owner_controls_cover_endpoints_siblings_nested_and_original_summaries() {
    let controls: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/oneof/owner-cases.json")).unwrap();
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
        let contract = ready(
            &document(&schema),
            "probe",
            Side::Input,
            resources(&entries),
            Limits::default(),
        );
        let outcome =
            contract.validate(&JsonValue::parse(case["valueText"].as_str().unwrap()).unwrap());
        assert_eq!(
            label(&outcome),
            case["expected"],
            "{}: {outcome:?}",
            case["id"]
        );
        if let ValueOutcome::Fails {
            problems,
            problems_complete,
        } = outcome
        {
            assert!(problems_complete, "{}", case["id"]);
            assert!(!problems.is_empty());
            if let Some(pointer) = case["summary"].as_str() {
                assert!(
                    problems.iter().any(|p| p.code == "oneOf"
                        && p.instance_pointer == case["instancePointer"].as_str().unwrap_or("")
                        && p.schema_location
                            .as_ref()
                            .is_some_and(|l| l.pointer == pointer
                                && l.resource.as_deref() == case["summaryResource"].as_str())),
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
                        .schema_location
                        .unwrap()
                        .pointer
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
                let complete = ready(
                    &document,
                    "probe",
                    Side::Input,
                    resources(&entries),
                    Limits::default(),
                );
                assert!(matches!(
                    complete.resource_completeness(),
                    ResourceCompleteness::Complete
                ));
                actual_truths.insert(label(&complete.validate(&JsonValue::null())));
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
        let setup = document(schema)
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
            .unwrap()
            .prepare("probe", Side::Input);
        let ContractPreparation::NoVerdict { detail } = setup else {
            panic!("{setup:?}")
        };
        assert_eq!(detail.reason, reason, "{schema}");
        assert_eq!(detail.code, code, "{schema}");
    }
}
#[test]
fn diagnostic_bytes_privacy_and_partial_work_do_not_fabricate_results() {
    let schema = r#"{"oneOf":[true,true,{"$ref":"https://oneof.invalid/U"}]}"#;
    let value = JsonValue::parse(r#"{"secret":"do-not-render"}"#).unwrap();
    let document = document(schema);
    let regular = ready(
        &document,
        "probe",
        Side::Input,
        ResourceSet::default(),
        Limits::default(),
    );
    let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = regular.validate(&value)
    else {
        panic!()
    };
    assert!(problems_complete);
    assert_eq!(problems.len(), 1);
    let problem = &problems[0];
    assert_eq!(problem.code, "oneOf");
    assert_eq!(
        problem.message,
        "value does not satisfy exactly one alternative"
    );
    assert!(
        !serde_json::to_string(problem)
            .unwrap()
            .contains("do-not-render")
    );
    let bytes = problem.message.len()
        + problem.code.len()
        + problem.instance_pointer.len()
        + problem.schema_location.as_ref().unwrap().pointer.len();
    for allowance in [0, bytes - 1, bytes] {
        let contract = ready(
            &document,
            "probe",
            Side::Input,
            ResourceSet::default(),
            Limits {
                diagnostic_bytes: allowance,
                ..Limits::default()
            },
        );
        let ValueOutcome::Fails {
            problems,
            problems_complete,
        } = contract.validate(&value)
        else {
            panic!()
        };
        assert_eq!(problems.len(), usize::from(allowance == bytes));
        assert_eq!(problems_complete, allowance == bytes);
    }
    let cancelled = WorkControl::new();
    cancelled.cancel();
    assert!(
        matches!(regular.validate_with_control(&value, &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
    assert_eq!(label(&regular.validate(&value)), "fails");
    let limited = ready(
        &document,
        "probe",
        Side::Input,
        ResourceSet::default(),
        Limits {
            evaluation_steps: 0,
            ..Limits::default()
        },
    );
    assert!(
        matches!(limited.validate(&value), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
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
            control: &WorkControl,
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
    let custom = Arc::new(Capture {
        calls: AtomicUsize::new(0),
        bounds: Mutex::new(None),
    });
    let context = document(r#"{"oneOf":[true,{"$ref":"https://oneof.invalid/U"}]}"#)
        .value_contracts(custom.clone(), ResourceSet::default())
        .unwrap();
    let ContractPreparation::NoVerdict { detail } = context.prepare("probe", Side::Input) else {
        panic!()
    };
    assert_eq!(detail.code, "my-adapter");
    assert_eq!(custom.calls.load(Ordering::SeqCst), 1);
    let (lower, upper, _) = custom.bounds.lock().unwrap().take().unwrap().into_parts();
    drop(context);
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
    let old = ready(
        &document,
        "probe",
        Side::Input,
        ResourceSet::default(),
        Limits::default(),
    );
    let no_match = ready(
        &document,
        "probe",
        Side::Input,
        resources(&[("https://oneof.invalid/U", "false")]),
        Limits::default(),
    );
    let two_match = ready(
        &document,
        "probe",
        Side::Input,
        resources(&[("https://oneof.invalid/U", "true")]),
        Limits::default(),
    );
    drop(document);
    for (contract, expected) in [
        (old.clone(), "no-verdict"),
        (no_match, "satisfies"),
        (two_match, "fails"),
    ] {
        assert_eq!(label(&contract.validate(&JsonValue::null())), expected);
    }
    assert_eq!(label(&old.validate(&JsonValue::null())), "no-verdict");
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
        "probe",
        Side::Input,
        ResourceSet::default(),
        Limits {
            evaluation_steps: 30,
            ..Limits::default()
        },
    );
    assert!(
        matches!(finite.validate(&JsonValue::parse("{}").unwrap()), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
    );
    let schema = r#"{"minimum":0.290000000000000000001,"oneOf":[false,{"properties":{"p":{"$ref":"https://oneof.invalid/U"}}}]}"#;
    let contracts = self::document(schema)
        .value_contracts(
            Arc::new(DefaultEvaluator::new().with_schema_details(true)),
            ResourceSet::default(),
        )
        .unwrap();
    let ContractPreparation::Ready(contract) = contracts.prepare("probe", Side::Input) else {
        panic!()
    };
    let ValueOutcome::Fails { problems, .. } =
        contract.validate(&JsonValue::parse("0.29").unwrap())
    else {
        panic!()
    };
    assert!(problems.iter().all(|p| p.code == "minimum"));
    let rendered = serde_json::to_string(&problems).unwrap();
    assert!(rendered.contains("0.290000000000000000001"));
    assert!(!rendered.contains("sdk-bounds"));
}

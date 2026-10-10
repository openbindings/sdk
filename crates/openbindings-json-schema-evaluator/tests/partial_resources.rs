use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
const U: &str = "https://review.invalid/U";
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document(schema: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
}
fn resources(values: &[(&str, &str)]) -> ResourceSet {
    ResourceSet::new(values.iter().map(|(uri, text)| SchemaResource {
        uri: (*uri).into(),
        document: json(text),
    }))
    .unwrap()
}
fn context(schema: &str, supplied: ResourceSet, evaluator: DefaultEvaluator) -> ValueContracts {
    document(schema)
        .value_contracts(Arc::new(evaluator), supplied)
        .unwrap()
}
fn ready(schema: &str) -> PreparedContract {
    let result =
        context(schema, ResourceSet::default(), DefaultEvaluator::new()).prepare("op", Side::Input);
    let ContractPreparation::Ready(contract) = result else {
        panic!("{result:?}")
    };
    contract
}
fn label(result: ValueOutcome) -> &'static str {
    match result {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::NoVerdict { detail }
            if detail.reason == NoVerdictReason::ResourceUnavailable =>
        {
            "unavailable"
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}
fn refusal(schema: &str, supplied: ResourceSet) -> NoVerdict {
    match context(schema, supplied, DefaultEvaluator::new()).prepare("op", Side::Input) {
        ContractPreparation::NoVerdict { detail } => detail,
        other => panic!("expected preparation refusal: {other:?}"),
    }
}
#[test]
fn uniform_ready_phase_including_bare_holes_and_static_aliases() {
    for schema in [
        format!(r#"{{"$ref":"{U}"}}"#),
        format!(r#"{{"allOf":[{{"$ref":"{U}"}}]}}"#),
        format!(
            r##"{{"$id":"https://review.invalid/root","$defs":{{"Alias":{{"$ref":"{U}"}}}},"$ref":"#/$defs/Alias"}}"##
        ),
    ] {
        let contract = ready(&schema);
        for text in ["null", "7", r#""secret""#, "{}", "[]"] {
            let ValueOutcome::NoVerdict { detail } = contract.validate(&json(text)) else {
                panic!("bare hole decided {text}")
            };
            assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
            assert!(detail.location.unwrap().pointer.ends_with("/$ref"));
            assert!(!detail.message.contains(U));
        }
    }
}
#[test]
fn all_positive_applicators_have_decided_and_dependent_witnesses() {
    let hole = format!(r#"{{"$ref":"{U}"}}"#);
    for (schema, decided, expected, dependent) in [
        (
            format!(r#"{{"properties":{{"x":{hole}}}}}"#),
            "{}",
            "satisfies",
            r#"{"x":1}"#,
        ),
        (
            format!(r#"{{"patternProperties":{{"^x":{hole}}}}}"#),
            r#"{"y":1}"#,
            "satisfies",
            r#"{"x":1}"#,
        ),
        (
            format!(r#"{{"properties":{{"x":true}},"additionalProperties":{hole}}}"#),
            r#"{"x":1}"#,
            "satisfies",
            r#"{"y":1}"#,
        ),
        (
            format!(r#"{{"propertyNames":{hole}}}"#),
            "{}",
            "satisfies",
            r#"{"x":1}"#,
        ),
        (
            format!(r#"{{"dependentSchemas":{{"x":{hole}}}}}"#),
            "{}",
            "satisfies",
            r#"{"x":1}"#,
        ),
        (
            format!(r#"{{"prefixItems":[{hole}],"items":false}}"#),
            "[1,2]",
            "fails",
            "[1]",
        ),
        (
            format!(r#"{{"prefixItems":[true],"items":{hole}}}"#),
            "[1]",
            "satisfies",
            "[1,2]",
        ),
        (
            format!(r#"{{"allOf":[{{"type":"number"}},{hole}]}}"#),
            r#""x""#,
            "fails",
            "1",
        ),
        (
            format!(r#"{{"anyOf":[{{"const":1}},{hole}]}}"#),
            "1",
            "satisfies",
            "2",
        ),
        (
            format!(r#"{{"$ref":"{U}","type":"number"}}"#),
            r#""x""#,
            "fails",
            "1",
        ),
    ] {
        let contract = ready(&schema);
        assert_eq!(
            label(contract.validate(&json(decided))),
            expected,
            "{schema}"
        );
        assert_eq!(
            label(contract.validate(&json(dependent))),
            "unavailable",
            "{schema}"
        );
        // Independent finite completions corroborate (but do not supply) the proof.
        for completion in [
            "true",
            "false",
            r#"{"const":1}"#,
            r#"{"not":{"type":"number"}}"#,
        ] {
            let full = context(
                &schema,
                resources(&[(U, completion)]),
                DefaultEvaluator::new(),
            )
            .prepare("op", Side::Input);
            let ContractPreparation::Ready(full) = full else {
                panic!("{full:?}")
            };
            assert_eq!(
                label(full.validate(&json(decided))),
                expected,
                "{schema}: {completion}"
            );
        }
    }
}
#[test]
fn property_pattern_and_prefix_selection_survive_substitution() {
    for (schema, value) in [
        (
            format!(r#"{{"properties":{{"x":{{"$ref":"{U}"}}}},"additionalProperties":false}}"#),
            r#"{"x":1}"#,
        ),
        (
            format!(
                r#"{{"patternProperties":{{"^x":{{"$ref":"{U}"}}}},"additionalProperties":false}}"#
            ),
            r#"{"xyz":1}"#,
        ),
        (
            format!(r#"{{"prefixItems":[{{"$ref":"{U}"}}],"items":false}}"#),
            "[1]",
        ),
    ] {
        assert_eq!(label(ready(&schema).validate(&json(value))), "unavailable");
    }
}
#[test]
fn active_unknown_can_fail_known_constraint_and_never_emits_lower_diagnostics() {
    let schema =
        format!(r#"{{"properties":{{"external":{{"$ref":"{U}"}}}},"required":["private-name"]}}"#);
    let evaluator = DefaultEvaluator::new().with_schema_details(true);
    let ContractPreparation::Ready(contract) =
        context(&schema, ResourceSet::default(), evaluator).prepare("op", Side::Input)
    else {
        panic!()
    };
    let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = contract.validate(&json(r#"{"external":7}"#))
    else {
        panic!()
    };
    assert!(problems_complete);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "required");
    assert_eq!(
        problems[0].schema_location.as_ref().unwrap().pointer,
        "/operations/op/input/required"
    );
    assert!(problems[0].details.is_some());
    assert!(!problems[0].message.contains("private-name"));
    let serialized = serde_json::to_string(&problems).unwrap();
    assert!(!serialized.contains("sdk-bounds"));
    let zero = DefaultEvaluator::with_limits(Limits {
        diagnostic_bytes: 0,
        ..Limits::default()
    });
    let ContractPreparation::Ready(contract) =
        context(&schema, ResourceSet::default(), zero).prepare("op", Side::Input)
    else {
        panic!()
    };
    assert!(
        matches!(contract.validate(&json(r#"{"external":7}"#)), ValueOutcome::Fails { problems, problems_complete: false } if problems.is_empty())
    );
}
#[test]
fn closed_nonpositive_subgraphs_remain_fixed_predicates() {
    for closed in [
        r#"{"oneOf":[{"type":"string"},{"type":"number"}]}"#,
        r#"{"not":{"const":false}}"#,
        r#"{"if":{"type":"string"},"then":{"minLength":2},"else":{"type":"number"}}"#,
        r#"{"contains":{"const":1},"minContains":1,"maxContains":1}"#,
    ] {
        let schema =
            format!(r#"{{"properties":{{"external":{{"$ref":"{U}"}},"known":{closed}}}}}"#);
        let contract = ready(&schema);
        for text in [
            r#"{"known":"ok"}"#,
            r#"{"known":false}"#,
            r#"{"known":[1,2]}"#,
        ] {
            let full = context(&schema, resources(&[(U, "false")]), DefaultEvaluator::new())
                .prepare("op", Side::Input);
            let ContractPreparation::Ready(full) = full else {
                panic!()
            };
            assert_eq!(
                label(contract.validate(&json(text))),
                label(full.validate(&json(text)))
            );
        }
    }
}
#[test]
fn nonpositive_hole_influence_and_annotation_dynamic_hazards_refuse() {
    let hole = format!(r#"{{"$ref":"{U}"}}"#);
    for schema in [
        format!(r#"{{"not":{hole}}}"#),
        format!(r#"{{"oneOf":[{hole},true]}}"#),
        format!(r#"{{"if":{hole},"then":false,"else":true}}"#),
        format!(r#"{{"if":true,"then":{hole}}}"#),
        format!(r#"{{"if":false,"else":{hole}}}"#),
        format!(r#"{{"contains":{hole},"maxContains":1}}"#),
        format!(r#"{{"anyOf":[true,{hole}],"unevaluatedProperties":false}}"#),
        format!(r#"{{"anyOf":[true,{hole}],"unevaluatedItems":false}}"#),
        format!(r#"{{"$dynamicAnchor":"node","properties":{{"x":{hole}}}}}"#),
        format!(r#"{{"properties":{{"x":{hole}}},"$dynamicRef":"{U}"}}"#),
        format!(
            r##"{{"$id":"https://review.invalid/root","$defs":{{"shared":{hole}}},"properties":{{"x":{{"$ref":"#/$defs/shared"}},"y":{{"oneOf":[{{"$ref":"#/$defs/shared"}},true]}}}}}}"##
        ),
        format!(
            r#"{{"anyOf":[{{"properties":{{"a":{hole}}}}},{{"not":{{"properties":{{"b":{hole}}}}}}}]}}"#
        ),
        format!(r#"{{"oneOf":[{{"properties":{{"a":{hole}}}}},{{"properties":{{"b":{hole}}}}}]}}"#),
    ] {
        assert_eq!(
            refusal(&schema, ResourceSet::default()).reason,
            NoVerdictReason::ConservativePreparation,
            "{schema}"
        );
    }
}
#[test]
fn shared_holes_and_advancing_recursion_reach_a_fixed_point() {
    let schema = format!(
        r##"{{"$id":"https://review.invalid/root","type":"object","properties":{{"next":{{"$ref":"#"}},"a":{{"$ref":"{U}"}},"b":{{"$ref":"{U}"}}}}}}"##
    );
    let contract = ready(&schema);
    for (value, expected) in [
        (r#"{"next":{"next":{}}}"#, "satisfies"),
        (r#"{"a":1,"b":2}"#, "unavailable"),
        (r#"{"a":1,"next":7}"#, "fails"),
    ] {
        assert_eq!(label(contract.validate(&json(value))), expected);
    }
}
#[test]
fn ignored_and_opaque_positions_never_create_evaluation_edges() {
    let schema = format!(
        r#"{{"properties":{{"x":{{"$ref":"{U}"}}}},"then":{{"not":{{"$ref":"{U}"}}}},"else":{{"$dynamicRef":"{U}"}},"$defs":{{"unused":{{"unevaluatedProperties":false}}}},"default":{{"oneOf":[{{"$ref":"{U}"}}]}},"contentSchema":{{"$ref":"{U}"}}}}"#
    );
    assert_eq!(label(ready(&schema).validate(&json("{}"))), "satisfies");
}
#[test]
fn missing_carriers_are_distinct_from_bad_fragments_or_known_targets() {
    for fragment in ["", "#/properties/x", "#valid_anchor", "#/%C3%A9~1x"] {
        let schema = format!(r#"{{"anyOf":[true,{{"$ref":"{U}{fragment}"}}]}}"#);
        assert_eq!(label(ready(&schema).validate(&json("0"))), "satisfies");
    }
    for fragment in ["#/~2", "#/~", "#9invalid", "#/%ff", "#%"] {
        let schema = format!(r#"{{"anyOf":[true,{{"$ref":"{U}{fragment}"}}]}}"#);
        assert_ne!(
            refusal(&schema, ResourceSet::default()).reason,
            NoVerdictReason::ResourceUnavailable
        );
    }
    for fragment in ["#/type", "#missing"] {
        let schema = format!(r#"{{"anyOf":[true,{{"$ref":"{U}{fragment}"}}]}}"#);
        assert_eq!(
            refusal(&schema, resources(&[(U, r#"{"type":"string"}"#)])).reason,
            NoVerdictReason::ConservativePreparation
        );
    }
}
#[test]
fn known_closure_checks_continue_past_holes() {
    let schema = format!(r#"{{"allOf":[{{"$ref":"{U}"}},{{"$ref":"https://review.invalid/K"}}]}}"#);
    for (known, code) in [
        (r#"{"type":42}"#, "invalid-schema"),
        (
            r#"{"$schema":"https://unsupported.invalid/dialect"}"#,
            "schema-dialect",
        ),
        (r#"{"pattern":"["}"#, "schema-pattern-compilation"),
        (r##"{"$ref":"#"}"##, "in-place-cycle"),
        (r#"{"const":"\ud800"}"#, "lone-surrogate-schema"),
    ] {
        assert_eq!(
            refusal(&schema, resources(&[("https://review.invalid/K", known)])).code,
            code,
            "{known}"
        );
    }
}
#[test]
fn exact_literals_survive_both_programs() {
    for (token, other) in [
        ("0.2900000000000000000001", "0.29"),
        ("9007199254740993", "9007199254740992"),
        ("1e999999999999999999", "1e999999999999999998"),
    ] {
        let schema =
            format!(r#"{{"properties":{{"external":{{"$ref":"{U}"}},"n":{{"const":{token}}}}}}}"#);
        let contract = ready(&schema);
        assert_eq!(
            label(contract.validate(&json(&format!(r#"{{"n":{token}}}"#)))),
            "satisfies"
        );
        assert_eq!(
            label(contract.validate(&json(&format!(r#"{{"n":{other}}}"#)))),
            "fails"
        );
    }
}
#[test]
fn old_owners_remain_independent_of_compatible_colliding_and_unsupported_completions() {
    let schema = format!(
        r#"{{"properties":{{"external":{{"$ref":"{U}"}},"name":{{"$ref":"https://review.invalid/K"}}}}}}"#
    );
    let known = ("https://review.invalid/K", r#"{"type":"string"}"#);
    let old_context = context(&schema, resources(&[known]), DefaultEvaluator::new());
    let ContractPreparation::Ready(old) = old_context.prepare("op", Side::Input) else {
        panic!()
    };
    drop(old_context);
    let compatible = context(
        &schema,
        resources(&[known, (U, "true")]),
        DefaultEvaluator::new(),
    );
    let ContractPreparation::Ready(new) = compatible.prepare("op", Side::Input) else {
        panic!()
    };
    assert_eq!(
        label(new.validate(&json(r#"{"external":1,"name":"ok"}"#))),
        "satisfies"
    );
    let collision = r#"{"$defs":{"Alias":{"$id":"https://review.invalid/K","type":"number"}}}"#;
    assert_eq!(
        refusal(&schema, resources(&[known, (U, collision)])).code,
        "ambiguous-resource"
    );
    assert_eq!(
        refusal(
            &schema,
            resources(&[
                known,
                (U, r#"{"$schema":"https://unsupported.invalid/dialect"}"#)
            ])
        )
        .code,
        "schema-dialect"
    );
    assert_eq!(label(old.validate(&json(r#"{"name":"ok"}"#))), "satisfies");
    assert_eq!(
        label(old.validate(&json(r#"{"external":1,"name":"ok"}"#))),
        "unavailable"
    );
}
#[test]
fn custom_evaluator_gets_original_request_and_is_not_retried_or_reinterpreted() {
    struct Custom {
        calls: AtomicUsize,
        original: String,
        captured: Mutex<Option<SchemaRequest>>,
    }
    impl SchemaEvaluator for Custom {
        fn prepare(
            &self,
            request: &SchemaRequest,
            control: &WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(request.entry().text(), self.original);
            assert_eq!(request.supplied_resources().iter().len(), 1);
            assert_eq!(
                request.evaluation_program(control).unwrap_err().reason,
                NoVerdictReason::ResourceUnavailable
            );
            *self.captured.lock().unwrap() = Some(request.clone());
            Err(NoVerdict::new(
                NoVerdictReason::ResourceUnavailable,
                "custom-refusal",
                "custom original refusal",
            ))
        }
    }
    let schema = format!(r#"{{"anyOf":[true,{{"$ref":"{U}"}}]}}"#);
    let custom = Arc::new(Custom {
        calls: AtomicUsize::new(0),
        original: schema.clone(),
        captured: Mutex::new(None),
    });
    let contracts = document(&schema)
        .value_contracts(
            custom.clone(),
            resources(&[("https://review.invalid/unused", "false")]),
        )
        .unwrap();
    let ContractPreparation::NoVerdict { detail } = contracts.prepare("op", Side::Input) else {
        panic!()
    };
    assert_eq!(detail.code, "custom-refusal");
    assert_eq!(custom.calls.load(Ordering::SeqCst), 1);
    let request = custom.captured.lock().unwrap().take().unwrap();
    let bounds = request.evaluation_bounds(&WorkControl::new()).unwrap();
    assert!(bounds.unavailable().is_some());
    assert!(bounds.lower_program().resources.iter().all(|r| r.uri != U));
    let (lower, upper, evidence) = bounds.into_parts();
    drop(request);
    drop(contracts);
    drop(custom);
    assert_eq!(lower.entry_uri, upper.entry_uri);
    assert_eq!(
        evidence.unwrap().location.unwrap().pointer,
        "/operations/op/input/anyOf/1/$ref"
    );
    for program in [lower, upper] {
        let sentinel = program
            .resources
            .iter()
            .find(|r| r.document.kind() == JsonKind::Boolean)
            .unwrap();
        assert_eq!(program.original_location(&sentinel.uri), None);
        assert_eq!(
            program.original_location(&format!("{}#/false", sentinel.uri)),
            None
        );
    }
}
#[test]
fn cancellation_limits_and_concurrent_calls_do_not_poison_prepared_owners() {
    let schema = format!(r#"{{"properties":{{"x":{{"$ref":"{U}"}}}},"type":"object"}}"#);
    let contracts = context(&schema, ResourceSet::default(), DefaultEvaluator::new());
    let cancelled = WorkControl::new();
    cancelled.cancel();
    assert!(
        matches!(contracts.prepare_with_control("op", Side::Input, &cancelled), ContractPreparation::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
    let ContractPreparation::Ready(contract) = contracts.prepare("op", Side::Input) else {
        panic!()
    };
    assert!(
        matches!(contract.validate_with_control(&json("{}"), &cancelled), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::Cancelled)
    );
    let contract = Arc::new(contract);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let contract = contract.clone();
            scope.spawn(move || {
                for _ in 0..10 {
                    assert_eq!(label(contract.validate(&json("{}"))), "satisfies");
                    assert_eq!(label(contract.validate(&json(r#"{"x":1}"#))), "unavailable");
                }
            });
        }
    });
    let low = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: 0,
        ..Limits::default()
    });
    let ContractPreparation::Ready(limited) =
        context(&schema, ResourceSet::default(), low).prepare("op", Side::Input)
    else {
        panic!()
    };
    assert!(
        matches!(limited.validate(&json("{}")), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
    );
}
#[test]
fn original_five_frozen_values_and_empty_resource_sets() {
    let b02 = include_str!("fixtures/partial-resources/B02.json");
    let c02 = include_str!("fixtures/partial-resources/C02.json");
    for (source, operation, input, expected, keyword, supplied_uri, supplied_text) in [
        (
            b02,
            "northbank.settlement.record",
            r#"{"recordId":1,"amount":0.29,"reference":"fee/17"}"#,
            "fails",
            "required",
            "https://northbank.example.invalid/reference/currency.json",
            include_str!("fixtures/partial-resources/currency.json"),
        ),
        (
            b02,
            "northbank.settlement.reconcile",
            r#"{"records":[],"statementBalance":0}"#,
            "fails",
            "minItems",
            "https://northbank.example.invalid/reference/currency.json",
            include_str!("fixtures/partial-resources/currency.json"),
        ),
        (
            c02,
            "findColorNames",
            r#"{"name":"red"}"#,
            "satisfies",
            "",
            "https://example.invalid/dev/c02/list-name",
            include_str!("fixtures/partial-resources/list.json"),
        ),
        (
            c02,
            "findColorNames",
            r#"{"name":"re"}"#,
            "fails",
            "minLength",
            "https://example.invalid/dev/c02/list-name",
            include_str!("fixtures/partial-resources/list.json"),
        ),
        (
            c02,
            "findColorNames",
            r#"{"name":null}"#,
            "fails",
            "type",
            "https://example.invalid/dev/c02/list-name",
            include_str!("fixtures/partial-resources/list.json"),
        ),
    ] {
        let doc = ParsedDocument::parse(source).unwrap();
        let evaluator = Arc::new(DefaultEvaluator::new().with_schema_details(true));
        let empty = doc
            .value_contracts(evaluator.clone(), ResourceSet::default())
            .unwrap();
        let ContractPreparation::Ready(partial) = empty.prepare(operation, Side::Input) else {
            panic!("{operation}")
        };
        let result = partial.validate(&json(input));
        assert_eq!(label(result.clone()), expected);
        let full = doc
            .value_contracts(evaluator, resources(&[(supplied_uri, supplied_text)]))
            .unwrap();
        let ContractPreparation::Ready(full) = full.prepare(operation, Side::Input) else {
            panic!()
        };
        let complete_result = full.validate(&json(input));
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::to_value(&complete_result).unwrap()
        );
        if let ValueOutcome::Fails {
            problems,
            problems_complete,
        } = result
        {
            assert!(problems_complete);
            assert!(problems.iter().any(|p| p.code == keyword));
            assert!(problems.iter().all(|p| {
                p.schema_location
                    .as_ref()
                    .is_some_and(|l| !l.pointer.contains("$defs/n"))
            }));
        }
    }
}
#[test]
fn negative_alias_into_advancing_recursive_graph_is_hole_dependent() {
    let schema = format!(
        r##"{{"$id":"https://review.invalid/root","$defs":{{"recursive":{{"type":"object","properties":{{"next":{{"$ref":"#/$defs/recursive"}},"external":{{"$ref":"{U}"}}}}}}}},"properties":{{"branch":{{"oneOf":[{{"$ref":"#/$defs/recursive"}},false]}}}}}}"##
    );
    assert_eq!(
        refusal(&schema, ResourceSet::default()).code,
        "partial-nonpositive-influence"
    );
    // A closed negative keyword on the same object is a fixed conjunct.
    let sibling =
        format!(r#"{{"properties":{{"external":{{"$ref":"{U}"}}}},"not":{{"const":1}}}}"#);
    assert_eq!(label(ready(&sibling).validate(&json("{}"))), "satisfies");
    assert_eq!(label(ready(&sibling).validate(&json("1"))), "fails");
}
#[test]
fn known_reference_errors_and_contained_id_anchor_identity_survive_holes() {
    let schema = format!(r#"{{"allOf":[{{"$ref":"{U}"}},{{"$ref":"relative"}}]}}"#);
    assert_eq!(
        refusal(&schema, ResourceSet::default()).code,
        "anonymous-relative-reference"
    );
    let schema =
        format!(r#"{{"allOf":[{{"$ref":"{U}"}},{{"$ref":"https://review.invalid/K#dup"}}]}}"#);
    let known = r#"{"$defs":{"a":{"$anchor":"dup"},"b":{"$anchor":"dup"}}}"#;
    assert_eq!(
        refusal(&schema, resources(&[("https://review.invalid/K", known)])).code,
        "ambiguous-anchor"
    );
    let schema = format!(
        r#"{{"properties":{{"external":{{"$ref":"{U}"}},"known":{{"$ref":"https://review.invalid/contained#value"}}}}}}"#
    );
    let known = r#"{"$defs":{"local":{"$id":"https://review.invalid/contained","$anchor":"value","const":9007199254740993}}}"#;
    let ctx = context(
        &schema,
        resources(&[("https://review.invalid/K", known)]),
        DefaultEvaluator::new(),
    );
    let ContractPreparation::Ready(contract) = ctx.prepare("op", Side::Input) else {
        panic!()
    };
    assert_eq!(
        label(contract.validate(&json(r#"{"known":9007199254740993}"#))),
        "satisfies"
    );
    assert_eq!(
        label(contract.validate(&json(r#"{"known":9007199254740992}"#))),
        "fails"
    );
}
#[test]
fn partial_failure_survives_diagnostic_exhaustion_and_unsupported_known_evaluation_refuses() {
    let costly = vec!["false"; 200].join(",");
    let schema = format!(
        r#"{{"properties":{{"external":{{"$ref":"{U}"}}}},"allOf":[false,{{"allOf":[{costly}]}}]}}"#
    );
    let limited = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: 30,
        max_problems: 3,
        ..Limits::default()
    });
    let ContractPreparation::Ready(contract) =
        context(&schema, ResourceSet::default(), limited).prepare("op", Side::Input)
    else {
        panic!()
    };
    assert!(matches!(
        contract.validate(&json("{}")),
        ValueOutcome::Fails {
            problems_complete: false,
            ..
        }
    ));
    let property = format!(
        r#"{{"properties":{{"external":{{"$ref":"{U}"}},"name":{{"pattern":"\\p{{L}}"}}}}}}"#
    );
    let contract = ready(&property);
    assert!(
        matches!(contract.validate(&json(r#"{"name":"a"}"#)),ValueOutcome::NoVerdict {detail} if detail.reason == NoVerdictReason::UnsupportedCapability)
    );
    assert_eq!(label(contract.validate(&json("{}"))), "satisfies");
}

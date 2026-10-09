use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
// Existing evaluator cases compare semantic outcomes from either setup refusal or
// value evaluation. New setup-state tests below assert the stage independently.
trait EvaluateCase {
    fn validate(&self, value: &JsonValue) -> ValueOutcome;
    fn validate_with_control(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome;
}
impl EvaluateCase for ContractPreparation {
    fn validate(&self, value: &JsonValue) -> ValueOutcome {
        self.validate_with_control(value, &WorkControl::new())
    }
    fn validate_with_control(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        match self {
            ContractPreparation::Ready(contract) => contract.validate_with_control(value, control),
            ContractPreparation::NoVerdict { detail } => ValueOutcome::NoVerdict {
                detail: detail.clone(),
            },
            other => panic!("unexpected setup state: {other:?}"),
        }
    }
}
fn doc(schema: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
    ))
    .unwrap()
}
fn contracts(schema: &str) -> ValueContracts {
    doc(schema)
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap()
}
fn value(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
#[test]
fn numeric_arithmetic_admission_preserves_carriage_types_counts_and_bounds() {
    let huge = format!("1e{}", "9".repeat(100_000));
    let parsed = value(&huge);
    assert_eq!(parsed.text(), huge);
    for schema in [
        "{\"type\":\"integer\"}".to_owned(),
        format!("{{\"minimum\":{huge}}}"),
        format!("{{\"const\":{huge}}}"),
    ] {
        assert!(matches!(
            contracts(&schema)
                .prepare("op", Side::Input)
                .validate(&parsed),
            ValueOutcome::Satisfies
        ));
    }
    assert!(matches!(
        contracts(&format!("{{\"maxItems\":{huge}}}"))
            .prepare("op", Side::Input)
            .validate(&value("[]")),
        ValueOutcome::Satisfies
    ));
    let multiple = contracts("{\"multipleOf\":3}").prepare("op", Side::Input);
    for token in [&huge, &"7".repeat(4097)] {
        assert!(
            matches!(multiple.validate(&value(token)),ValueOutcome::NoVerdict {detail:NoVerdict {reason:NoVerdictReason::LimitExceeded,ref code,..}} if code=="numeric-arithmetic-limit")
        );
        assert!(matches!(
            multiple.validate(&value("3")),
            ValueOutcome::Satisfies
        ));
    }
    for token in ["7".repeat(4096), "1e10000".into(), "1e-10000".into()] {
        assert!(matches!(
            multiple.validate(&value(&token)),
            ValueOutcome::Mismatch { .. }
        ));
    }
}
#[test]
fn deep_literal_and_opaque_data_do_not_inherit_compiler_tree_depth() {
    // 9,995 literal containers fit the 10,000-level document carriage floor.
    // They are data, not 9,995 nested schema applicators.
    for depth in [510, 513, 1000, 9995] {
        let literal = format!("{}7{}", "[".repeat(depth), "]".repeat(depth));
        let different = format!("{}8{}", "[".repeat(depth), "]".repeat(depth));
        for schema in [
            format!("{{\"const\":{literal}}}"),
            format!("{{\"enum\":[{literal}]}}"),
        ] {
            let prepared = contracts(&schema).prepare("op", Side::Input);
            assert!(
                matches!(prepared.validate(&value(&literal)), ValueOutcome::Satisfies),
                "depth {depth}"
            );
            let result = prepared.validate(&value(&different));
            let ValueOutcome::Mismatch {
                problems,
                problems_complete,
            } = result
            else {
                panic!("{depth}: {result:?}")
            };
            assert!(problems_complete);
            assert_eq!(problems[0].instance_pointer, "");
            assert!(matches!(problems[0].code.as_str(), "const" | "enum"));
        }
        let prepared = contracts(&format!(
            "{{\"type\":\"integer\",\"default\":{literal},\"x-opaque\":{literal}}}"
        ))
        .prepare("op", Side::Input);
        assert!(matches!(
            prepared.validate(&value("7")),
            ValueOutcome::Satisfies
        ));
    }
}
#[test]
fn preparation_cache_is_bounded_lru_and_eviction_preserves_owned_handles() {
    struct Probe {
        calls: Arc<AtomicUsize>,
        live: Arc<AtomicUsize>,
    }
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl PreparedSchema for Held {
        fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
            ValueOutcome::Satisfies
        }
    }
    impl SchemaEvaluator for Probe {
        fn prepare(
            &self,
            _: &SchemaRequest,
            _: &WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.live.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Held(self.live.clone())))
        }
    }
    let document=ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"a":{"input":true},"b":{"input":true},"c":{"input":true}}}"#).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let live = Arc::new(AtomicUsize::new(0));
    let evaluator = Arc::new(Probe {
        calls: calls.clone(),
        live: live.clone(),
    });
    let context = document
        .value_contracts_with_options(
            evaluator.clone(),
            ResourceSet::default(),
            ValueContractOptions { cache_capacity: 2 },
        )
        .unwrap();
    let retained = context.prepare("a", Side::Input);
    drop(context.prepare("b", Side::Input));
    drop(context.prepare("a", Side::Input)); // a is now most recent.
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    drop(context.prepare("c", Side::Input)); // b is evicted and freed.
    assert_eq!(live.load(Ordering::SeqCst), 2);
    drop(context.prepare("b", Side::Input)); // b is recompiled; retained a survives eviction.
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(live.load(Ordering::SeqCst), 3);
    drop(context);
    assert_eq!(live.load(Ordering::SeqCst), 1);
    assert!(matches!(
        retained.validate(&value("7")),
        ValueOutcome::Satisfies
    ));
    drop(retained);
    assert_eq!(live.load(Ordering::SeqCst), 0);
    let uncached = document
        .value_contracts_with_options(
            evaluator,
            ResourceSet::default(),
            ValueContractOptions { cache_capacity: 0 },
        )
        .unwrap();
    drop(uncached.prepare("a", Side::Input));
    drop(uncached.prepare("a", Side::Input));
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
#[test]
fn retained_contracts_outlive_documents_and_distinguish_absent_sides() {
    let context = contracts(r#"{"maximum":9007199254740992}"#);
    let prepared = context.prepare("op", Side::Input);
    drop(context);
    assert!(matches!(
        prepared.validate(&value("9007199254740992")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepared.validate(&value("9007199254740993")),
        ValueOutcome::Mismatch { .. }
    ));
    let context = contracts("true");
    assert!(matches!(
        context.prepare("op", Side::Output),
        ContractPreparation::NoContract
    ));
    assert!(matches!(
        context.prepare("missing", Side::Input),
        ContractPreparation::OperationMissing
    ));
}
#[test]
fn review_s09_anonymous_scope_selects_outer_dynamic_anchor() {
    let document=ParsedDocument::parse(r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://review.invalid/list"}}},"schemas":{"List":{"$id":"https://review.invalid/list","type":"array","items":{"$dynamicRef":"#element"},"$defs":{"element":{"$dynamicAnchor":"element","type":"string"}}},"Override":{"$dynamicAnchor":"element","type":"integer"}}}"##).unwrap();
    let context = document
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    let prepared = context.prepare("op", Side::Input);
    assert!(matches!(
        prepared.validate(&value("[7]")),
        ValueOutcome::Satisfies
    ));
    let result = prepared.validate(&value(r#"["x"]"#));
    let ValueOutcome::Mismatch { problems, .. } = result else {
        panic!("{result:?}");
    };
    assert_eq!(problems[0].instance_pointer, "/0");
    assert_eq!(
        problems[0].schema_location.as_ref().unwrap().pointer,
        "/schemas/Override/type"
    );
}
#[test]
fn review_s11_same_uri_resources_are_isolated_and_precancellation_does_not_poison() {
    let document = doc(r#"{"$ref":"https://review.invalid/shared"}"#);
    let evaluator = Arc::new(DefaultEvaluator::new());
    let make = |schema: &str| {
        document
            .value_contracts(
                evaluator.clone(),
                ResourceSet::new([SchemaResource {
                    uri: "https://review.invalid/shared".into(),
                    document: value(schema),
                }])
                .unwrap(),
            )
            .unwrap()
    };
    let integer = make(r#"{"type":"integer"}"#);
    let string = make(r#"{"type":"string"}"#);
    for context in [&integer, &string] {
        let cancelled = WorkControl::new();
        cancelled.cancel();
        assert!(matches!(
            context.prepare_with_control("op", Side::Input, &cancelled),
            ContractPreparation::NoVerdict {
                detail: NoVerdict {
                    reason: NoVerdictReason::Cancelled,
                    ..
                }
            }
        ));
    }
    assert!(matches!(
        integer.prepare("op", Side::Input).validate(&value("7")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        string.prepare("op", Side::Input).validate(&value("7")),
        ValueOutcome::Mismatch { .. }
    ));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let context = integer.clone();
            scope.spawn(move || {
                for _ in 0..100 {
                    assert!(matches!(
                        context.prepare("op", Side::Input).validate(&value("7")),
                        ValueOutcome::Satisfies
                    ));
                }
            });
        }
    });
}
struct CancelOnce {
    calls: AtomicUsize,
}
impl SchemaEvaluator for CancelOnce {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            control.cancel();
            control.check()?;
        }
        DefaultEvaluator::new().prepare(request, control)
    }
}
#[test]
fn cancellation_during_preparation_is_not_cached() {
    let evaluator = Arc::new(CancelOnce {
        calls: AtomicUsize::new(0),
    });
    let context = doc("true")
        .value_contracts(evaluator.clone(), ResourceSet::default())
        .unwrap();
    let cancelled = context.prepare_with_control("op", Side::Input, &WorkControl::new());
    assert!(matches!(
        cancelled,
        ContractPreparation::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::Cancelled,
                ..
            }
        }
    ));
    assert!(matches!(
        context.prepare("op", Side::Input).validate(&value("1")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        context.prepare("op", Side::Input).validate(&value("2")),
        ValueOutcome::Satisfies
    ));
    assert_eq!(evaluator.calls.load(Ordering::SeqCst), 2);
}
#[test]
fn review_f01_f06_conservative_preparation_is_not_semantic_undefinedness() {
    for schema in [
        r#"{"anyOf":[true,{"pattern":"["}]}"#,
        r#"{"anyOf":[true,{"$ref":"https://absent.invalid/x"}]}"#,
    ] {
        let result = contracts(schema)
            .prepare("op", Side::Input)
            .validate(&value(r#""secret""#));
        assert!(matches!(
            result,
            ValueOutcome::NoVerdict {
                detail: NoVerdict {
                    reason: NoVerdictReason::ConservativePreparation
                        | NoVerdictReason::ResourceUnavailable,
                    ..
                }
            }
        ));
    }
    // Passing anyOf annotations still matter despite a true branch.
    let prepared = contracts(
        r#"{"anyOf":[{"properties":{"x":{"type":"string"}}},true],"unevaluatedProperties":false}"#,
    )
    .prepare("op", Side::Input);
    assert!(matches!(
        prepared.validate(&value(r#"{"x":"ok"}"#)),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepared.validate(&value(r#"{"x":7}"#)),
        ValueOutcome::Mismatch { .. }
    ));
}
#[test]
fn review_f04_diagnostics_have_a_separate_bounded_pass_and_healthy_reuse() {
    let expensive = format!(
        r#"{{"allOf":[false,{{"allOf":[{}]}}]}}"#,
        vec!["false"; 200].join(",")
    );
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: 30,
        max_problems: 3,
        ..Limits::default()
    });
    let context = doc(&expensive)
        .value_contracts(Arc::new(evaluator), ResourceSet::default())
        .unwrap();
    let prepared = context.prepare("op", Side::Input);
    for _ in 0..5 {
        let result = prepared.validate(&value("0"));
        let ValueOutcome::Mismatch {
            problems,
            problems_complete,
        } = result
        else {
            panic!("{result:?}");
        };
        assert!(!problems_complete);
        assert!(!problems.is_empty());
        assert!(problems.len() <= 3);
    }
    assert!(matches!(
        contracts("true")
            .prepare("op", Side::Input)
            .validate(&value("0")),
        ValueOutcome::Satisfies
    ));
    let limited = doc(r#"{"allOf":[true,true,true]}"#)
        .value_contracts(
            Arc::new(DefaultEvaluator::with_limits(Limits {
                evaluation_steps: 1,
                ..Limits::default()
            })),
            ResourceSet::default(),
        )
        .unwrap();
    assert!(matches!(
        limited.prepare("op", Side::Input).validate(&value("0")),
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::LimitExceeded,
                ..
            }
        }
    ));
}
#[test]
fn diagnostics_are_original_and_do_not_expose_instance_values() {
    let prepared =
        contracts(r#"{"properties":{"a/b":{"type":"string"}},"unevaluatedProperties":false}"#)
            .prepare("op", Side::Input);
    let result = prepared.validate(&value(r#"{"a/b":789,"secret":"SYNTHETIC_SECRET_VALUE"}"#));
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains("789"));
    assert!(!encoded.contains("SYNTHETIC_SECRET_VALUE"));
    assert!(!encoded.contains("sdk-program"));
    let ValueOutcome::Mismatch { problems, .. } = result else {
        panic!("{result:?}");
    };
    assert!(problems.iter().any(|p| p.instance_pointer == "/a~1b"));
    assert!(problems.iter().any(|p| p.instance_pointer == "/secret"));
}
#[test]
fn property_escape_refusal_is_lazy_and_does_not_poison_thread_state() {
    let prepared = contracts(r#"{"pattern":"\\p{Lu}"}"#).prepare("op", Side::Input);
    assert!(matches!(
        prepared.validate(&value("3")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        prepared.validate(&value(r#""A""#)),
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::UnsupportedCapability,
                ..
            }
        }
    ));
    assert!(matches!(
        prepared.validate(&value("3")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        contracts(r#"{"pattern":"^A$"}"#)
            .prepare("op", Side::Input)
            .validate(&value(r#""A""#)),
        ValueOutcome::Satisfies
    ));
}

#[test]
fn diagnostic_allocation_admission_bounds_many_missing_names() {
    let names = (0..10000)
        .map(|i| format!("\"field{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let schema = format!("{{\"required\":[{names}]}}");
    let context = doc(&schema)
        .value_contracts(
            Arc::new(DefaultEvaluator::with_limits(Limits {
                max_problems: 2,
                ..Limits::default()
            })),
            ResourceSet::default(),
        )
        .unwrap();
    let result = context.prepare("op", Side::Input).validate(&value("{}"));
    let ValueOutcome::Mismatch {
        problems,
        problems_complete,
    } = result
    else {
        panic!("{result:?}");
    };
    assert!(!problems_complete);
    assert_eq!(problems.len(), 2);
}

#[test]
fn internal_work_scopes_restore_after_nested_exhaustion_unwind_and_threads() {
    use jsonschema::{ob_ecma, ob_work};
    let validator = jsonschema::options()
        .build(&serde_json::json!({"required":["a","b"]}))
        .unwrap();
    let instance = serde_json::json!({});
    assert!(matches!(
        ob_work::diagnostics(0, || validator.iter_errors(&instance).count()),
        Err(ob_work::Stop::Diagnostics)
    ));
    assert_eq!(validator.iter_errors(&instance).count(), 2);
    let _ = std::panic::catch_unwind(|| ob_work::diagnostics(0, || panic!("scope probe")));
    assert_eq!(validator.iter_errors(&instance).count(), 2);
    assert!(
        ob_work::bounded(100, 100, || ob_work::bounded(0, 100, || validator
            .is_valid(&instance)))
        .is_err()
    );
    assert!(ob_work::bounded(100, 100, || validator.is_valid(&instance)).is_ok());
    let pattern = ob_ecma::Regex::new(r"\p{Lu}").unwrap();
    assert!(matches!(
        ob_ecma::top_level(100, || ob_ecma::top_level(100, || pattern.is_match("A"))),
        Err(ob_ecma::Exhausted::UnsupportedProperty)
    ));
    let _ = std::panic::catch_unwind(|| ob_ecma::top_level(0, || panic!("regex scope probe")));
    assert!(ob_ecma::top_level(100, || true).is_ok());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..100 {
                    assert!(ob_work::bounded(1, 1, || true).is_ok());
                    assert!(ob_ecma::top_level(1, || true).is_ok());
                }
            });
        }
    });
}

#[test]
fn upstream_test_dispositions_use_the_supported_exact_ecma_u_path() {
    // Upstream's alternate Rust-regex backend treats U+0085 as whitespace.
    // The SDK exclusively uses ECMAScript Unicode-mode matching.
    for (pattern, expected) in [(r"^\s$", false), (r"^\S$", true), (r"^[^\s]$", true)] {
        let schema = serde_json::json!({"pattern": pattern}).to_string();
        let result = contracts(&schema)
            .prepare("op", Side::Input)
            .validate(&value(r#""\u0085""#));
        assert_eq!(
            matches!(result, ValueOutcome::Satisfies),
            expected,
            "{pattern}: {result:?}"
        );
        assert!(matches!(
            result,
            ValueOutcome::Satisfies | ValueOutcome::Mismatch { .. }
        ));
    }
    // These legacy non-Unicode class forms are invalid in the mandated u mode.
    for pattern in [r"^[\w--z]$", r"^[\d--z]$", r"^[[a]]$"] {
        let schema = serde_json::json!({"pattern": pattern}).to_string();
        assert!(matches!(
            contracts(&schema)
                .prepare("op", Side::Input)
                .validate(&value(r#""-""#)),
            ValueOutcome::NoVerdict { .. }
        ));
    }
    // The original test expected an engine failure for a valid empty match.
    let prepared = contracts(r#"{"pattern":"^.{0,404600}$"}"#).prepare("op", Side::Input);
    assert!(matches!(
        prepared.validate(&value(r#""""#)),
        ValueOutcome::Satisfies
    ));
    // Huge finite decimal exponents do not become infinity or cease to be integers.
    let prepared = contracts(r#"{"type":"integer"}"#).prepare("op", Side::Input);
    for number in ["1e1000001", "-1e1000001"] {
        assert!(matches!(
            prepared.validate(&value(number)),
            ValueOutcome::Satisfies
        ));
    }
    // Raw dependency APIs lack the SDK's enclosing refusal guard. Through the
    // public adapter, selected property matching can never become false validity.
    let schema = serde_json::json!({"patternProperties": {r"^\p{L}{300}$": {"type":"integer"}}, "unevaluatedProperties": false}).to_string();
    let result = contracts(&schema)
        .prepare("op", Side::Input)
        .validate(&value(&serde_json::json!({"a".repeat(300): 1}).to_string()));
    assert!(matches!(
        result,
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::UnsupportedCapability,
                ..
            }
        }
    ));
}

#[test]
fn preparation_separates_setup_from_value_and_keeps_ready_healthy() {
    let d=ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"absent":{},"yes":{"input":true,"aliases":["shared"]},"no":{"input":false,"aliases":["shared"]},"null":{"input":null}}}"#).unwrap();
    let c = d
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap();
    assert!(matches!(
        c.prepare("absent", Side::Input),
        ContractPreparation::NoContract
    ));
    assert!(matches!(
        c.prepare("missing", Side::Input),
        ContractPreparation::OperationMissing
    ));
    assert!(
        matches!(c.prepare("shared",Side::Input),ContractPreparation::OperationAmbiguous{candidates} if candidates==["no","yes"])
    );
    assert!(matches!(
        c.prepare("null", Side::Input),
        ContractPreparation::NoVerdict { .. }
    ));
    let ContractPreparation::Ready(ready) = c.prepare("yes", Side::Input) else {
        panic!()
    };
    let ContractPreparation::Ready(no) = c.prepare("no", Side::Input) else {
        panic!()
    };
    let cancel = WorkControl::new();
    cancel.cancel();
    assert!(matches!(
        ready.validate_with_control(&value("null"), &cancel),
        ValueOutcome::NoVerdict {
            detail: NoVerdict {
                reason: NoVerdictReason::Cancelled,
                ..
            }
        }
    ));
    drop(c);
    drop(d);
    assert!(matches!(
        ready.validate(&value("null")),
        ValueOutcome::Satisfies
    ));
    assert!(matches!(
        no.validate(&value("null")),
        ValueOutcome::Mismatch { .. }
    ));
    for (text, pointer) in [
        (
            r#"{"openbindings":"0.2.0","operations":{"op":null}}"#,
            "/operations/op",
        ),
        (
            r#"{"openbindings":"0.2.0","operations":null}"#,
            "/operations",
        ),
        (r#"{"openbindings":"0.2.0"}"#, ""),
        (
            r#"{"openbindings":"0.2.0","operations":{"op":{"aliases":false}}}"#,
            "/operations/op/aliases",
        ),
    ] {
        let d = ParsedDocument::parse(text).unwrap();
        let c = d
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
            .unwrap();
        let ContractPreparation::NoVerdict { detail } = c.prepare("op", Side::Input) else {
            panic!()
        };
        assert_eq!(detail.reason, NoVerdictReason::Undefined);
        assert_eq!(detail.code, "invalid-operation-structure");
        assert_eq!(detail.location.unwrap().pointer, pointer);
    }
}
#[test]
fn resources_reject_compared_uri_duplicates_even_for_equal_bytes() {
    let schema = value("true");
    assert!(
        ResourceSet::new([
            SchemaResource {
                uri: "https://example.invalid/a/../b".into(),
                document: schema.clone()
            },
            SchemaResource {
                uri: "https://example.invalid/b#".into(),
                document: schema
            }
        ])
        .is_err()
    );
}

//! Decision experiment controls. No route counters are compiled into timing peers.
use super::*;
use openbindings_internal_schema::preparation::{Attempt, EnvironmentRequest};
use openbindings_schema_evaluator_test_support::{run, suite};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static HEALTHY: AtomicBool = AtomicBool::new(false);

/// Independent preparation of the pinned current original-schema Plan. It does
/// not construct a compiled owner and then disable it.
struct Original(Limits);
impl SchemaEvaluator for Original {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Attempt::run(
            limits.max_sdk_work_units,
            limits.max_sdk_reservation_bytes,
            &|| control.is_cancelled(),
            |attempt| {
                control.check()?;
                let environment = request.environment_with_control(
                    limits.max_schema_nodes,
                    limits.max_identity_bytes,
                    attempt,
                )?;
                let plan = semantic::Plan::new(environment, self.0.clone(), *limits, attempt)?;
                control.check()?;
                Ok(DefaultValidator {
                    plan: Arc::new(plan),
                    compiled: compiled::Lane::Original("independent-original"),
                })
            },
        )
        .map_err(PreparationFailure::from)?
    }
}
fn write(name: &str, receipt: &serde_json::Value) {
    if let Ok(directory) = std::env::var("OB_COMPILED_RECEIPTS") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join(name),
            serde_json::to_vec_pretty(receipt).unwrap(),
        )
        .unwrap();
    }
}
fn prepare_document<E: SchemaEvaluator<Validator = DefaultValidator>>(
    document: &str,
    resources: &ExternalSchemas,
    evaluator: &E,
    limits: &PreparationLimits,
) -> Result<DefaultValidator, PreparationFailure> {
    Document::parse(document)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant qualification input")
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(evaluator, resources, limits)
        .map(PreparedSchema::into_validator)
}
fn document(schema: &str) -> String {
    format!(r##"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"##)
}
fn owners(
    schema: &str,
    resources: &ExternalSchemas,
    limits: Limits,
    preparation: PreparationLimits,
) -> (DefaultValidator, DefaultValidator) {
    let resources = resources
        .with(
            "https://qualification.invalid/root",
            JsonValue::parse(schema).unwrap(),
        )
        .unwrap();
    let document = document(r#"{"$ref":"https://qualification.invalid/root"}"#);
    let selected = prepare_document(
        &document,
        &resources,
        &DefaultEvaluator::with_limits(limits.clone()),
        &preparation,
    )
    .unwrap_or_else(|failure| panic!("{}", serde_json::to_string(&failure).unwrap()));
    let original = prepare_document(&document, &resources, &Original(limits), &preparation)
        .unwrap_or_else(|failure| panic!("{}", serde_json::to_string(&failure).unwrap()));
    (selected, original)
}
fn serialized(outcome: ValueOutcome) -> serde_json::Value {
    serde_json::to_value(outcome).unwrap()
}
fn compare(
    selected: &DefaultValidator,
    original: &DefaultValidator,
    value: &JsonValue,
) -> (&'static str, [usize; 3]) {
    compiled::entries::take();
    for detail in [Detail::Outcome, Detail::Explain] {
        let actual = selected.evaluate(value, detail, &HEALTHY);
        let expected = original.evaluate(value, detail, &HEALTHY);
        assert_eq!(
            serialized(actual),
            serialized(expected),
            "{} / {detail:?}",
            &value.text()[..value.text().len().min(100)]
        );
    }
    let counts = compiled::entries::take();
    assert_eq!(
        &counts[..2],
        &[0, 0],
        "value evaluation cannot reconstruct a backend"
    );
    (
        if counts[2] == 1 {
            "compiled"
        } else {
            assert_eq!(counts[2], 0);
            "original"
        },
        counts,
    )
}
#[test]
fn compiled_decision_complete_kit_agreement_and_routes() {
    let original = run(&Original(Limits::default()));
    let selected = run(&DefaultEvaluator::new());
    assert!(original.is_success());
    assert!(selected.is_success());
    assert_eq!(selected.observations.len(), 1546);
    assert_eq!(
        serde_json::to_value(&selected).unwrap(),
        serde_json::to_value(&original).unwrap(),
        "complete kit observations, including original locations"
    );
    let mut routes = Vec::new();
    for group in &suite().groups {
        let resources = ExternalSchemas::new(group.resources.iter().map(|id| {
            let r = &suite().resources[id];
            ExternalSchema {
                uri: r.uri.clone(),
                document: JsonValue::parse(&r.document).unwrap(),
            }
        }))
        .unwrap();
        compiled::entries::take();
        let selected = prepare_document(
            &group.document,
            &resources,
            &DefaultEvaluator::new(),
            &PreparationLimits::default(),
        )
        .unwrap();
        let construction = compiled::entries::take();
        let original = prepare_document(
            &group.document,
            &resources,
            &Original(Limits::default()),
            &PreparationLimits::default(),
        )
        .unwrap();
        let reason = match &selected.compiled {
            compiled::Lane::Original(reason) => *reason,
            compiled::Lane::Compiled(_) => "eligible",
        };
        if reason != "eligible" {
            assert_eq!(construction, [0, 0, 0], "rejected group {}", group.id);
        }
        for case in &group.cases {
            let (lane, counts) = compare(
                &selected,
                &original,
                &JsonValue::parse(&case.value).unwrap(),
            );
            routes.push(json!({"id":case.id,"preparation":reason,"lane":lane,"construction":construction,"value_entries":counts[2]}));
        }
    }
    let compiled = routes.iter().filter(|r| r["lane"] == "compiled").count();
    assert!(
        compiled > 1000,
        "positive compiled coverage required: {compiled}"
    );
    write(
        "kit-agreement.json",
        &json!({"required":1546,"complete_observations_equal":true,"configuration_failures":[],"compiled":compiled,"original_count":1546-compiled,"routes":routes,"selected":selected,"original":original}),
    );
}
#[test]
fn compiled_decision_exact_numbers_text_and_references() {
    let cases = [
        (r##"{"type":"integer"}"##, "1.0", true),
        (r##"{"type":"integer"}"##, "1e400", true),
        (r##"{"type":"integer"}"##, "1e-200000", true),
        (
            r##"{"type":"integer"}"##,
            "-0e999999999999999999999999",
            true,
        ),
        (
            r##"{"maximum":9007199254740992}"##,
            "9007199254740993",
            true,
        ),
        (
            r##"{"minimum":9223372036854775807}"##,
            "9223372036854775808",
            true,
        ),
        (
            r##"{"minimum":-9223372036854775808}"##,
            "-9223372036854775809",
            true,
        ),
        (r##"{"minimum":1e400}"##, "9e399", true),
        (r##"{"exclusiveMaximum":1e400}"##, "10e399", true),
        (r##"{"maximum":-0}"##, "0e99999999999999999999999", true),
        (r##"{"multipleOf":0.1}"##, "0.3", true),
        (r##"{"multipleOf":0.1}"##, "0.30000000000000004", true),
        (r##"{"multipleOf":1e-400}"##, "2e-400", true),
        (r##"{"multipleOf":3}"##, "1e19990", true),
        (r##"{"multipleOf":3}"##, "1e20001", false),
        (r##"{"multipleOf":3}"##, "0e9999999999999999999999", false),
        (
            r##"{"multipleOf":3}"##,
            "123456789012345678901234567890",
            false,
        ),
        (
            r##"{"const":{"n":[1e400,-0,9007199254740993]}}"##,
            r##"{"n":[10e399,0.0,9.007199254740993e15]}"##,
            true,
        ),
        (r##"{"const":9007199254740993}"##, "9007199254740992", true),
        (r##"{"enum":[{"n":1.0},1e400]}"##, "10e399", true),
        (r##"{"uniqueItems":true}"##, "[0.1,0.10]", true),
        (
            r##"{"uniqueItems":true}"##,
            "[9007199254740992,9007199254740993]",
            true,
        ),
        (
            r##"{"uniqueItems":true}"##,
            r##"[{"n":1e400},{"n":10e399}]"##,
            false,
        ),
        (r##"{"type":"string"}"##, r##""\ud800""##, false),
        (r##"{"type":"object"}"##, r##"{"\ud800":1}"##, false),
        (r##"{"type":"object"}"##, r##"{"x":1,"x":2}"##, false),
        (r##"{"pattern":"^[A-Za-z]+$"}"##, r##""Matthew""##, true),
        (r##"{"pattern":"(?<é>a)"}"##, r##""a""##, true),
        (
            r##"{"$defs":{"n":{"type":"integer"}},"allOf":[{"$ref":"#/$defs/n"},{"$ref":"#/$defs/n"}]}"##,
            "42",
            true,
        ),
        (
            r##"{"$defs":{"n":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/n"}}]}},"$ref":"#/$defs/n"}"##,
            "[[42]]",
            false,
        ),
        (r##"{"$ref":"https://absent.invalid/schema"}"##, "42", false),
        (r##"{"$ref":"#/$defs/missing"}"##, "42", false),
        (
            r##"{"anyOf":[true,{"$ref":"https://absent.invalid/schema"}]}"##,
            "42",
            false,
        ),
        (
            r##"{"$dynamicAnchor":"node","properties":{"child":{"$dynamicRef":"#node"}}}"##,
            r##"{"child":{}}"##,
            false,
        ),
        (
            r##"{"allOf":[{"properties":{"x":true}}],"unevaluatedProperties":false}"##,
            r##"{"x":1}"##,
            false,
        ),
    ];
    let mut results = Vec::new();
    for (schema, text, compiled) in cases {
        compiled::entries::take();
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        let construction = compiled::entries::take();
        let (lane, counts) = compare(&selected, &original, &JsonValue::parse(text).unwrap());
        assert_eq!(lane == "compiled", compiled, "{schema} / {text}");
        if let compiled::Lane::Original(_) = selected.compiled {
            assert_eq!(construction, [0, 0, 0]);
        }
        results.push(json!({"schema":schema,"value":text,"lane":lane,"construction":construction,"value_entries":counts[2]}));
    }
    // Unpaired content outside the selected subtree must not disqualify it.
    let (selected, original) = owners(
        r##"{"type":"integer"}"##,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let wrapper = JsonValue::parse(r##"{"outside":"\ud800","selected":42}"##).unwrap();
    assert_eq!(
        compare(
            &selected,
            &original,
            &wrapper.get("selected").unwrap().to_owned()
        )
        .0,
        "compiled"
    );
    write(
        "exact-reference-differentials.json",
        &json!({"cases":results,"unsafe_outside_subtree":"compiled","all_equal":true}),
    );
}
#[test]
fn compiled_decision_explicit_limits_and_controls() {
    let mut limits = Vec::new();
    for index in 0..7 {
        let mut l = Limits::default();
        match index {
            0 => l.evaluation_sdk_work_units = Some(0),
            1 => l.evaluation_sdk_reservation_bytes = 0,
            2 => l.pattern_bytes = 1,
            3 => l.pattern_depth = 1,
            4 => l.total_pattern_bytes = 1,
            5 => l.total_pattern_memory = 1,
            6 => l.schema_resources = Some(0),
            _ => unreachable!(),
        }
        limits.push(l);
    }
    let schema = r##"{"type":"array","items":{"type":"integer"}}"##;
    let value = JsonValue::parse("[1,2,3]").unwrap();
    let mut observations = Vec::new();
    for l in limits {
        compiled::entries::take();
        let doc = document(schema);
        let selected = prepare_document(
            &doc,
            &ExternalSchemas::default(),
            &DefaultEvaluator::with_limits(l.clone()),
            &PreparationLimits::default(),
        );
        let original = prepare_document(
            &doc,
            &ExternalSchemas::default(),
            &Original(l.clone()),
            &PreparationLimits::default(),
        );
        let counts = compiled::entries::take();
        assert_eq!(counts, [0, 0, 0]);
        match (selected, original) {
            (Ok(selected), Ok(original)) => {
                assert_eq!(compare(&selected, &original, &value).0, "original");
                observations.push(json!({"limits":l,"preparation":"ready","outcome":selected.evaluate(&value,Detail::Outcome,&HEALTHY)}));
            }
            (Err(a), Err(b)) => {
                assert_eq!(
                    serde_json::to_value(&a).unwrap(),
                    serde_json::to_value(&b).unwrap()
                );
                observations.push(json!({"limits":l,"preparation":a}));
            }
            _ => panic!("preparation disagreement"),
        }
    }
    for index in 0..4 {
        let mut preparation = PreparationLimits::default();
        match index {
            0 => preparation.max_schema_nodes -= 1,
            1 => preparation.max_identity_bytes -= 1,
            2 => preparation.max_sdk_work_units -= 1,
            3 => preparation.max_sdk_reservation_bytes -= 1,
            _ => unreachable!(),
        }
        compiled::entries::take();
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            preparation,
        );
        assert_eq!(compiled::entries::take(), [0, 0, 0]);
        assert!(matches!(
            selected.compiled,
            compiled::Lane::Original("explicit-preparation-allowance")
        ));
        assert_eq!(compare(&selected, &original, &value).0, "original");
    }
    let (selected, original) = owners(
        r##"{"type":"integer","minimum":0}"##,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    struct CancelAt {
        checks: AtomicUsize,
        boundary: usize,
    }
    impl WorkControl for CancelAt {
        fn is_cancelled(&self) -> bool {
            self.checks.fetch_add(1, Ordering::Relaxed) >= self.boundary
        }
    }
    for boundary in [0, 1] {
        compiled::entries::take();
        let outcome = selected.evaluate(
            &JsonValue::parse("42").unwrap(),
            Detail::Outcome,
            &CancelAt {
                checks: AtomicUsize::new(0),
                boundary,
            },
        );
        assert!(
            matches!(outcome,ValueOutcome::NoVerdict{detail} if detail.reason==NoVerdictReason::Cancelled)
        );
        assert_eq!(compiled::entries::take()[2], boundary);
    }
    assert_eq!(
        compare(&selected, &original, &JsonValue::parse("42").unwrap()).0,
        "compiled"
    );
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let selected = &selected;
            scope.spawn(move || {
                for _ in 0..100 {
                    assert!(matches!(
                        selected.evaluate(
                            &JsonValue::parse("42").unwrap(),
                            Detail::Outcome,
                            &HEALTHY
                        ),
                        ValueOutcome::Satisfies
                    ));
                }
            });
        }
    });
    write(
        "limits-controls.json",
        &json!({"limits":observations,"entry_and_completion_cancellation":true,"healthy_retry":true,"concurrent_calls":400}),
    );
}
#[test]
fn compiled_decision_repeated_child_scratch_and_old_slot_policy() {
    let literal = json!((0..1000).collect::<Vec<_>>());
    let schema = json!({"$defs":{"object":{"properties":{"x":{"const":literal}}}},"allOf":(0..5000).map(|_|json!({"$ref":"#/$defs/object"})).collect::<Vec<_>>()});
    let value = JsonValue::parse(json!({"x":literal}).to_string()).unwrap();
    let (selected, original) = owners(
        &schema.to_string(),
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert!(
        matches!(selected.compiled, compiled::Lane::Compiled(_)),
        "schema eligible; value must choose before is_valid"
    );
    assert_eq!(compare(&selected, &original, &value).0, "original");
    let mut forms = (0..1024)
        .map(|i| json!({"pattern":format!("(?:a|b){i}")}))
        .collect::<Vec<_>>();
    forms.push(json!({"pattern":"^a$"}));
    compiled::entries::take();
    let (selected, original) = owners(
        &json!({"allOf":forms}).to_string(),
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert!(matches!(
        selected.compiled,
        compiled::Lane::Original("restored-private-pattern-profile")
    ));
    assert_eq!(compiled::entries::take(), [0, 0, 0]);
    assert_eq!(
        compare(&selected, &original, &JsonValue::parse(r##""a""##).unwrap()).0,
        "original"
    );
    write(
        "review-regressions.json",
        &json!({"repeated_child_const":"original-before-value-entry","obsolete_pattern_slots":"original-before-projection","all_equal":true}),
    );
}

#[test]
fn compiled_decision_shared_preparation_failure_blocks_later_builds() {
    use openbindings_internal_schema::Control;
    struct Poison<'a> {
        attempt: &'a dyn Control,
        calls: AtomicUsize,
        boundary: usize,
        at: std::sync::Mutex<Option<[usize; 3]>>,
    }
    impl WorkControl for Poison<'_> {
        fn is_cancelled(&self) -> bool {
            if self.calls.fetch_add(1, Ordering::Relaxed) == self.boundary {
                *self.at.lock().unwrap() = Some(compiled::entries::snapshot());
                let _ignored = self.attempt.charge_allocation(usize::MAX);
            }
            false
        }
    }
    struct Probe {
        boundary: usize,
        observations: std::sync::Mutex<Vec<serde_json::Value>>,
    }
    impl SchemaEvaluator for Probe {
        type Validator = DefaultValidator;
        fn prepare(
            &self,
            request: EvaluationRequest<'_>,
            limits: &PreparationLimits,
            _control: &dyn WorkControl,
        ) -> Result<Self::Validator, PreparationFailure> {
            Attempt::run(limits.max_sdk_work_units,limits.max_sdk_reservation_bytes,&||false,|attempt|{
                let environment=request.environment_with_control(limits.max_schema_nodes,limits.max_identity_bytes,attempt)?;
                let plan=semantic::Plan::new(environment,Limits::default(),*limits,attempt)?;
                let poison=Poison{attempt,calls:AtomicUsize::new(0),boundary:self.boundary,at:std::sync::Mutex::new(None)};
                compiled::entries::take();
                let result=compiled::Lane::prepare(&plan,&Limits::default(),limits,&poison,attempt,&|source|Ok(match pattern::read(source,32,1<<20) {Ok(pattern::Reading::Linear(linear))=>Some(linear),_=>None}));
                let counts=compiled::entries::take();
                if let Some(at)=*poison.at.lock().unwrap() {
                    assert_eq!(counts[1],at[1],"no build may begin after ignored nested reservation failure");
                    assert!(matches!(&result,Err(failure) if matches!(failure.kind,PreparationFailureKind::Limit{..})));
                }
                self.observations.lock().unwrap().push(json!({"boundary":self.boundary,"checks":poison.calls.load(Ordering::Relaxed),"at_poison":*poison.at.lock().unwrap(),"entries":counts}));
                let compiled=result?;
                Ok(DefaultValidator{plan:Arc::new(plan),compiled})
            }).map_err(PreparationFailure::from)?
        }
    }
    let mut rows = Vec::new();
    let document = document(r#"{"patternProperties":{"^(a|b)+$":{"type":"integer"}}}"#);
    for boundary in 0..40 {
        let probe = Probe {
            boundary,
            observations: std::sync::Mutex::new(Vec::new()),
        };
        let _result = prepare_document(
            &document,
            &ExternalSchemas::default(),
            &probe,
            &PreparationLimits::default(),
        );
        rows.extend(probe.observations.into_inner().unwrap());
    }
    assert!(rows.iter().any(|r| r["at_poison"].is_array()));
    assert!(rows.iter().any(|r| r["entries"][1].as_u64().unwrap() > 0));
    write(
        "sticky-construction.json",
        &json!({"observations":rows,"no_backend_after_shared_failure":true}),
    );
}

#[test]
fn compiled_decision_protocol_routes() {
    let Ok(directory) = std::env::var("OB_COMPILED_RECEIPTS") else {
        return;
    };
    let fixtures: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&directory).join("protocol-fixtures.json")).unwrap(),
    )
    .unwrap();
    let mut routes = Vec::new();
    for (name, fixture) in fixtures.as_object().unwrap() {
        let Some(schema) = fixture.get("schema").and_then(|x| x.as_str()) else {
            continue;
        };
        let mut resources = Vec::new();
        if let Some(supplied) = fixture.get("resources").and_then(|r| r.as_object()) {
            for (uri, text) in supplied {
                resources.push(ExternalSchema {
                    uri: uri.clone(),
                    document: JsonValue::parse(text.as_str().unwrap()).unwrap(),
                });
            }
        }
        resources.push(ExternalSchema {
            uri: "https://benchmark.invalid/root".into(),
            document: JsonValue::parse(schema).unwrap(),
        });
        let resources = ExternalSchemas::new(resources).unwrap();
        let doc = document(r#"{"$ref":"https://benchmark.invalid/root"}"#);
        let mut limits = Limits::default();
        if let Some(work) = fixture
            .get("limits")
            .and_then(|l| l.get("evaluation_steps"))
            .and_then(|n| n.as_u64())
        {
            limits.evaluation_sdk_work_units = Some(work as usize);
        }
        compiled::entries::take();
        let selected = prepare_document(
            &doc,
            &resources,
            &DefaultEvaluator::with_limits(limits.clone()),
            &PreparationLimits::default(),
        )
        .unwrap();
        let construction = compiled::entries::take();
        let original = prepare_document(
            &doc,
            &resources,
            &Original(limits),
            &PreparationLimits::default(),
        )
        .unwrap();
        let value = JsonValue::parse(fixture["value"].as_str().unwrap()).unwrap();
        let detail = if fixture["upstream"] == true
            || fixture["modes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m == "explain")
        {
            Detail::Explain
        } else {
            Detail::Outcome
        };
        let control = AtomicBool::new(fixture["cancelled"] == true);
        compiled::entries::take();
        let outcome = selected.evaluate(&value, detail, &control);
        let entries = compiled::entries::take();
        assert_eq!(
            serialized(outcome),
            serialized(original.evaluate(&value, detail, &control)),
            "protocol {name}"
        );
        let reason = match &selected.compiled {
            compiled::Lane::Original(reason) => *reason,
            compiled::Lane::Compiled(_) => "eligible",
        };
        if reason != "eligible" {
            assert_eq!(construction, [0, 0, 0]);
        }
        routes.push(json!({"workload":name,"preparation":reason,"construction":construction,"detail":format!("{detail:?}"),"value_entries":entries[2],"lane":if entries[2]>0 {"compiled"}else{"original"}}));
    }
    write(
        "protocol-routes.json",
        &json!({"all_observations_equal":true,"routes":routes}),
    );
}

#[test]
fn compiled_decision_large_pattern_current_positive() {
    let (selected, original) = owners(
        r#"{"type":"object","patternProperties":{"^.{1,10000}$":false}}"#,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    compare(
        &selected,
        &original,
        &JsonValue::parse(r#"{"a":1}"#).unwrap(),
    );
}

#[test]
fn shared_preparation_targets_edges_and_lowering_match_original() {
    for schema in [
        r##"{"type":"integer","minimum":0}"##.to_owned(),
        r##"{"$defs":{"leaf":{"type":"integer"}},"allOf":[{"$ref":"#/$defs/leaf"},{"properties":{"x":{"$ref":"#/$defs/leaf"}}},{"if":{"type":"array"},"then":{"items":{"$ref":"#/$defs/leaf"}},"else":{"not":false}}]}"##.to_owned(),
        format!(r##"{{"$defs":{{"leaf":{{"type":"integer"}}}},"allOf":[{}]}}"##,vec![r##"{"$ref":"#/$defs/leaf"}"##;5000].join(",")),
    ] {
        let (selected,_)=owners(&schema,&ExternalSchemas::default(),Limits::default(),PreparationLimits::default());
        crate::compiled::verify_shared_reach(&selected.plan);
    }
}

#[test]
fn a_skipped_active_carrier_cannot_disappear_from_shared_compiler_facts() {
    let u = "https://closed-facts.test/U";
    let v = "https://closed-facts.test/V";
    for schema in [
        json!({"properties":{"x":{"$id":u,"type":"string"}}}),
        json!({"allOf":[{"$id":u,"type":"string"}]}),
    ] {
        let supplied = ExternalSchemas::new([ExternalSchema {
            uri: v.into(),
            document: JsonValue::parse(schema.to_string()).unwrap(),
        }])
        .unwrap();
        let document=json!({"openbindings":"0.2.0","schemas":{"owner":{"$id":u,"type":"integer"}},"operations":{"op":{"input":{"$ref":v}}}}).to_string();
        compiled::entries::take();
        let selected = prepare_document(
            &document,
            &supplied,
            &DefaultEvaluator::new(),
            &PreparationLimits::default(),
        )
        .unwrap();
        assert_eq!(compiled::entries::take(), [0, 0, 0]);
        assert!(matches!(
            selected.compiled,
            compiled::Lane::Original("incomplete-prepared-closure")
        ));
        assert!(selected.plan.compiled_reach(&|| false).unwrap().is_none());
        let original = prepare_document(
            &document,
            &supplied,
            &Original(Limits::default()),
            &PreparationLimits::default(),
        )
        .unwrap();
        for text in ["{}", r#"{"x":1}"#, r#"{"x":"a"}"#] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "original"
            );
        }
    }
}

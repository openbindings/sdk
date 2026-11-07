//! Decision experiment controls. No route counters are compiled into timing peers.
use super::*;
use openbindings_internal_schema::preparation::{Attempt, EnvironmentRequest};
use openbindings_schema_evaluator_test_support::{run, suite};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static HEALTHY: AtomicBool = AtomicBool::new(false);
mod expansion;
mod literal_scratch;
mod product_corpus;

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
fn shared_long_literal_and_run_matchers_keep_compiled_outcomes_and_diagnostics() {
    let body = "a".repeat(10_000);
    let schema = json!({"properties": {
        "literal": {"pattern": format!("^{body}$")},
        "alias": {"pattern": format!(r"^\x61{}$", &body[1..])},
        "runs": {"pattern": "^ab[0-9]{2}$"}
    }});
    let (selected, original) = owners(
        &schema.to_string(),
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert!(matches!(selected.compiled, compiled::Lane::Compiled(_)));
    for value in [
        json!({"literal": body, "alias": body, "runs": "ab42"}),
        json!({"literal": "a", "alias": "a", "runs": "abx2"}),
    ] {
        let value = JsonValue::parse(value.to_string()).unwrap();
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
        for detail in [Detail::Outcome, Detail::Explain] {
            let cancelled = AtomicBool::new(true);
            assert_eq!(
                serialized(selected.evaluate(&value, detail, &cancelled)),
                serialized(original.evaluate(&value, detail, &cancelled)),
            );
        }
    }
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
            true, // Step2a qualifies this productive, allocation-free return.
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
                let result=compiled::Lane::prepare(&plan,&Limits::default(),limits,&poison,attempt,&|source|Ok(match pattern::read(source,32,1<<20) {Ok(pattern::Reading::Linear(linear))=>Some(linear.into()),_=>None}));
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

const RECURSIVE_INTEGER_ARRAY: &str = r##"{"$defs":{"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/node"}}]},"unused":{"const":false}},"$ref":"#/$defs/node"}"##;
const ORDINARY_NAMED: &str = r##"{"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","children"],"additionalProperties":false}"##;
const ORDINARY_ENUM: &str = r##"{"type":"object","properties":{"name":{"type":"string"},"kind":{"enum":["branch","leaf"]},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","kind","children"],"additionalProperties":false}"##;
const ORDINARY_BINARY: &str = r##"{"type":["object","null"],"properties":{"left":{"$ref":"#"},"right":{"$ref":"#"}},"required":["left","right"],"additionalProperties":false}"##;
const PRODUCTIVE_MINIMUM: &str = r##"{"type":["object","null"],"properties":{"value":{"type":"integer","minimum":0},"left":{"$ref":"#"},"right":{"$ref":"#"}},"required":["value","left","right"],"additionalProperties":false}"##;
const PRODUCTIVE_ADDITIONAL: &str = r##"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":{"$ref":"#"}}"##;
const PRODUCTIVE_UNION: &str = r##"{"oneOf":[{"type":"object","properties":{"kind":{"const":"leaf"},"value":{"type":"integer"}},"required":["kind","value"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"branch"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["kind","children"],"additionalProperties":false}]}"##;
fn nested(leaf: &str, depth: usize) -> JsonValue {
    JsonValue::parse(format!("{}{leaf}{}", "[".repeat(depth), "]".repeat(depth))).unwrap()
}

#[test]
fn recursive_values_keep_exact_numbers_and_depth_selection() {
    let (selected, original) = owners(
        RECURSIVE_INTEGER_ARRAY,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let limit = selected
        .compiled
        .qualified_depth()
        .expect("compiled recursion");
    assert!((64..1024).contains(&limit));
    let mut rows = Vec::new();
    for depth in [0, 1, 32, 64, limit, limit + 1] {
        for leaf in ["0", "false", "[]", "[0,-1,[],[2]]"] {
            let lane = compare(&selected, &original, &nested(leaf, depth)).0;
            // The container leaves add their own depth; scalar leaves expose
            // the qualified boundary without depending on that extra nesting.
            if matches!(leaf, "0" | "false") {
                assert_eq!(
                    lane,
                    if depth <= limit {
                        "compiled"
                    } else {
                        "original"
                    }
                );
            }
            rows.push(json!({"depth":depth,"leaf":leaf,"lane":lane}));
        }
    }
    let huge = "9".repeat(3000);
    for token in ["-0", "9223372036854775808", "-9223372036854775809", &huge] {
        assert_eq!(
            compare(&selected, &original, &nested(token, 32)).0,
            "compiled"
        );
    }
    for token in ["1.0", "1e0", "1.2", "1e1000000", "-0.00"] {
        assert_eq!(
            compare(&selected, &original, &nested(token, 32)).0,
            "original"
        );
    }
    for text in [r#"["\ud800"]"#, r#"{"x":1,"x":2}"#] {
        assert_eq!(
            compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
            "original"
        );
    }
    for schema in [
        r##"{"type":["integer","null","array"],"items":{"$ref":"#"}}"##,
        r##"{"$defs":{"alias":{"$ref":"#/$defs/node"},"node":{"anyOf":[{"type":"integer"},{"type":"array","items":{"$ref":"#/$defs/alias"}}]}},"$ref":"#/$defs/alias"}"##,
    ] {
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        for text in ["0", "[]", "[0,-1,[],[2]]", "[false]"] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "compiled"
            );
        }
        assert_eq!(
            compare(&selected, &original, &nested("1.0", 32)).0,
            "original"
        );
    }
    compiled::entries::take();
    let (limited, limited_original) = owners(
        RECURSIVE_INTEGER_ARRAY,
        &ExternalSchemas::default(),
        Limits {
            evaluation_sdk_work_units: Some(1),
            ..Limits::default()
        },
        PreparationLimits::default(),
    );
    assert_eq!(compiled::entries::take(), [0, 0, 0]);
    assert_eq!(
        compare(&limited, &limited_original, &nested("0", 32)).0,
        "original"
    );
    write(
        "recursive-values.json",
        &json!({"all_equal":true,"qualified_depth":limit,"rows":rows,"plain_integer_tokens":"compiled including overflow and3000digits","decimal_exponent_tokens":"original-before-entry","unpaired_and_duplicates":"original-before-entry"}),
    );
}

#[test]
fn recursive_duplicate_routes_and_container_or_unique_exits_stay_before_projection() {
    let mut schemas = vec![
        r##"{"$ref":"#"}"##.to_owned(),
        r##"{"$defs":{"n":{"allOf":[{"$ref":"#/$defs/step"},{"$ref":"#/$defs/step"}]},"step":{"type":"array","items":{"$ref":"#/$defs/n"}}},"$ref":"#/$defs/n"}"##.to_owned(),
        r##"{"$defs":{"n":{"allOf":[{"$ref":"#/$defs/a"},{"$ref":"#/$defs/b"}]},"a":{"$ref":"#/$defs/step"},"b":{"$ref":"#/$defs/step"},"step":{"type":"array","items":{"$ref":"#/$defs/n"}}},"$ref":"#/$defs/n"}"##.to_owned(),
        r##"{"$defs":{"n":{"allOf":[{"items":{"$ref":"#/$defs/n"}},{"items":{"$ref":"#/$defs/n"}}]}},"$ref":"#/$defs/n"}"##.to_owned(),
        r##"{"$defs":{"n":{"anyOf":[{"type":"integer"},{"items":{"$ref":"#/$defs/n"}}],"allOf":[{"$ref":"#/$defs/m"}]},"m":{"anyOf":[{"type":"integer"},{"items":{"$ref":"#/$defs/m"}}]}},"$ref":"#/$defs/n"}"##.to_owned(),
    ];
    for predicate in [
        json!({"const":[]}),
        json!({"enum":[{},[]]}),
        json!({"uniqueItems":true}),
    ] {
        // Container equality and uniqueness have internal traversals that the
        // scalar path proof does not cover, even on terminal side branches.
        let mut schema: serde_json::Value = serde_json::from_str(RECURSIVE_INTEGER_ARRAY).unwrap();
        schema["$defs"]["node"]["allOf"] = json!([predicate]);
        schemas.push(schema.to_string());
    }
    let mut rows = Vec::new();
    for schema in schemas {
        compiled::entries::take();
        let (selected, original) = owners(
            &schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        assert_eq!(compiled::entries::take(), [0, 0, 0], "{schema}");
        let compiled::Lane::Original(reason) = selected.compiled else {
            panic!("unexpected compiled route: {schema}")
        };
        for text in ["0", "[]", "[[]]", "[0]"] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "original"
            );
        }
        rows.push(json!({"schema":schema,"reason":reason}));
    }
    write(
        "recursive-admission.json",
        &json!({"all_equal":true,"zero_projection_or_backend_entries":true,"rows":rows}),
    );
}

#[test]
fn recursive_preflight_and_completion_observed_cancellation_are_terminal() {
    struct Polls {
        at: usize,
        seen: AtomicUsize,
    }
    impl WorkControl for Polls {
        fn is_cancelled(&self) -> bool {
            self.seen.fetch_add(1, Ordering::Relaxed) == self.at
        }
    }
    let (selected, original) = owners(
        RECURSIVE_INTEGER_ARRAY,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let value = nested("0", 32);
    let healthy = Polls {
        at: usize::MAX,
        seen: AtomicUsize::new(0),
    };
    assert!(matches!(
        selected.evaluate(&value, Detail::Outcome, &healthy),
        ValueOutcome::Satisfies
    ));
    let total = healthy.seen.load(Ordering::Relaxed);
    assert!(total > 2);
    for (at, entries) in [(1, 0), (total - 1, 1)] {
        compiled::entries::take();
        let control = Polls {
            at,
            seen: AtomicUsize::new(0),
        };
        assert!(
            matches!(selected.evaluate(&value,Detail::Outcome,&control),ValueOutcome::NoVerdict{detail} if detail.reason==NoVerdictReason::Cancelled)
        );
        assert_eq!(compiled::entries::take(), [0, 0, entries]);
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
    write(
        "recursive-cancellation.json",
        &json!({"preflight_zero_backend":true,"completion_one_backend":true,"no_retry":true,"healthy_reuse":true,"actual_polls":total}),
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only compiled recursion boundary stack qualification"
)]
fn recursive_compiled_boundary_runs_on_qualified_native_threads() {
    for stack in [256 * 1024, 1024 * 1024] {
        let (selected, original) = owners(
            RECURSIVE_INTEGER_ARRAY,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                let limit = selected.compiled.qualified_depth().unwrap();
                for depth in [limit, limit + 1] {
                    for leaf in ["0", "false"] {
                        assert_eq!(
                            compare(&selected, &original, &nested(leaf, depth)).0,
                            if depth == limit {
                                "compiled"
                            } else {
                                "original"
                            }
                        );
                    }
                }
            })
            .unwrap()
            .join()
            .expect("qualified compiled recursion stack");
    }
    write(
        "recursive-stack.json",
        &json!({"native_thread_bytes":[256*1024,1024*1024],"compiled_exact_boundary":true,"original_beyond_boundary":true,"satisfying_and_failing":true}),
    );
}

#[test]
fn compiled_scalar_equality_preserves_an_enclosing_preparation_work_stop() {
    let (selected, original) = owners(
        r#"{"const":"node"}"#,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    let value = JsonValue::parse(r#""node""#).unwrap();
    assert_eq!(compare(&selected, &original, &value).0, "compiled");
    for poisoned in [false, true] {
        let mut observed = None;
        compiled::entries::take();
        let outer = Attempt::run(0, usize::MAX, &|| false, |attempt| {
            if poisoned {
                let _ = openbindings_internal_schema::Control::charge_work(attempt, 1);
            }
            observed = Some(selected.evaluate(&value, Detail::Outcome, &HEALTHY));
        });
        assert_eq!(outer.unwrap_err().code, "preparation-sdk-work-units");
        assert!(
            matches!(observed, Some(ValueOutcome::NoVerdict { detail }) if detail.reason == NoVerdictReason::LimitExceeded && detail.code == "evaluation-sdk-work-units"),
            "a stopped exact predicate cannot publish a provisional Boolean"
        );
        assert_eq!(compiled::entries::take(), [0, 0, usize::from(!poisoned)]);
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only ordinary recursion stack qualification"
)]
fn ordinary_recursive_boundaries_run_on_qualified_native_threads() {
    let schemas = [
        ("named", ORDINARY_NAMED),
        ("enum", ORDINARY_ENUM),
        ("binary", ORDINARY_BINARY),
    ];
    let mut rows = Vec::new();
    for (name, schema) in schemas {
        for stack in [256 * 1024, 1024 * 1024] {
            let (selected, original) = owners(
                schema,
                &ExternalSchemas::default(),
                Limits::default(),
                PreparationLimits::default(),
            );
            let limit = selected.compiled.qualified_depth().unwrap();
            // Linear chains reach the value-depth boundary without constructing
            // an exponentially large balanced tree. Parse before the small thread.
            let steps = if name == "binary" {
                limit
            } else {
                (limit - 1) / 2
            };
            let mut values = Vec::new();
            for beyond in [false, true] {
                for failing in [false, true] {
                    let mut text = if name == "binary" {
                        if failing { "42" } else { "null" }.to_owned()
                    } else {
                        format!(
                            r#"{{"name":{},"kind":{},"children":[]}}"#,
                            if failing && name == "named" {
                                "42"
                            } else {
                                "\"node\""
                            },
                            if failing && name == "enum" {
                                "42"
                            } else {
                                "\"leaf\""
                            }
                        )
                    };
                    if name == "named" {
                        text = text.replace(r#","kind":"leaf""#, "");
                    }
                    for _ in 0..steps + usize::from(beyond) {
                        text = if name == "binary" {
                            format!(r#"{{"left":{text},"right":null}}"#)
                        } else if name == "enum" {
                            format!(r#"{{"name":"node","kind":"branch","children":[{text}]}}"#)
                        } else {
                            format!(r#"{{"name":"node","children":[{text}]}}"#)
                        };
                    }
                    values.push((beyond, failing, JsonValue::parse(&text).unwrap()));
                }
            }
            std::thread::Builder::new()
                .stack_size(stack)
                .spawn(move || {
                    for (beyond, failing, value) in values {
                        assert_eq!(
                            matches!(
                                selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                                ValueOutcome::Satisfies
                            ),
                            !failing
                        );
                        assert_eq!(
                            compare(&selected, &original, &value).0,
                            if beyond { "original" } else { "compiled" }
                        );
                    }
                })
                .unwrap()
                .join()
                .expect("ordinary recursive boundary stack");
            rows.push(
                json!({"schema":name,"native_thread_bytes":stack,"qualified_value_depth":limit}),
            );
        }
    }
    write(
        "ordinary-recursive-stack.json",
        &json!({"rows":rows,"satisfying_and_failing":true,"original_beyond_boundary":true}),
    );
}

#[test]
fn ordinary_recursive_children_trees_and_scalar_enums_use_compiled() {
    fn named(depth: usize, with_enum: bool) -> serde_json::Value {
        let children = if depth == 0 {
            vec![]
        } else {
            vec![named(depth - 1, with_enum), named(depth - 1, with_enum)]
        };
        let mut node = json!({"name":"node","children":children});
        if with_enum {
            node["kind"] = json!(if depth == 0 { "leaf" } else { "branch" });
        }
        node
    }
    fn binary(depth: usize) -> serde_json::Value {
        if depth == 0 {
            serde_json::Value::Null
        } else {
            json!({"left":binary(depth-1),"right":binary(depth-1)})
        }
    }
    let base = json!({"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","children"],"additionalProperties":false});
    let mut with_enum = base.clone();
    with_enum["properties"]["kind"] = json!({"enum":["branch","leaf"]});
    with_enum["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("kind"));
    let tree = json!({"type":["object","null"],"properties":{"left":{"$ref":"#"},"right":{"$ref":"#"}},"required":["left","right"],"additionalProperties":false});
    let mut rows = Vec::new();
    for (name, schema, value, pointer) in [
        (
            "named",
            base,
            named(4, false),
            "/children/1/children/1/children/1/children/1/name",
        ),
        (
            "enum",
            with_enum,
            named(4, true),
            "/children/1/children/1/children/1/children/1/kind",
        ),
        ("binary", tree, binary(5), "/right/right/right/right/right"),
    ] {
        let (selected, original) = owners(
            &schema.to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        let positive = JsonValue::parse(value.to_string()).unwrap();
        assert!(matches!(
            selected.evaluate(&positive, Detail::Outcome, &HEALTHY),
            ValueOutcome::Satisfies
        ));
        assert_eq!(compare(&selected, &original, &positive).0, "compiled");
        let mut invalid = value.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!(42);
        let negative = JsonValue::parse(invalid.to_string()).unwrap();
        assert!(matches!(
            selected.evaluate(&negative, Detail::Outcome, &HEALTHY),
            ValueOutcome::Fails { .. }
        ));
        assert_eq!(compare(&selected, &original, &negative).0, "compiled");
        rows.push(json!({"shape":name,"positive":"compiled-S","late_error":"compiled-F"}));
    }
    // Literal-only exact work does not need integer preflight for other fields.
    for literal in [
        json!({"enum":["α",""]}),
        json!({"const":true}),
        json!({"const":null}),
    ] {
        let schema = json!({"properties":{"kind":literal,"children":{"items":{"$ref":"#"}}},"additionalProperties":true});
        let (selected, original) = owners(
            &schema.to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        for text in [
            r#"{"children":[],"extra":1.5}"#,
            r#"{"children":[{"kind":"α","extra":1e400}]}"#,
            r#"{"kind":"e\u0301"}"#,
        ] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "compiled"
            );
        }
    }
    write(
        "ordinary-recursive-values.json",
        &json!({"complete_observations_equal":true,"rows":rows,"scalar_literal_kinds":["string","boolean","null"],"unrelated_decimal_fields":"compiled"}),
    );
}

#[test]
fn recursive_child_partitions_keep_overlaps_and_alias_multiplicity_on_plan() {
    let cases = [
        (
            r##"{"properties":{"a":{"$ref":"#"},"ab":{"$ref":"#"}}}"##,
            r#"{"a":{},"ab":{}}"#,
            true,
        ),
        (
            r##"{"properties":{"α":{"$ref":"#"},"αβ":{"$ref":"#"}}}"##,
            r#"{"α":{},"αβ":{}}"#,
            true,
        ),
        (
            r##"{"properties":{"a":{"$ref":"#"}},"patternProperties":{"^b":{"$ref":"#"}}}"##,
            r#"{"a":{},"b1":{}}"#,
            true,
        ),
        (
            r##"{"properties":{"a":{"$ref":"#"}},"patternProperties":{"^a":{"$ref":"#"}}}"##,
            r#"{"a":{}}"#,
            false,
        ),
        (
            r##"{"patternProperties":{"^a":{"$ref":"#"},"^ab":{"$ref":"#"}}}"##,
            r#"{"ab":{}}"#,
            false,
        ),
        (
            r##"{"patternProperties":{"^a":{"$ref":"#"},"^b":{"$ref":"#"}}}"##,
            r#"{"a":{},"b":{}}"#,
            true,
        ),
        (
            r##"{"patternProperties":{"^a+$":{"$ref":"#"}}}"##,
            r#"{"aaa":{}}"#,
            true,
        ),
        (
            r##"{"prefixItems":[{"$ref":"#"},{"$ref":"#"}],"items":{"$ref":"#"}}"##,
            "[[],[],[]]",
            true,
        ),
        (
            r##"{"items":{"$ref":"#"},"contains":{"$ref":"#"},"minContains":0}"##,
            "[[]]",
            false,
        ),
        (
            r##"{"allOf":[{"properties":{"a":{"$ref":"#"}}},{"properties":{"a":{"$ref":"#"}}}]}"##,
            r#"{"a":{}}"#,
            false,
        ),
    ];
    let mut rows = Vec::new();
    for (schema, text, compiled_lane) in cases {
        compiled::entries::take();
        let (selected, original) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        let built = compiled::entries::take();
        if !compiled_lane {
            assert_eq!(built, [0, 0, 0], "{schema}");
        }
        let lane = compare(&selected, &original, &JsonValue::parse(text).unwrap()).0;
        assert_eq!(lane == "compiled", compiled_lane, "{schema}");
        rows.push(json!({"schema":schema,"lane":lane}));
    }
    write(
        "recursive-child-partitions.json",
        &json!({"all_equal":true,"rows":rows}),
    );
}

#[test]
fn productive_only_routes_admit_the_requested_families_and_keep_terminal_failures() {
    let cases = [
        (
            "oneof-kind-union",
            json!({"oneOf":[{"type":"object","properties":{"kind":{"const":"leaf"},"value":{"type":"integer"}},"required":["kind","value"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"branch"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["kind","children"],"additionalProperties":false}]}),
            json!({"kind":"branch","children":[{"kind":"leaf","value":1},{"kind":"leaf","value":2}]}),
        ),
        (
            "named-additional-schema",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":{"$ref":"#"}}),
            json!({"name":"root","left":{"name":"left"},"right":{"name":"right"}}),
        ),
        (
            "named-terminal-additional",
            json!({"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["name","children"],"additionalProperties":{"type":"string"}}),
            json!({"name":"root","note":"extra","children":[{"name":"left","note":"extra","children":[]},{"name":"right","note":"extra","children":[]}]}),
        ),
        (
            "tree-minimum",
            json!({"type":["object","null"],"properties":{"left":{"$ref":"#"},"right":{"$ref":"#"},"value":{"type":"integer","minimum":0}},"required":["left","right","value"],"additionalProperties":false}),
            json!({"value":0,"left":null,"right":{"value":1,"left":null,"right":null}}),
        ),
    ];
    let mut rows = Vec::new();
    for (name, schema, value) in cases {
        let (selected, original) = owners(
            &schema.to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        let value = JsonValue::parse(value.to_string()).unwrap();
        assert!(matches!(
            selected.evaluate(&value, Detail::Outcome, &HEALTHY),
            ValueOutcome::Satisfies
        ));
        assert_eq!(
            compare(&selected, &original, &value).0,
            "compiled",
            "{name}"
        );
        let mut bad: serde_json::Value = serde_json::from_str(value.text()).unwrap();
        match name {
            "oneof-kind-union" => bad["children"][1]["kind"] = json!("wrong"),
            "named-additional-schema" => bad["right"]["name"] = json!(42),
            "named-terminal-additional" => bad["children"][1]["note"] = json!(42),
            "tree-minimum" => bad["right"]["value"] = json!(-1),
            _ => unreachable!(),
        }
        let bad = JsonValue::parse(bad.to_string()).unwrap();
        assert!(matches!(
            selected.evaluate(&bad, Detail::Outcome, &HEALTHY),
            ValueOutcome::Fails { .. }
        ));
        assert_eq!(compare(&selected, &original, &bad).0, "compiled");
        let weight = selected.compiled.recursive_exact_weight().unwrap();
        if name == "tree-minimum" {
            assert_eq!(weight, 6);
        }
        rows.push(json!({"workload":name,"lane":"compiled","exact_weight":weight,"satisfying_and_late_failing_equal":true}));
    }
    write(
        "productive-recursive-families.json",
        &json!({"rows":rows,"full_observations_equal":true}),
    );
}

#[test]
fn alternating_recursive_entries_sum_terminal_numeric_paths() {
    let schema = r##"{"$defs":{"B":{"properties":{"v":{"minimum":0},"next":{"$ref":"#"}}}},"properties":{"next":{"$ref":"#/$defs/B"}},"allOf":[{"properties":{"next":{"properties":{"v":{"minimum":0}}}}}]}"##;
    let (selected, original) = owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert_eq!(selected.compiled.recursive_exact_weight(), Some(3));
    for text in [
        r#"{"next":{"v":1}}"#,
        r#"{"next":{"v":-1}}"#,
        r#"{"next":{"v":0.5,"next":{"next":{"v":2.5}}}}"#,
    ] {
        assert_eq!(
            compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
            "compiled"
        );
    }
    let huge = "1".repeat(4096);
    assert_eq!(
        compare(
            &selected,
            &original,
            &JsonValue::parse(format!(r#"{{"next":{{"v":{huge}}}}}"#)).unwrap()
        )
        .0,
        "compiled"
    );
    write(
        "alternating-entry-scratch.json",
        &json!({"summed_exact_weight":3,"max_weight_one_rejected_in_design":true,"full_observations_equal":true}),
    );
}

#[test]
fn discriminators_do_not_waive_overlapping_productive_routes() {
    let schema = r##"{"oneOf":[{"properties":{"kind":{"const":"a"},"next":{"$ref":"#"}},"required":["kind"]},{"properties":{"kind":{"const":"b"},"next":{"$ref":"#"}},"required":["kind"]}]}"##;
    compiled::entries::take();
    let (selected, original) = owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert_eq!(compiled::entries::take(), [0, 0, 0]);
    assert!(matches!(
        selected.compiled,
        compiled::Lane::Original("recursive-child-partition-profile")
    ));
    for text in [
        r#"{"kind":"a"}"#,
        r#"{"kind":"b","next":{"kind":"a"}}"#,
        r#"{"kind":"wrong"}"#,
    ] {
        assert_eq!(
            compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
            "original"
        );
    }
}

#[test]
fn terminal_diamonds_retain_a_finite_expansion_guard_without_scratch() {
    let schema = |depth: usize| {
        let mut defs = serde_json::Map::new();
        for i in 0..depth {
            let reference = json!({"$ref":format!("#/$defs/d{}",i+1)});
            defs.insert(
                format!("d{i}"),
                json!({"allOf":[reference.clone(),reference]}),
            );
        }
        defs.insert(
            format!("d{depth}"),
            json!({"properties":{"v":{"type":"integer"}}}),
        );
        json!({"properties":{"next":{"$ref":"#"}},"allOf":[{"$ref":"#/$defs/d0"}],"$defs":defs})
    };
    let mut rows = Vec::new();
    for depth in [8, 16] {
        compiled::entries::take();
        let (selected, original) = owners(
            &schema(depth).to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        let construction = compiled::entries::take();
        let expected = if depth == 8 { "compiled" } else { "original" };
        assert_eq!(construction, if depth == 8 { [1, 1, 0] } else { [0, 0, 0] });
        if depth == 16 {
            assert!(matches!(
                selected.compiled,
                compiled::Lane::Original("recursive-finite-expansion-profile")
            ));
        }
        assert_eq!(
            compare(
                &selected,
                &original,
                &JsonValue::parse(r#"{"v":0,"next":{}}"#).unwrap()
            )
            .0,
            expected
        );
        rows.push(json!({"diamond_depth":depth,"lane":expected,"construction":construction}));
    }
    write(
        "productive-terminal-diamonds.json",
        &json!({"rows":rows,"unit_guard_applies_to_allocation_free_integer":true}),
    );
}

#[test]
fn recursive_scalar_exact_exits_use_funded_finite_paths() {
    let mut rows = Vec::new();
    for predicate in [
        json!({"const":0}),
        json!({"enum":[0,1]}),
        json!({"minimum":0}),
        json!({"maximum":18446744073709551615u64}),
        json!({"multipleOf":2}),
    ] {
        let mut schema: serde_json::Value = serde_json::from_str(RECURSIVE_INTEGER_ARRAY).unwrap();
        schema["$defs"]["node"]["allOf"] = json!([predicate]);
        let (selected, original) = owners(
            &schema.to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        for text in ["0", "1", "[]", "[[]]", "[0]", "[[2]]", "[-1]"] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "compiled"
            );
        }
        // The existing recursive integer-domain guard still selects before entry.
        for text in ["1.0", "1e0", "[1e400]"] {
            assert_eq!(
                compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
                "original"
            );
        }
        rows.push(json!({"predicate":predicate,"exact_weight":selected.compiled.recursive_exact_weight(),"all_equal":true}));
    }
    write(
        "recursive-scalar-exits.json",
        &json!({"rows":rows,"container_unique_exclusions_unchanged":true}),
    );
}

#[test]
fn recursive_scalar_reservation_boundary_selects_before_backend_entry() {
    let schema = r##"{"type":["object","null"],"properties":{"n":{"type":"integer","minimum":0},"padding":{"type":"string"},"next":{"$ref":"#"}},"additionalProperties":false}"##;
    let (selected, original) = owners(
        schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert_eq!(selected.compiled.recursive_exact_weight(), Some(4));
    // Seven subject nodes, one one-byte schema operand, K4. The existing
    //16/256 schedule yields64*raw_bytes+15296, including its512-byte tail.
    const RAW_AT_LIMIT: usize = (48 * 1024 * 1024 - 15296) / 64;
    let template = r#"{"n":0,"padding":"","next":null}"#;
    for (token, needs_scratch) in [("0", false), ("9223372036854775808", true)] {
        for extra in [0, 1] {
            let text = format!(
                r#"{{"n":{token},"padding":"{}","next":null}}"#,
                "a".repeat(RAW_AT_LIMIT - template.len() - (token.len() - 1) + extra)
            );
            assert_eq!(text.len(), RAW_AT_LIMIT + extra);
            let value = JsonValue::parse(text).unwrap();
            assert_eq!(
                openbindings_internal_json::backend::node_count(value.view()),
                7
            );
            assert_eq!(
                compare(&selected, &original, &value).0,
                if needs_scratch && extra == 1 {
                    "original"
                } else {
                    "compiled"
                }
            );
        }
    }
    write(
        "recursive-reservation-boundary.json",
        &json!({"default_bytes":48*1024*1024,"exact_bound_compiled":true,"wide_integer_one_raw_byte_above_original_before_entry":true,"i64_above_bound_compiled":true,"raw_bytes_at_bound":RAW_AT_LIMIT}),
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only productive scalar recursion stack qualification"
)]
fn productive_scalar_recursive_boundaries_fit_qualified_threads() {
    let minimum = PRODUCTIVE_MINIMUM;
    let additional = PRODUCTIVE_ADDITIONAL;
    let union = PRODUCTIVE_UNION;
    let mut rows = Vec::new();
    for (name, schema) in [
        ("minimum", minimum),
        ("additional", additional),
        ("union", union),
    ] {
        for stack in [256 * 1024, 1024 * 1024] {
            for failing in [false, true] {
                let (selected, original) = owners(
                    schema,
                    &ExternalSchemas::default(),
                    Limits::default(),
                    PreparationLimits::default(),
                );
                let limit = selected.compiled.qualified_depth().unwrap();
                let mut values = Vec::new();
                let levels = match name {
                    "minimum" => limit,
                    "additional" => limit - 1,
                    _ => (limit - 1) / 2,
                };
                for beyond in [false, true] {
                    let mut text = match name {
                        "minimum" => "null".to_owned(),
                        "additional" => if failing {
                            r#"{"name":42}"#
                        } else {
                            r#"{"name":"leaf"}"#
                        }
                        .to_owned(),
                        _ => format!(
                            r#"{{"kind":"leaf","value":{}}}"#,
                            if failing { "false" } else { "0" }
                        ),
                    };
                    for i in 0..levels + usize::from(beyond) {
                        text = match name {
                            "minimum" => format!(
                                r#"{{"value":{},"left":{text},"right":null}}"#,
                                if i == 0 && failing { -1 } else { 0 }
                            ),
                            "additional" => format!(r#"{{"name":"node","next":{text}}}"#),
                            _ => format!(r#"{{"kind":"branch","children":[{text}]}}"#),
                        };
                    }
                    values.push((beyond, JsonValue::parse(text).unwrap()));
                }
                std::thread::Builder::new()
                    .stack_size(stack)
                    .spawn(move || {
                        for (beyond, value) in values {
                            assert_eq!(
                                matches!(
                                    selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                                    ValueOutcome::Satisfies
                                ),
                                !failing
                            );
                            assert_eq!(
                                compare(&selected, &original, &value).0,
                                if beyond { "original" } else { "compiled" }
                            );
                        }
                    })
                    .unwrap()
                    .join()
                    .expect("qualified productive scalar recursion stack");
                rows.push(json!({"schema":name,"native_thread_bytes":stack,"failing":failing,"qualified_depth":limit}));
            }
        }
    }
    write(
        "productive-recursive-stack.json",
        &json!({"rows":rows,"satisfying_and_failing_equal":true,"first_beyond_original":true}),
    );
}

#[test]
fn productive_name_subjects_and_mixed_application_edges_preserve_constraints() {
    let name_schema = r##"{"type":["object","string","integer"],"propertyNames":{"$ref":"#"},"properties":{"next":{"$ref":"#"}},"not":{"const":"bad"}}"##;
    let (selected, original) = owners(
        name_schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    for (text, satisfies) in [
        (r#"{"next":{"same":1},"same":1}"#, true),
        (r#"{"next":{"bad":1}}"#, false),
        (r#"{"bad":1}"#, false),
    ] {
        let value = JsonValue::parse(text).unwrap();
        assert_eq!(
            matches!(
                selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                ValueOutcome::Satisfies
            ),
            satisfies
        );
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
    assert_eq!(
        compare(
            &selected,
            &original,
            &JsonValue::parse(r#"{"\ud800":1}"#).unwrap()
        )
        .0,
        "original"
    );
    // Root→child occurs once via same-instance $ref and once via properties.
    let mixed = r##"{"$ref":"#/properties/child","properties":{"child":{"properties":{"score":{"minimum":0},"next":{"$ref":"#"}}}}}"##;
    let (selected, original) = owners(
        mixed,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    for (text, satisfies) in [
        (
            r#"{"score":0,"child":{"score":1,"next":{"score":2}}}"#,
            true,
        ),
        (r#"{"score":-1}"#, false),
        (r#"{"child":{"score":-1}}"#, false),
    ] {
        let value = JsonValue::parse(text).unwrap();
        assert_eq!(
            matches!(
                selected.evaluate(&value, Detail::Outcome, &HEALTHY),
                ValueOutcome::Satisfies
            ),
            satisfies
        );
        assert_eq!(compare(&selected, &original, &value).0, "compiled");
    }
    write(
        "productive-names-and-edge-kinds.json",
        &json!({"same_name_occurrences_distinct":true,"names_and_value_subjects":true,"unpaired_name_original":true,"mixed_same_target_edges_equal":true}),
    );
}

#[test]
fn several_recursive_entries_preserve_converging_terminal_paths() {
    let mut defs = serde_json::Map::new();
    for i in 0..3 {
        defs.insert(
            format!("d{i}"),
            json!({
                "properties":{"v":{"minimum":0},"next":{"$ref":format!("#/$defs/d{}",(i+1)%3)}},
                "allOf":[{"properties":{"next":{"properties":{"v":{"minimum":0}}}}}]
            }),
        );
    }
    let schema = json!({"$ref":"#/$defs/d0","$defs":defs});
    let (selected, original) = owners(
        &schema.to_string(),
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    assert_eq!(selected.compiled.recursive_exact_weight(), Some(8));
    for token in ["0".to_owned(), "-1".to_owned(), "1".repeat(4096)] {
        let text = format!(r#"{{"v":0,"next":{{"v":0,"next":{{"v":0,"next":{{"v":{token}}}}}}}}}"#);
        assert_eq!(
            compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
            "compiled"
        );
    }
    // A finite container comparison can descend internally to the same numeric
    // subject from several ancestors. Scalar-only admission must keep it out.
    for depth in [1, 2, 4] {
        let mut literal = json!(0);
        for _ in 0..depth {
            literal = json!({"next":literal});
        }
        let schema = json!({"properties":{"next":{"$ref":"#"}},"not":{"const":literal}});
        compiled::entries::take();
        let (selected, original) = owners(
            &schema.to_string(),
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        assert_eq!(compiled::entries::take(), [0, 0, 0]);
        let mut text = "1".repeat(4096);
        for _ in 0..depth + 2 {
            text = format!(r#"{{"next":{text}}}"#);
        }
        assert_eq!(
            compare(&selected, &original, &JsonValue::parse(text).unwrap()).0,
            "original"
        );
    }
    write(
        "several-recursive-entry-paths.json",
        &json!({"summed_exact_weight":8,"entry_types":3,"huge_numeric_subject_equal":true,"container_internal_paths_remain_original":true}),
    );
}

#[test]
fn recursive_wide_schema_exponents_fund_each_subject_before_entry() {
    // A nonzero wide exponent keeps a heap-backed schema power through exact
    // comparisons; leading zero text alone would normalize to inline400.
    let wide = format!("1e+{}", "1".repeat(4096));
    let wide_schema =
        format!(r##"{{"properties":{{"n":{{"maximum":{wide}}},"next":{{"$ref":"#"}}}}}}"##);
    let (wide_selected, wide_original) = owners(
        &wide_schema,
        &ExternalSchemas::default(),
        Limits::default(),
        PreparationLimits::default(),
    );
    for (token, satisfies) in [
        ("1".to_owned(), true),
        (format!("1e+{}", "2".repeat(4096)), false),
    ] {
        let text = format!(r#"{{"n":1,"next":{{"n":1,"next":{{"n":{token}}}}}}}"#);
        let subject = JsonValue::parse(text).unwrap();
        assert_eq!(
            matches!(
                wide_selected.evaluate(&subject, Detail::Outcome, &HEALTHY),
                ValueOutcome::Satisfies
            ),
            satisfies
        );
        assert_eq!(
            compare(&wide_selected, &wide_original, &subject).0,
            "compiled"
        );
    }

    let value_text = r#"{"n":0,"next":{"n":0,"next":null}}"#;
    let value = JsonValue::parse(value_text).unwrap();
    let count = openbindings_internal_json::backend::node_count(value.view());
    assert_eq!(count, 9);
    let reservation =
        |bytes: usize| (16 * (value_text.len() + bytes * count) + 256 * (2 * count)) * 2 + 512;
    let limit = 48 * 1024 * 1024;
    let largest = ((limit - 512) / 2 - 16 * value_text.len() - 512 * count) / (16 * count);
    assert!(reservation(largest) <= limit);
    assert!(reservation(largest + 1) > limit);
    let mut rows = Vec::new();
    for length in [128, largest, largest + 1] {
        let token = format!("1e+{}400", "0".repeat(length - 6));
        assert_eq!(token.len(), length);
        let schema = format!(
            r##"{{"type":["object","null"],"properties":{{"n":{{"maximum":{token}}},"next":{{"$ref":"#"}}}}}}"##
        );
        let (selected, original) = owners(
            &schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        assert_eq!(selected.compiled.recursive_exact_weight(), Some(2));
        assert!(matches!(
            selected.evaluate(&value, Detail::Outcome, &HEALTHY),
            ValueOutcome::Satisfies
        ));
        assert_eq!(
            compare(&selected, &original, &value).0,
            if length <= largest {
                "compiled"
            } else {
                "original"
            }
        );
        if length == 128 {
            let bad = JsonValue::parse(r#"{"n":0,"next":{"n":1e401,"next":null}}"#).unwrap();
            assert!(matches!(
                selected.evaluate(&bad, Detail::Outcome, &HEALTHY),
                ValueOutcome::Fails { .. }
            ));
            assert_eq!(compare(&selected, &original, &bad).0, "compiled");
        }
        rows.push(json!({"schema_operand_bytes":length,"subject_nodes":count,"reserved_bytes":reservation(length),"lane":if length<=largest {"compiled"} else {"original"}}));
    }
    write(
        "recursive-schema-exponent-boundary.json",
        &json!({"rows":rows,"default_reservation_bytes":limit,"full_observations_equal":true,"largest_operand_funded_and_next_byte_original_before_entry":true}),
    );
}

// `packages/typescript/test/compiled-depth.mjs` walks the Wasm build densely
// around these compiled boundaries, in container levels. Change both together.
#[test]
fn wasm_depth_walk_bounds_match_native_selection() {
    for (schema, expected) in [
        (RECURSIVE_INTEGER_ARRAY, 203),
        (ORDINARY_NAMED, 340),
        (ORDINARY_ENUM, 340),
        (ORDINARY_BINARY, 340),
        (PRODUCTIVE_MINIMUM, 340),
        (PRODUCTIVE_ADDITIONAL, 340),
        (PRODUCTIVE_UNION, 203),
    ] {
        let (selected, _) = owners(
            schema,
            &ExternalSchemas::default(),
            Limits::default(),
            PreparationLimits::default(),
        );
        assert_eq!(
            selected.compiled.qualified_depth(),
            Some(expected),
            "{schema}"
        );
    }
}

use crate::planner::BoundsRequest;
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use std::sync::{Arc, Mutex};
const U: &str = "https://recovery.invalid/U";
const V: &str = "https://recovery.invalid/V";
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn document(schema: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}},"unselected":{{"input":{{"$ref":"https://unselected.invalid/U"}}}}}}}}"#)).unwrap()
}
fn resources(entries: &[(&str, &str)]) -> ResourceSet {
    ResourceSet::new(entries.iter().map(|(uri, text)| SchemaResource {
        uri: (*uri).into(),
        document: json(text),
    }))
    .unwrap()
}
fn prepare(
    schema: &str,
    entries: ResourceSet,
    evaluator: Arc<dyn SchemaEvaluator>,
) -> ValueContract {
    match document(schema)
        .validate()
        .unwrap()
        .validated()
        .expect("conformant document")
        .value_contracts(evaluator, entries)
        .prepare("op", Direction::Input)
    {
        ValueContractPreparation::Ready(contract) => contract,
        other => panic!("{other:?}"),
    }
}
fn evidence(contract: &ValueContract) -> NoVerdict {
    match contract.validate(&json(r#"{"x":1,"a":1,"b":1}"#)) {
        ValueOutcome::NoVerdict { detail } => detail,
        other => panic!("{other:?}"),
    }
}
fn detail_json(detail: &NoVerdict) -> serde_json::Value {
    serde_json::to_value(detail).unwrap()
}
#[test]
fn bare_and_aliased_holes_are_ready_but_not_is_a_preparation_refusal() {
    for (schema, ready) in [
        (format!(r#"{{"$ref":"{U}"}}"#), true),
        (
            format!(
                r##"{{"$defs":{{"alias":{{"$ref":"{U}"}}}},"$ref":"#/operations/op/input/$defs/alias"}}"##
            ),
            true,
        ),
        (format!(r#"{{"not":{{"$ref":"{U}"}}}}"#), false),
    ] {
        let doc = document(&schema);
        let validation = doc.validate().unwrap();
        let context = validation
            .validated()
            .unwrap()
            .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
        match context.prepare("op", Direction::Input) {
            ValueContractPreparation::Ready(contract) if ready => {
                for value in ["null", "true", "1", r#""x""#, "[]", "{}"] {
                    assert!(matches!(contract.validate(&json(value)),
                        ValueOutcome::NoVerdict { detail }
                            if detail.reason == NoVerdictReason::ResourceUnavailable));
                }
            }
            ValueContractPreparation::NoVerdict { detail } if !ready => {
                assert_eq!(detail.reason, NoVerdictReason::ResourceUnavailable);
                assert_eq!(detail.code, "resource-unavailable");
            }
            other => panic!("unexpected preparation stage for {schema}: {other:?}"),
        }
    }
}

#[test]
fn preparation_is_selected_and_does_not_promise_value_decidability() {
    let default = || Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>;
    for schema in [
        "true".to_owned(),
        format!(
            r#"{{"then":{{"$ref":"{U}"}},"else":{{"$ref":"{U}"}},"contentSchema":{{"$ref":"{U}"}},"$defs":{{"unused":{{"$ref":"{U}"}}}}}}"#
        ),
    ] {
        let contract = prepare(&schema, ResourceSet::default(), default());
        assert!(matches!(
            contract.validate(&json("null")),
            ValueOutcome::Satisfies
        ));
    }
    for schema in [
        format!(r#"{{"$ref":"{U}"}}"#),
        format!(r#"{{"properties":{{"x":{{"$ref":"{U}"}}}}}}"#),
        format!(r#"{{"anyOf":[true,{{"$ref":"{U}"}}]}}"#),
        format!(
            r##"{{"$defs":{{"a":{{"$ref":"{U}"}}}},"allOf":[{{"$ref":"#/operations/op/input/$defs/a"}}]}}"##
        ),
    ] {
        let contract = prepare(&schema, ResourceSet::default(), default());
        if schema.contains("anyOf") {
            assert!(matches!(
                contract.validate(&json("null")),
                ValueOutcome::Satisfies
            ));
            continue;
        }
        let retained = contract.clone();
        assert_eq!(
            evidence(&contract).reason,
            NoVerdictReason::ResourceUnavailable
        );
        assert!(evidence(&contract).location.is_some());
        let saved = detail_json(&evidence(&contract));
        drop(contract);
        assert_eq!(detail_json(&evidence(&retained)), saved);
        if let ValueOutcome::NoVerdict { detail } = retained.validate(&json(r#"{"x":1}"#)) {
            assert_eq!(detail_json(&detail), saved);
        }
    }
    let limited = prepare(
        "true",
        ResourceSet::default(),
        Arc::new(DefaultEvaluator::with_limits(Limits {
            evaluation_steps: 0,
            ..Limits::default()
        })),
    );
    assert!(
        matches!(limited.validate(&json("null")), ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::LimitExceeded)
    );
}
#[test]
fn transitive_holes_and_original_snapshot_evidence_survive_new_contexts() {
    let schema = format!(r#"{{"$ref":"{U}"}}"#);
    let known = format!(r#"{{"properties":{{"x":{{"$ref":"{V}"}}}}}}"#);
    let old = prepare(
        &schema,
        resources(&[(U, &known)]),
        Arc::new(DefaultEvaluator::new()),
    );
    let old_detail = evidence(&old);
    let at = old_detail.location.as_ref().unwrap();
    assert_eq!(at.resource.as_deref(), Some(U));
    assert_eq!(at.pointer, "/properties/x/$ref");
    let original = detail_json(&evidence(&old));
    let new = prepare(
        &schema,
        resources(&[(U, &known), (V, "true")]),
        Arc::new(DefaultEvaluator::new()),
    );
    assert!(matches!(
        new.validate(&json(r#"{"x":1}"#)),
        ValueOutcome::Satisfies
    ));
    assert_eq!(detail_json(&evidence(&old)), original);
    assert!(matches!(
        old.validate(&json(r#"{"x":1}"#)),
        ValueOutcome::NoVerdict { .. }
    ));
    let two = prepare(
        &format!(r#"{{"properties":{{"a":{{"$ref":"{U}"}},"b":{{"$ref":"{V}"}}}}}}"#),
        ResourceSet::default(),
        Arc::new(DefaultEvaluator::with_limits(Limits::default())),
    );
    assert!(
        evidence(&two)
            .location
            .as_ref()
            .unwrap()
            .pointer
            .ends_with("/a/$ref")
    );
    let ValueOutcome::NoVerdict { detail } = two.validate(&json(r#"{"b":1}"#)) else {
        panic!()
    };
    assert_eq!(detail_json(&detail), detail_json(&evidence(&two))); // one witness, not necessarily active
}
struct Capture(Mutex<Option<SchemaRequest>>);
impl SchemaEvaluator for Capture {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        *self.0.lock().unwrap() = Some(request.clone());
        Err(NoVerdict::new(
            NoVerdictReason::EvaluatorFailure,
            "capture",
            "test capture",
        ))
    }
}
#[test]
fn direct_bounds_keeps_planner_diagnostics_while_default_restores_strict_refusal() {
    let schema = format!(r#"{{"not":{{"$ref":"{U}"}}}}"#);
    let doc = document(&schema);
    let capture = Arc::new(Capture(Mutex::new(None)));
    let _ = doc
        .validate()
        .unwrap()
        .validated()
        .expect("conformant document")
        .value_contracts(capture.clone(), ResourceSet::default())
        .prepare("op", Direction::Input);
    let request = capture.0.lock().unwrap().take().unwrap();
    let strict = request
        .evaluation_program(&std::sync::atomic::AtomicBool::new(false))
        .unwrap_err();
    let direct = request
        .evaluation_bounds(&std::sync::atomic::AtomicBool::new(false))
        .unwrap_err();
    assert_eq!(direct.code, "partial-nonpositive-influence");
    assert_eq!(direct.reason, NoVerdictReason::ConservativePreparation);
    let ValueContractPreparation::NoVerdict { detail } = doc
        .validate()
        .unwrap()
        .validated()
        .expect("conformant document")
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .prepare("op", Direction::Input)
    else {
        panic!()
    };
    assert_eq!(detail_json(&detail), detail_json(&strict));
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        request.evaluation_bounds(&cancelled).unwrap_err().reason,
        NoVerdictReason::Cancelled
    );
}
struct Legacy;
impl SchemaEvaluator for Legacy {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Legacy))
    }
}
impl PreparedSchema for Legacy {
    fn validate(&self, _: &JsonValue, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
struct Declared(NoVerdict);
impl SchemaEvaluator for Declared {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Self(self.0.clone())))
    }
}
impl PreparedSchema for Declared {
    fn validate(&self, _: &JsonValue, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::NoVerdict {
            detail: self.0.clone(),
        }
    }
}
#[test]
fn custom_evaluators_return_their_own_outcomes() {
    let legacy = prepare("true", ResourceSet::default(), Arc::new(Legacy));
    assert!(matches!(
        legacy.validate(&json("null")),
        ValueOutcome::Satisfies
    ));
    let mut own = NoVerdict::new(
        NoVerdictReason::ResourceUnavailable,
        "custom",
        "custom evidence",
    );
    own.location = Some(SchemaLocation {
        resource: None,
        pointer: "/operations/op/input".into(),
    });
    let custom = prepare(
        "true",
        ResourceSet::default(),
        Arc::new(Declared(own.clone())),
    );
    assert_eq!(detail_json(&evidence(&custom)), detail_json(&own));
}

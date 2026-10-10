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
) -> PreparedContract {
    match document(schema)
        .value_contracts(evaluator, entries)
        .unwrap()
        .prepare("op", Side::Input)
    {
        ContractPreparation::Ready(contract) => contract,
        other => panic!("{other:?}"),
    }
}
fn evidence(contract: &PreparedContract) -> &NoVerdict {
    match contract.resource_completeness() {
        ResourceCompleteness::Incomplete { evidence, .. } => evidence,
        other => panic!("{other:?}"),
    }
}
fn detail_json(detail: &NoVerdict) -> serde_json::Value {
    serde_json::to_value(detail).unwrap()
}
#[test]
fn completeness_is_selected_borrowed_and_independent_of_value_decidability() {
    let default = || Arc::new(DefaultEvaluator::new()) as Arc<dyn SchemaEvaluator>;
    for schema in [
        "true".to_owned(),
        format!(
            r#"{{"then":{{"$ref":"{U}"}},"else":{{"$ref":"{U}"}},"contentSchema":{{"$ref":"{U}"}},"$defs":{{"unused":{{"$ref":"{U}"}}}}}}"#
        ),
    ] {
        let contract = prepare(&schema, ResourceSet::default(), default());
        assert!(matches!(
            contract.resource_completeness(),
            ResourceCompleteness::Complete
        ));
        assert_eq!(
            serde_json::to_string(&contract.resource_completeness()).unwrap(),
            r#"{"status":"complete"}"#
        );
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
        let retained = contract.clone();
        assert!(std::ptr::eq(evidence(&contract), evidence(&contract)));
        assert!(std::ptr::eq(evidence(&contract), evidence(&retained)));
        assert_eq!(
            evidence(&contract).reason,
            NoVerdictReason::ResourceUnavailable
        );
        assert!(evidence(&contract).location.is_some());
        let saved = detail_json(evidence(&contract));
        drop(contract);
        assert_eq!(detail_json(evidence(&retained)), saved);
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
    assert!(matches!(
        limited.resource_completeness(),
        ResourceCompleteness::Complete
    ));
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
    let at = evidence(&old).location.as_ref().unwrap();
    assert_eq!(at.resource.as_deref(), Some(U));
    assert_eq!(at.pointer, "/properties/x/$ref");
    let original = detail_json(evidence(&old));
    let new = prepare(
        &schema,
        resources(&[(U, &known), (V, "true")]),
        Arc::new(DefaultEvaluator::new()),
    );
    assert!(matches!(
        new.resource_completeness(),
        ResourceCompleteness::Complete
    ));
    assert!(matches!(
        new.validate(&json(r#"{"x":1}"#)),
        ValueOutcome::Satisfies
    ));
    assert_eq!(detail_json(evidence(&old)), original);
    assert!(matches!(
        old.validate(&json(r#"{"x":1}"#)),
        ValueOutcome::NoVerdict { .. }
    ));
    let two = prepare(
        &format!(r#"{{"properties":{{"a":{{"$ref":"{U}"}},"b":{{"$ref":"{V}"}}}}}}"#),
        ResourceSet::default(),
        Arc::new(DefaultEvaluator::with_limits(Limits {
            diagnostic_bytes: 0,
            ..Limits::default()
        })),
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
    assert_eq!(detail_json(&detail), detail_json(evidence(&two))); // one witness, not necessarily active
}
struct Capture(Mutex<Option<SchemaRequest>>);
impl SchemaEvaluator for Capture {
    fn prepare(
        &self,
        request: &SchemaRequest,
        _: &WorkControl,
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
    let schema = format!(r#"{{"oneOf":[true,{{"$ref":"{U}"}}]}}"#);
    let doc = document(&schema);
    let capture = Arc::new(Capture(Mutex::new(None)));
    let _ = doc
        .value_contracts(capture.clone(), ResourceSet::default())
        .unwrap()
        .prepare("op", Side::Input);
    let request = capture.0.lock().unwrap().take().unwrap();
    let strict = request.evaluation_program(&WorkControl::new()).unwrap_err();
    let direct = request.evaluation_bounds(&WorkControl::new()).unwrap_err();
    assert_eq!(direct.code, "partial-nonpositive-influence");
    assert_eq!(direct.reason, NoVerdictReason::ConservativePreparation);
    let ContractPreparation::NoVerdict { detail } = doc
        .value_contracts(Arc::new(DefaultEvaluator::new()), ResourceSet::default())
        .unwrap()
        .prepare("op", Side::Input)
    else {
        panic!()
    };
    assert_eq!(detail_json(&detail), detail_json(&strict));
    let cancelled = WorkControl::new();
    cancelled.cancel();
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
        _: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Legacy))
    }
}
impl PreparedSchema for Legacy {
    fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
struct Declared(NoVerdict);
impl SchemaEvaluator for Declared {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(Self(self.0.clone())))
    }
}
impl PreparedSchema for Declared {
    fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
        ValueOutcome::NoVerdict {
            detail: self.0.clone(),
        }
    }
    fn resource_completeness(&self) -> ResourceCompleteness<'_> {
        ResourceCompleteness::incomplete(&self.0)
    }
}
#[test]
fn custom_evaluators_default_to_undeclared_and_can_borrow_their_own_evidence() {
    let legacy = prepare("true", ResourceSet::default(), Arc::new(Legacy));
    assert!(matches!(
        legacy.resource_completeness(),
        ResourceCompleteness::Undeclared
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
    assert_eq!(detail_json(evidence(&custom)), detail_json(&own));
    assert!(std::ptr::eq(evidence(&custom), evidence(&custom)));
}

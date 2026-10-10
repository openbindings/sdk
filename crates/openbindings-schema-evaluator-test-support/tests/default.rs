use openbindings::*;
use openbindings_schema_evaluator_test_support::*;
use std::sync::Arc;
#[test]
fn pinned_suite_and_adversarial_contracts() {
    let report = run(
        Arc::new(openbindings_json_schema_evaluator::DefaultEvaluator::new()),
        &Options::without_unicode_property_matching(),
    );
    for observation in &report.observations {
        if !observation.failures.is_empty() {
            eprintln!(
                "{}: {:?} => {:?}",
                observation.id, observation.failures, observation.outcome
            );
        }
    }
    assert!(report.is_success(), "{:?}", report.configuration_failures);
    assert_eq!(report.observations.len(), 1566);
    assert_eq!(report.refusal_count(), 43);
}
#[test]
fn controls_reject_wrong_verdict_refusal_and_bogus_paths() {
    let group = suite()
        .groups
        .iter()
        .find(|g| g.id == "adversarial/path-through-ref")
        .unwrap();
    let case = &group.cases[0];
    let value = JsonValue::parse(&case.value).unwrap();
    assert!(!judge(case, &value, &ValueOutcome::Satisfies, false).is_empty());
    let refusal = ValueOutcome::NoVerdict {
        detail: NoVerdict::new(
            NoVerdictReason::Cancelled,
            "cancelled",
            "not actually cancelled",
        ),
    };
    assert!(!judge(case, &value, &refusal, true).is_empty());
    for path in ["/absent", "", "/a/absent"] {
        let wrong = ValueOutcome::Fails {
            problems: vec![ValueProblem {
                instance_pointer: path.into(),
                schema_location: None,
                code: "wrong".into(),
                message: "wrong".into(),
            }],
            problems_complete: true,
        };
        assert!(!judge(case, &value, &wrong, false).is_empty());
    }
    let valid = ValueOutcome::Fails {
        problems: vec![ValueProblem {
            instance_pointer: "/a".into(),
            schema_location: None,
            code: "type".into(),
            message: "expected string".into(),
        }],
        problems_complete: true,
    };
    assert!(judge(case, &value, &valid, false).is_empty());
}

/// An adapter can consume original source identity without depending on a bundler.
struct OriginalContextAdapter;
impl SchemaEvaluator for OriginalContextAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let original = request
            .document()
            .value()
            .at(&request.entry_location().pointer)
            .expect("original entry location")
            .to_owned();
        assert_eq!(original.semantic_eq(request.entry()), Some(true));
        openbindings_json_schema_evaluator::DefaultEvaluator::new().prepare(request, control)
    }
}
#[test]
fn external_adapter_receives_original_context_and_passes_the_kit() {
    let report = run(
        Arc::new(OriginalContextAdapter),
        &Options::without_unicode_property_matching(),
    );
    assert!(report.is_success());
}
struct Wrong;
struct WrongPrepared;
impl SchemaEvaluator for Wrong {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(WrongPrepared))
    }
}
impl PreparedSchema for WrongPrepared {
    fn validate(&self, _: &JsonValue, _: &WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
#[test]
fn controls_exercise_real_runner_and_reject_bad_options() {
    let mut options = Options::default();
    options
        .permitted_refusals
        .insert("not-a-case".into(), "unsupported".into());
    let report = run(Arc::new(Wrong), &options);
    assert!(!report.is_success());
    assert!(
        report
            .configuration_failures
            .iter()
            .any(|s| s.contains("unknown"))
    );
    assert!(
        report
            .configuration_failures
            .iter()
            .any(|s| s.contains("unused"))
    );
    assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    EmptyMismatch,
    UnknownSource,
    BroadUnsupported,
    FalseCancellation,
}
struct FaultyAdapter(Fault);
struct FaultyContract(Arc<dyn PreparedSchema>, Fault);
impl SchemaEvaluator for FaultyAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let prepared = openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, control)?;
        Ok(Arc::new(FaultyContract(prepared, self.0)))
    }
}
impl PreparedSchema for FaultyContract {
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        let outcome = self.0.validate(value, control);
        match self.1 {
            Fault::EmptyMismatch => ValueOutcome::Fails {
                problems: vec![],
                problems_complete: true,
            },
            Fault::UnknownSource => match outcome {
                ValueOutcome::Fails {
                    mut problems,
                    problems_complete,
                } => {
                    for problem in &mut problems {
                        problem.schema_location = Some(SchemaLocation {
                            resource: Some(
                                "https://not-an-original-resource.invalid/fabricated".into(),
                            ),
                            pointer: String::new(),
                        });
                    }
                    ValueOutcome::Fails {
                        problems,
                        problems_complete,
                    }
                }
                other => other,
            },
            Fault::BroadUnsupported => ValueOutcome::NoVerdict {
                detail: NoVerdict::new(
                    NoVerdictReason::UnsupportedCapability,
                    "unsupported",
                    "unsupported",
                ),
            },
            Fault::FalseCancellation => ValueOutcome::NoVerdict {
                detail: NoVerdict::new(NoVerdictReason::Cancelled, "cancelled", "not requested"),
            },
        }
    }
}
#[test]
fn real_runner_rejects_malformed_outcomes_fabricated_sources_and_broad_refusals() {
    for fault in [
        Fault::EmptyMismatch,
        Fault::UnknownSource,
        Fault::BroadUnsupported,
        Fault::FalseCancellation,
    ] {
        let report = run(
            Arc::new(FaultyAdapter(fault)),
            &Options::without_unicode_property_matching(),
        );
        assert!(!report.is_success(), "missed {fault:?}");
        assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
    }
}
struct LeakyAdapter(std::sync::Mutex<Option<Arc<dyn PreparedSchema>>>);
impl SchemaEvaluator for LeakyAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let mut cache = self.0.lock().unwrap();
        if let Some(prepared) = &*cache {
            return Ok(prepared.clone());
        }
        let prepared = openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, control)?;
        *cache = Some(prepared.clone());
        Ok(prepared)
    }
}
#[test]
fn real_runner_rejects_cross_context_compiled_state_leaks() {
    let report = run(
        Arc::new(LeakyAdapter(std::sync::Mutex::new(None))),
        &Options::default(),
    );
    assert!(!report.is_success());
    assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
}
#[test]
fn optional_capability_declaration_does_not_authorize_other_refusal_reasons() {
    let case = suite()
        .groups
        .iter()
        .flat_map(|g| &g.cases)
        .find(|c| c.optional_capability.is_some())
        .unwrap();
    let value = JsonValue::parse(&case.value).unwrap();
    for reason in [
        NoVerdictReason::ConservativePreparation,
        NoVerdictReason::ResourceUnavailable,
        NoVerdictReason::Cancelled,
        NoVerdictReason::LimitExceeded,
        NoVerdictReason::Undefined,
        NoVerdictReason::EvaluatorFailure,
    ] {
        let outcome = ValueOutcome::NoVerdict {
            detail: NoVerdict::new(reason, "refusal", "cannot establish"),
        };
        assert!(!judge(case, &value, &outcome, true).is_empty());
    }
}

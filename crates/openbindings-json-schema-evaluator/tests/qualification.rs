use openbindings::*;
use openbindings_json_schema_evaluator::DefaultValidator;
use openbindings_schema_evaluator_test_support::*;
#[test]
fn pinned_suite_and_adversarial_contracts() {
    let report = run(&openbindings_json_schema_evaluator::DefaultEvaluator::new());
    for observation in &report.observations {
        if !observation.failures.is_empty() {
            eprintln!(
                "{}: {:?} => {:?}",
                observation.id, observation.failures, observation.outcome
            );
        }
    }
    assert!(report.is_success(), "{:?}", report.configuration_failures);
    assert_eq!(report.observations.len(), 1546);
    assert_eq!(report.refusal_count(), 0);
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
    assert!(!judge(case, &value, &ValueOutcome::Satisfies).is_empty());
    let refusal = ValueOutcome::NoVerdict {
        detail: NoVerdict::new(
            NoVerdictReason::Cancelled,
            "cancelled",
            "not actually cancelled",
        ),
    };
    assert!(!judge(case, &value, &refusal).is_empty());
    let schema = SchemaOrigin::new(None, "/operations/op/input".parse().unwrap());
    let failure = |instance: &str| ValueOutcome::Fails {
        message: "value does not satisfy the schema".into(),
        problems: vec![ValueProblem::new(
            instance.parse().unwrap(),
            schema.clone(),
            "type",
            "expected string",
        )],
        problems_complete: true,
    };
    for path in ["/absent", "", "/a/absent"] {
        assert!(!judge(case, &value, &failure(path)).is_empty());
    }
    assert!(judge(case, &value, &failure("/a")).is_empty());
}

/// An adapter can consume original source identity without depending on a bundler.
struct OriginalContextAdapter;
impl SchemaEvaluator for OriginalContextAdapter {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        control.check()?;
        let root = request.environment(limits, control)?.root();
        let original = root
            .source()
            .at_pointer(root.pointer())
            .expect("original entry location")
            .to_owned();
        assert_eq!(original.semantic_eq(&root.value()), Some(true));
        openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, limits, control)
    }
}
#[test]
fn external_adapter_receives_original_context_and_passes_the_kit() {
    let report = run(&OriginalContextAdapter);
    assert!(report.is_success());
}
struct Wrong;
struct WrongPrepared;
impl SchemaEvaluator for Wrong {
    type Validator = WrongPrepared;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Ok(WrongPrepared)
    }
}
impl SchemaValidator for WrongPrepared {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
#[test]
fn controls_exercise_real_runner_and_reject_wrong_answers() {
    let report = run(&Wrong);
    assert!(!report.is_success());
    assert!(report.configuration_failures.is_empty());
    assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    EmptyMismatch,
    UnknownSource,
    BroadUnsupported,
    FalseCancellation,
    // The cheap check returns the explanation's problems.
    DetailedCheck,
    // The explanation reaches another verdict than the cheap check.
    DisagreeingExplanation,
    // An interrupted explanation answers satisfies instead of no verdict.
    InterruptedSatisfies,
    // Problems name their instance from the value's source rather than the value.
    SourceRootedInstance,
}
struct FaultyAdapter(Fault);
struct FaultyContract(DefaultValidator, Fault);
impl SchemaEvaluator for FaultyAdapter {
    type Validator = FaultyContract;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let prepared = openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, limits, control)?;
        Ok(FaultyContract(prepared, self.0))
    }
}
impl SchemaValidator for FaultyContract {
    fn evaluate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        let outcome = self.0.evaluate(value, detail, control);
        match self.1 {
            Fault::DetailedCheck => self.0.evaluate(value, Detail::Explain, control),
            Fault::DisagreeingExplanation if detail == Detail::Explain => match outcome {
                ValueOutcome::Satisfies => ValueOutcome::Fails {
                    message: "value does not satisfy the schema".into(),
                    problems: vec![],
                    problems_complete: false,
                },
                _ => ValueOutcome::Satisfies,
            },
            Fault::DisagreeingExplanation => outcome,
            Fault::InterruptedSatisfies if detail == Detail::Explain => match outcome {
                ValueOutcome::NoVerdict { detail }
                    if detail.reason == NoVerdictReason::Cancelled =>
                {
                    ValueOutcome::Satisfies
                }
                other => other,
            },
            Fault::InterruptedSatisfies => outcome,
            Fault::EmptyMismatch => ValueOutcome::Fails {
                message: "value does not satisfy the schema".into(),
                problems: vec![],
                problems_complete: true,
            },
            Fault::UnknownSource => match outcome {
                ValueOutcome::Fails {
                    mut problems,
                    problems_complete,
                    ..
                } => {
                    for problem in &mut problems {
                        problem.schema = SchemaOrigin::new(
                            Some("https://not-an-original-resource.invalid/fabricated".into()),
                            "".parse().unwrap(),
                        );
                    }
                    ValueOutcome::Fails {
                        problems,
                        problems_complete,
                        message: "value does not satisfy the schema".into(),
                    }
                }
                other => other,
            },
            Fault::SourceRootedInstance => match outcome {
                ValueOutcome::Fails {
                    mut problems,
                    problems_complete,
                    ..
                } => {
                    let root = value.location().pointer.expect("a value's pointer");
                    for problem in &mut problems {
                        problem.instance = format!("{root}{}", problem.instance).parse().unwrap();
                    }
                    ValueOutcome::Fails {
                        problems,
                        problems_complete,
                        message: "value does not satisfy the schema".into(),
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
        Fault::DetailedCheck,
        Fault::DisagreeingExplanation,
        Fault::InterruptedSatisfies,
        Fault::SourceRootedInstance,
    ] {
        let report = run(&FaultyAdapter(fault));
        assert!(!report.is_success(), "missed {fault:?}");
        assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
    }
}
struct LeakyAdapter(std::sync::Mutex<Option<DefaultValidator>>);
impl SchemaEvaluator for LeakyAdapter {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        let mut cache = self.0.lock().unwrap();
        if let Some(prepared) = &*cache {
            return Ok(prepared.clone());
        }
        let prepared = openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, limits, control)?;
        *cache = Some(prepared.clone());
        Ok(prepared)
    }
}
#[test]
fn real_runner_rejects_cross_context_compiled_state_leaks() {
    let report = run(&LeakyAdapter(std::sync::Mutex::new(None)));
    assert!(!report.is_success());
    assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
}
#[test]
fn cheap_checks_report_no_problems_and_agree_with_explanations() {
    let failure = |problems: Vec<ValueProblem>, problems_complete| ValueOutcome::Fails {
        message: "value does not satisfy the schema".into(),
        problems,
        problems_complete,
    };
    let problem = ValueProblem::new(
        "".parse().unwrap(),
        SchemaOrigin::new(None, "".parse().unwrap()),
        "type",
        "expected string",
    );
    assert!(
        judge_detail(
            &failure(vec![], false),
            &failure(vec![problem.clone()], false)
        )
        .is_empty()
    );
    assert!(judge_detail(&ValueOutcome::Satisfies, &ValueOutcome::Satisfies).is_empty());
    for (outcome, explanation) in [
        (
            failure(vec![problem.clone()], false),
            failure(vec![problem.clone()], false),
        ),
        (failure(vec![], true), failure(vec![problem.clone()], true)),
        (failure(vec![], false), ValueOutcome::Satisfies),
        (ValueOutcome::Satisfies, failure(vec![problem], false)),
    ] {
        assert!(!judge_detail(&outcome, &explanation).is_empty());
    }
}
#[test]
fn capability_refusals_are_gaps_for_every_finite_semantic_case() {
    let case = &suite().groups[0].cases[0];
    let value = JsonValue::parse(&case.value).unwrap();
    for reason in [
        NoVerdictReason::UnsupportedCapability,
        NoVerdictReason::ConservativePreparation,
        NoVerdictReason::ResourceUnavailable,
        NoVerdictReason::Cancelled,
        NoVerdictReason::LimitExceeded,
        NoVerdictReason::EvaluatorFailure,
    ] {
        let outcome = ValueOutcome::NoVerdict {
            detail: NoVerdict::new(reason, "refusal", "cannot establish"),
        };
        assert!(!judge(case, &value, &outcome).is_empty());
    }
}

#[test]
fn undefined_is_an_answer_and_declining_it_is_not_that_answer() {
    let mut case = suite().groups[0].cases[0].clone();
    case.expected = "undefined".into();
    let value = JsonValue::parse(&case.value).unwrap();
    let outcome = ValueOutcome::Undefined {
        detail: UndefinedResult::new("proof", "established undefined result"),
    };
    assert!(judge(&case, &value, &outcome).is_empty());
    let refusal = ValueOutcome::NoVerdict {
        detail: NoVerdict::new(
            NoVerdictReason::ConservativePreparation,
            "unknown",
            "not proved",
        ),
    };
    assert!(!judge(&case, &value, &refusal).is_empty());
    let unexplained = ValueOutcome::Undefined {
        detail: UndefinedResult::new("", ""),
    };
    assert!(!judge(&case, &value, &unexplained).is_empty());
}

#[test]
fn format_is_an_annotation_where_the_assertion_vocabulary_is_declared_false() {
    // OBI 5.2 reads `format` as an annotation where JSON Schema leaves its
    // assertion optional, so an invalid address satisfies a schema whose
    // meta-schema declares the format-assertion vocabulary false. The suite's
    // `fails` is the verdict of an implementation that asserts the vocabulary.
    let case = suite()
        .groups
        .iter()
        .flat_map(|g| &g.cases)
        .find(|c| {
            c.id.starts_with("suite/draft2020-12/optional/format-assertion.json#0/1 ")
        })
        .unwrap();
    assert_eq!(case.expected, "satisfies");
    let value = JsonValue::parse(&case.value).unwrap();
    assert!(judge(case, &value, &ValueOutcome::Satisfies).is_empty());
    let failure = ValueOutcome::Fails {
        message: "value does not satisfy the schema".into(),
        problems: vec![],
        problems_complete: false,
    };
    assert!(!judge(case, &value, &failure).is_empty());
    let refusal = ValueOutcome::NoVerdict {
        detail: NoVerdict::new(
            NoVerdictReason::UnsupportedCapability,
            "schema-dialect",
            "the dialect is not read",
        ),
    };
    assert!(!judge(case, &value, &refusal).is_empty());
}

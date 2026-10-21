use openbindings::*;
use openbindings_schema_evaluator_test_support::*;
use std::sync::Arc;
// The default evaluator's qualified limitations.
fn default_options() -> Options {
    Options::without(&[
        "unicode-property-matching",
        "backtracking-pattern-matching",
        "undefined-results",
    ])
}
#[test]
fn pinned_suite_and_adversarial_contracts() {
    let report = run(
        &openbindings_json_schema_evaluator::DefaultEvaluator::new(),
        &default_options(),
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
    assert_eq!(report.observations.len(), 1564);
    // 32 permitted refusals, with 13 more for its pattern engine and its
    // undefined results: 10 undefined results, one lookahead and two values a
    // property-escape `patternProperties` name does not reach.
    assert_eq!(report.refusal_count(), 45);
    assert_eq!(
        report
            .observations
            .iter()
            .filter(|o| matches!(o.outcome, Some(ValueOutcome::Undefined { .. })))
            .count(),
        0
    );
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
    let schema = SchemaLocation::new(None, "/operations/op/input".parse().unwrap());
    let failure = |instance: &str| ValueOutcome::Fails {
        message: "value does not satisfy the contract".into(),
        problems: vec![ValueProblem::new(
            instance.parse().unwrap(),
            schema.clone(),
            "type",
            "expected string",
        )],
        problems_complete: true,
    };
    for path in ["/absent", "", "/a/absent"] {
        assert!(!judge(case, &value, &failure(path), false).is_empty());
    }
    assert!(judge(case, &value, &failure("/a"), false).is_empty());
}

/// An adapter can consume original source identity without depending on a bundler.
struct OriginalContextAdapter;
impl SchemaEvaluator for OriginalContextAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let original = request
            .document()
            .value()
            .at(request.entry_location().pointer.as_str().unwrap())
            .expect("original entry location")
            .to_owned();
        assert_eq!(original.semantic_eq(request.entry()), Some(true));
        openbindings_json_schema_evaluator::DefaultEvaluator::new().prepare(request, control)
    }
}
#[test]
fn external_adapter_receives_original_context_and_passes_the_kit() {
    let report = run(&OriginalContextAdapter, &default_options());
    assert!(report.is_success());
}
struct Wrong;
struct WrongPrepared;
impl SchemaEvaluator for Wrong {
    fn prepare(
        &self,
        _: &SchemaRequest,
        _: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        Ok(Arc::new(WrongPrepared))
    }
}
impl PreparedSchema for WrongPrepared {
    fn validate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        ValueOutcome::Satisfies
    }
}
#[test]
fn controls_exercise_real_runner_and_reject_bad_options() {
    let mut options = Options::default();
    options
        .permitted_refusals
        .insert("not-a-case".into(), "unsupported".into());
    let report = run(&Wrong, &options);
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
struct FaultyContract(Arc<dyn PreparedSchema>, Fault);
impl SchemaEvaluator for FaultyAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        let prepared = openbindings_json_schema_evaluator::DefaultEvaluator::new()
            .prepare(request, control)?;
        Ok(Arc::new(FaultyContract(prepared, self.0)))
    }
}
impl PreparedSchema for FaultyContract {
    fn validate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        let outcome = self.0.validate(value, detail, control);
        match self.1 {
            Fault::DetailedCheck => self.0.validate(value, Detail::Explain, control),
            Fault::DisagreeingExplanation if detail == Detail::Explain => match outcome {
                ValueOutcome::Satisfies => ValueOutcome::Fails {
                    message: "value does not satisfy the contract".into(),
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
                message: "value does not satisfy the contract".into(),
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
                        problem.schema = SchemaLocation::new(
                            Some("https://not-an-original-resource.invalid/fabricated".into()),
                            "".parse().unwrap(),
                        );
                    }
                    ValueOutcome::Fails {
                        problems,
                        problems_complete,
                        message: "value does not satisfy the contract".into(),
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
                        message: "value does not satisfy the contract".into(),
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
        let report = run(&FaultyAdapter(fault), &default_options());
        assert!(!report.is_success(), "missed {fault:?}");
        assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
    }
}
struct LeakyAdapter(std::sync::Mutex<Option<Arc<dyn PreparedSchema>>>);
impl SchemaEvaluator for LeakyAdapter {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
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
        &LeakyAdapter(std::sync::Mutex::new(None)),
        &Options::default(),
    );
    assert!(!report.is_success());
    assert!(report.observations.iter().any(|o| !o.failures.is_empty()));
}
#[test]
fn cheap_checks_report_no_problems_and_agree_with_explanations() {
    let failure = |problems: Vec<ValueProblem>, problems_complete| ValueOutcome::Fails {
        message: "value does not satisfy the contract".into(),
        problems,
        problems_complete,
    };
    let problem = ValueProblem::new(
        "".parse().unwrap(),
        SchemaLocation::new(None, "".parse().unwrap()),
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
        NoVerdictReason::EvaluatorFailure,
    ] {
        let outcome = ValueOutcome::NoVerdict {
            detail: NoVerdict::new(reason, "refusal", "cannot establish"),
        };
        assert!(!judge(case, &value, &outcome, true).is_empty());
    }
}

#[test]
fn undefined_is_an_answer_and_declining_it_needs_the_declaration() {
    let case = suite()
        .groups
        .iter()
        .flat_map(|g| &g.cases)
        .find(|c| c.expected == "undefined")
        .unwrap();
    assert_eq!(
        case.optional_capability.as_deref(),
        Some("undefined-results")
    );
    let value = JsonValue::parse(&case.value).unwrap();
    let outcome = ValueOutcome::Undefined {
        detail: UndefinedResult::new("proof", "established undefined result"),
    };
    assert!(judge(case, &value, &outcome, false).is_empty());
    let refusal = |reason| ValueOutcome::NoVerdict {
        detail: NoVerdict::new(reason, "unknown", "not proved"),
    };
    // Declining to establish the undefined result is the stated refusal, and
    // only with the caller's declaration.
    assert!(
        !judge(
            case,
            &value,
            &refusal(NoVerdictReason::ConservativePreparation),
            false
        )
        .is_empty()
    );
    assert!(
        judge(
            case,
            &value,
            &refusal(NoVerdictReason::ConservativePreparation),
            true
        )
        .is_empty()
    );
    assert!(
        !judge(
            case,
            &value,
            &refusal(NoVerdictReason::UnsupportedCapability),
            true
        )
        .is_empty()
    );
    let optional = suite()
        .groups
        .iter()
        .flat_map(|g| &g.cases)
        .find(|c| c.optional_capability.as_deref() == Some("unicode-property-matching"))
        .unwrap();
    assert!(
        !judge(
            optional,
            &JsonValue::parse(&optional.value).unwrap(),
            &outcome,
            true
        )
        .is_empty()
    );
    let unexplained = ValueOutcome::Undefined {
        detail: UndefinedResult::new("", ""),
    };
    assert!(!judge(case, &value, &unexplained, false).is_empty());
}

#[test]
fn other_dialect_cases_accept_the_specification_verdict_or_a_truthful_refusal() {
    // A schema reached through an external URI follows its own dialect, so a
    // capable evaluator answers with JSON Schema's verdict; one that does not
    // read the dialect may refuse it, needing no declaration.
    let cases: Vec<_> = suite()
        .groups
        .iter()
        .flat_map(|g| &g.cases)
        .filter(|c| {
            c.optional_capability.is_none()
                && c.expected != "no-verdict"
                && c.refusal_kind.as_deref() == Some("unsupported-capability")
        })
        .collect();
    assert_eq!(cases.len(), 10);
    let failure = || ValueOutcome::Fails {
        message: "value does not satisfy the contract".into(),
        problems: vec![],
        problems_complete: false,
    };
    let refusal = |reason| ValueOutcome::NoVerdict {
        detail: NoVerdict::new(reason, "schema-dialect", "the dialect is not read"),
    };
    for case in cases {
        let value = JsonValue::parse(&case.value).unwrap();
        let (right, wrong) = match case.expected.as_str() {
            "satisfies" => (ValueOutcome::Satisfies, failure()),
            "fails" => (failure(), ValueOutcome::Satisfies),
            other => panic!("{other}"),
        };
        assert!(judge(case, &value, &right, false).is_empty(), "{}", case.id);
        assert!(
            !judge(case, &value, &wrong, false).is_empty(),
            "{}",
            case.id
        );
        let truthful = refusal(NoVerdictReason::UnsupportedCapability);
        assert!(
            judge(case, &value, &truthful, false).is_empty(),
            "{}",
            case.id
        );
        let other = refusal(NoVerdictReason::ConservativePreparation);
        assert!(!judge(case, &value, &other, true).is_empty(), "{}", case.id);
    }
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
    assert!(judge(case, &value, &ValueOutcome::Satisfies, false).is_empty());
    let failure = ValueOutcome::Fails {
        message: "value does not satisfy the contract".into(),
        problems: vec![],
        problems_complete: false,
    };
    assert!(!judge(case, &value, &failure, false).is_empty());
    let refusal = ValueOutcome::NoVerdict {
        detail: NoVerdict::new(
            NoVerdictReason::UnsupportedCapability,
            "schema-dialect",
            "the dialect is not read",
        ),
    };
    assert!(judge(case, &value, &refusal, false).is_empty());
}

//! Current-spec expectations retain the original admission negatives separately
//! from value cases reexpressed through valid document/resource boundaries.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Historical {
    id: String,
    rule: String,
    document: String,
}
#[test]
fn historical_invalid_documents_remain_admission_negatives() {
    let cases: Vec<Historical> =
        serde_json::from_str(include_str!("fixtures/historical-admission-negatives.json")).unwrap();
    assert_eq!(cases.len(), 13);
    for case in cases {
        let validation = Document::parse(case.document).unwrap().validate().unwrap();
        assert_eq!(
            validation.report().evidence[case.rule.as_str()],
            Evidence::Violated,
            "{}",
            case.id
        );
        assert!(validation.conformant().is_none(), "{}", case.id);
    }
}
#[derive(Deserialize)]
struct Case {
    id: String,
    #[serde(default)]
    document: Option<Value>,
    #[serde(default)]
    schema: Option<Value>,
    #[serde(default)]
    value: Value,
    #[serde(default, rename = "valueText", alias = "value_text")]
    value_text: Option<String>,
    #[serde(default)]
    resources: Vec<Resource>,
    required: String,
    #[serde(default)]
    limits: Limits,
}
#[derive(Deserialize)]
struct Resource {
    uri: String,
    #[serde(default)]
    schema: Value,
    #[serde(default)]
    schema_text: Option<String>,
}
#[test]
fn current_reference_obligations_have_required_semantic_results() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/current-reference-obligations.json")).unwrap();
    assert_eq!(cases.len(), 82);
    check_values(cases);
}
#[test]
fn auxiliary_cases_have_required_semantic_results() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/current-auxiliary-obligations.json")).unwrap();
    assert_eq!(cases.len(), 50);
    check_values(cases);
}
#[test]
fn semantic_review_counterexamples_keep_their_adjudicated_results() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/semantic-soundness.json")).unwrap();
    assert_eq!(cases.len(), 26);
    check_values(cases);
}
#[test]
fn recursive_summaries_preserve_validity_and_evaluated_annotations() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/semantic-recursive-summaries.json")).unwrap();
    assert_eq!(cases.len(), 24);
    check_values(cases);
}
#[test]
fn recursive_extension_meets_independently_adjudicated_results() {
    #[derive(Deserialize)]
    struct Controls {
        cases: Vec<Case>,
    }
    let controls: Controls =
        serde_json::from_str(include_str!("fixtures/semantic-recursive-extension.json")).unwrap();
    assert_eq!(controls.cases.len(), 33);
    check_values(controls.cases);
}
#[test]
fn recursive_coordinates_follow_selected_complete_producers() {
    #[derive(Deserialize)]
    struct Controls {
        cases: Vec<Case>,
    }
    let controls: Controls =
        serde_json::from_str(include_str!("fixtures/semantic-coordinate-refinement.json")).unwrap();
    assert_eq!(controls.cases.len(), 71);
    check_values(controls.cases);
}
#[test]
fn semantic_fallback_honors_construction_and_evaluation_limits() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/semantic-limits.json")).unwrap();
    assert_eq!(cases.len(), 16);
    check_values(cases);
}
fn check_values(cases: Vec<Case>) {
    let mut failures = Vec::new();
    for case in cases {
        let validation = Document::parse(case.document.unwrap_or_else(|| serde_json::json!({"openbindings":"0.2.0","operations":{"op":{"input":case.schema.expect("schema control")}}})).to_string())
            .unwrap()
            .validate()
            .unwrap();
        if let Some(rule) = case.required.strip_prefix("admission:") {
            assert_eq!(
                validation.report().evidence[rule],
                Evidence::Violated,
                "{}",
                case.id
            );
            assert!(validation.conformant().is_none(), "{}", case.id);
            continue;
        }
        let proof = validation
            .conformant()
            .unwrap_or_else(|| panic!("{} admission", case.id));
        let external =
            ExternalSchemas::new(
                case.resources.into_iter().map(|r| ExternalSchema {
                    uri: r.uri,
                    document: JsonValue::parse(
                        r.schema_text.unwrap_or_else(|| r.schema.to_string()),
                    )
                    .unwrap(),
                }),
            )
            .unwrap();
        let schema = proof.resolve_operation("op").unwrap().input().unwrap();
        match schema.prepare(
            &DefaultEvaluator::with_limits(case.limits),
            &external,
            &PreparationLimits::default(),
        ) {
            Ok(prepared) => {
                let value =
                    JsonValue::parse(case.value_text.unwrap_or_else(|| case.value.to_string()))
                        .unwrap();
                for result in [prepared.evaluate(&value), prepared.explain(&value)] {
                    let actual = match &result {
                        ValueOutcome::Satisfies => "S",
                        ValueOutcome::Fails { .. } => "F",
                        ValueOutcome::Undefined { .. } => "U",
                        ValueOutcome::NoVerdict { detail }
                            if detail.reason == NoVerdictReason::ResourceUnavailable =>
                        {
                            "external"
                        }
                        ValueOutcome::NoVerdict { detail }
                            if detail.reason == NoVerdictReason::LimitExceeded =>
                        {
                            "limit"
                        }
                        ValueOutcome::NoVerdict { .. } => "N",
                    };
                    if actual != case.required
                        && !(actual == "external" && case.required == "N:resource-unavailable")
                    {
                        failures.push(format!(
                            "{}: expected {}, got {result:?}",
                            case.id, case.required
                        ));
                    }
                }
            }
            Err(PreparationFailure {
                kind: PreparationFailureKind::Limit { .. },
                ..
            }) if case.required == "limit" => {}
            Err(failure) => failures.push(format!(
                "{}: expected {}, preparation refused {}",
                case.id,
                case.required,
                serde_json::to_string(&failure).unwrap()
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

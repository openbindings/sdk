//! Reusable, finite contract checks for an OpenBindings schema evaluator.
//!
//! Fixtures come from the pinned JSON Schema Test Suite and Go SDK adversarial
//! kit. See `fixtures/provenance.json` for identities and intentional translations.
//! A permitted refusal remains a refusal in the report, never a validity verdict.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
use openbindings::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug, Deserialize)]
/// One frozen evaluator observation. Exact JSON text avoids host numeric conversion; expected outcome and capability exceptions come from packaged provenance.
pub struct Case {
    /// Stable case identity, also used for explicit capability allowances.
    pub id: String,
    /// Exact instance JSON text parsed independently for this case.
    pub value: String,
    /// Expected current outcome tag; frozen fixture spellings are translated at load.
    pub expected: String,
    /// Alternative complete sets of instance pointers accepted for an established failure; absent means no specific set constraint.
    pub acceptable_paths: Option<Vec<Vec<String>>>,
    /// Capability explanation eligible for a case-specific unsupported-capability allowance.
    pub optional_capability: Option<String>,
    /// Human-readable fixture explanation for its refusal expectation.
    pub refusal_reason: String,
    /// Expected no-verdict reason wire spelling, when a refusal is expected.
    pub refusal_kind: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
/// Related cases sharing one original document, operation `op` input contract and supplied resource set.
pub struct Group {
    /// Stable group identity.
    pub id: String,
    /// Exact OpenBindings document JSON containing operation `op`.
    pub document: String,
    /// Keys of explicitly supplied resources in Suite::resources.
    pub resources: Vec<String>,
    /// Finite observations sharing this prepared contract.
    pub cases: Vec<Case>,
}
#[derive(Clone, Debug, Deserialize)]
/// One exact schema resource in the packaged fixture registry; no acquisition is implied.
pub struct Resource {
    /// Absolute resource identity used when building the explicit resource set.
    pub uri: String,
    #[serde(rename = "documentJson")]
    /// Exact resource JSON text; serialized fixture key is `documentJson`.
    pub document: String,
}
#[derive(Clone, Debug, Deserialize)]
/// Frozen finite evaluator contract suite. This is a qualification input, not a claim of complete JSON Schema coverage.
pub struct Suite {
    /// Named resource fixtures available to groups.
    pub resources: BTreeMap<String, Resource>,
    /// All finite groups in the pinned suite.
    pub groups: Vec<Group>,
    /// Embedded meta-schema identities/text used to check original diagnostic locations.
    pub builtin_resources: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct PolicyTranslation {
    case: String,
    from_expected: String,
    from_refusal_kind: Option<String>,
    to_expected: String,
    original_document: String,
    original_value: String,
    original_resources: Vec<Resource>,
}
#[derive(Deserialize)]
struct Provenance {
    runtime_policy_translations: Vec<PolicyTranslation>,
}
fn translate_policy(suite: &mut Suite) {
    let provenance: Provenance = serde_json::from_str(include_str!("../fixtures/provenance.json"))
        .expect("packaged policy provenance");
    for translation in provenance.runtime_policy_translations {
        let group = suite
            .groups
            .iter()
            .find(|group| group.cases.iter().any(|case| case.id == translation.case))
            .expect("policy translation names an existing group");
        assert_eq!(group.document, translation.original_document);
        assert_eq!(group.resources.len(), translation.original_resources.len());
        for (id, original) in group.resources.iter().zip(&translation.original_resources) {
            let actual = &suite.resources[id];
            assert_eq!(actual.uri, original.uri);
            assert_eq!(actual.document, original.document);
        }
        let cases = suite.groups.iter_mut().flat_map(|group| &mut group.cases);
        let mut selected = cases.filter(|case| case.id == translation.case);
        let case = selected
            .next()
            .expect("policy translation names an existing case");
        assert!(selected.next().is_none(), "policy translation is unique");
        assert_eq!(case.value, translation.original_value);
        assert_eq!(case.expected, translation.from_expected);
        assert_eq!(case.refusal_kind, translation.from_refusal_kind);
        assert_eq!(translation.to_expected, "satisfies");
        case.expected = translation.to_expected;
        case.refusal_kind = None;
        case.refusal_reason.clear();
    }
}
/// Frozen inputs. No network access, environment variables, or sibling checkout is needed.
pub fn suite() -> &'static Suite {
    static SUITE: OnceLock<Suite> = OnceLock::new();
    SUITE.get_or_init(|| {
        let mut suite: Suite = serde_json::from_str(include_str!("../fixtures/cases.json"))
            .expect("packaged fixture JSON is verified by build tests");
        // The frozen Go-derived inputs predate the specification-aligned Fails name.
        for case in suite.groups.iter_mut().flat_map(|group| &mut group.cases) {
            if case.expected == "mismatch" {
                case.expected = "fails".into();
            }
        }
        // Preserve the raw historical fixture. Admission changes are explicit
        // provenance shared with the independent browser qualification judge.
        translate_policy(&mut suite);
        suite
    })
}
/// Exact optional case IDs. Unknown, blank and unused declarations fail the run.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Exact optional case IDs mapped to their declared capability explanation. Unknown, mismatched, blank or unused allowances make the report fail.
    pub permitted_refusals: BTreeMap<String, String>,
}
impl Options {
    /// The default companion's qualified Unicode-property limitation, case by case.
    pub fn without_unicode_property_matching() -> Self {
        Self {
            permitted_refusals: suite()
                .groups
                .iter()
                .flat_map(|g| &g.cases)
                .filter_map(|c| {
                    c.optional_capability
                        .as_ref()
                        .map(|reason| (c.id.clone(), reason.clone()))
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
/// Receipt for one case, preserving verdicts and refusals separately from contract-check failures.
pub struct Observation {
    /// Case identity from the fixture.
    pub id: String,
    /// Expected fixture outcome, distinct from the actual observation.
    pub expected: String,
    /// Setup refusal when no prepared owner was produced; outcome is absent in this case.
    pub preparation_refusal: Option<NoVerdict>,
    /// Actual value outcome when preparation succeeded, including evaluation refusal.
    pub outcome: Option<ValueOutcome>,
    /// Contract-check failures for this observation; empty can include an allowed refusal.
    pub failures: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
/// Finite suite receipt. Success permits explicitly declared capability refusals; inspect verdict/refusal counts before claiming coverage.
pub struct Report {
    /// Actual observations in fixture order.
    pub observations: Vec<Observation>,
    /// Invalid or unused allowance declarations and setup invariant failures.
    pub configuration_failures: Vec<String>,
}
impl Report {
    /// Whether configuration is valid, every frozen case was observed and no observation violates the kit contract. Permitted refusals are not counted as verdicts.
    pub fn is_success(&self) -> bool {
        self.configuration_failures.is_empty()
            && self.observations.iter().all(|o| o.failures.is_empty())
            && self.observations.len()
                == suite().groups.iter().map(|g| g.cases.len()).sum::<usize>()
    }
    /// Count established satisfies/fails observations, excluding preparation and evaluation refusals.
    pub fn verdict_count(&self) -> usize {
        self.observations
            .iter()
            .filter(|o| {
                matches!(
                    o.outcome,
                    Some(ValueOutcome::Satisfies | ValueOutcome::Fails { .. })
                )
            })
            .count()
    }
    /// Count preparation or evaluation no-verdict observations, including explicitly permitted capability refusals.
    pub fn refusal_count(&self) -> usize {
        self.observations
            .iter()
            .filter(|o| {
                o.preparation_refusal.is_some()
                    || matches!(o.outcome, Some(ValueOutcome::NoVerdict { .. }))
            })
            .count()
    }
}
/// Apply the kit through the public original-document/context route.
/// Implementations must keep preparation independent between contexts, preserve
/// borrowed input snapshots, and return honest unsupported/limit outcomes.
pub fn run(evaluator: Arc<dyn SchemaEvaluator>, options: &Options) -> Report {
    let suite = suite();
    let mut report = Report {
        observations: Vec::new(),
        configuration_failures: Vec::new(),
    };
    let mut used = BTreeSet::new();
    for (id, reason) in &options.permitted_refusals {
        let case = suite
            .groups
            .iter()
            .flat_map(|g| &g.cases)
            .find(|c| &c.id == id);
        if reason.trim().is_empty()
            || case.is_none_or(|c| c.optional_capability.as_deref() != Some(reason))
        {
            report
                .configuration_failures
                .push(format!("unknown or unsupported refusal declaration: {id}"));
        }
    }
    for group in &suite.groups {
        let document = ParsedDocument::parse(&group.document).expect("packaged document JSON");
        let resources = ResourceSet::new(group.resources.iter().map(|id| {
            let r = &suite.resources[id];
            SchemaResource {
                uri: r.uri.clone(),
                document: JsonValue::parse(&r.document).expect("packaged resource JSON"),
            }
        }))
        .expect("packaged resource set");
        let context = document
            .value_contracts(evaluator.clone(), resources)
            .expect("packaged interpretable document");
        let prepared = context.prepare("op", Side::Input);
        if !matches!(
            prepared,
            ContractPreparation::Ready(_) | ContractPreparation::NoVerdict { .. }
        ) {
            report.configuration_failures.push(format!(
                "{}: expected a schema preparation, observed {prepared:?}",
                group.id
            ));
            continue;
        }
        for case in &group.cases {
            let value = JsonValue::parse(&case.value).expect("packaged instance JSON");
            let original = value.text().to_owned();
            let (outcome, preparation_refusal) = match &prepared {
                ContractPreparation::Ready(contract) => (contract.validate(&value), None),
                ContractPreparation::NoVerdict { detail } => (
                    ValueOutcome::NoVerdict {
                        detail: detail.clone(),
                    },
                    Some(detail.clone()),
                ),
                _ => unreachable!("setup state checked above"),
            };
            let allowed = options
                .permitted_refusals
                .get(&case.id)
                .filter(|reason| case.optional_capability.as_ref() == Some(reason));
            if allowed.is_some() && matches!(outcome, ValueOutcome::NoVerdict { .. }) {
                used.insert(case.id.clone());
            }
            let mut failures = judge(case, &value, &outcome, allowed.is_some());
            if let ContractPreparation::Ready(contract) = &prepared
                && let Some(failure) =
                    judge_resource_declaration(contract.resource_completeness(), &outcome)
            {
                failures.push(failure.into());
            }
            if value.text() != original {
                failures.push("evaluator changed an immutable input".into());
            }
            for problem in match &outcome {
                ValueOutcome::Fails { problems, .. } => problems.as_slice(),
                _ => &[],
            } {
                if let Some(location) = &problem.schema_location {
                    let source = match &location.resource {
                        None => Some(document.value().clone()),
                        Some(uri) => group
                            .resources
                            .iter()
                            .map(|id| &suite.resources[id])
                            .find(|r| &r.uri == uri)
                            .and_then(|r| JsonValue::parse(&r.document).ok())
                            .or_else(|| {
                                suite
                                    .builtin_resources
                                    .get(uri)
                                    .and_then(|text| JsonValue::parse(text).ok())
                            }),
                    };
                    if source.is_none_or(|s| s.at(&location.pointer).is_none()) {
                        failures.push(
                            "reported schema location does not exist in the original source".into(),
                        );
                    }
                    if location
                        .resource
                        .as_deref()
                        .is_some_and(|s| s.starts_with("https://sdk-program.openbindings.invalid/"))
                    {
                        failures
                            .push("generated resource identity leaked through diagnostics".into());
                    }
                }
            }
            report.observations.push(Observation {
                id: case.id.clone(),
                expected: case.expected.clone(),
                outcome: preparation_refusal.is_none().then_some(outcome),
                preparation_refusal,
                failures,
            });
        }
    }
    for id in options.permitted_refusals.keys() {
        if !used.contains(id) {
            report
                .configuration_failures
                .push(format!("unused refusal declaration: {id}"));
        }
    }
    report
}
/// Judge one observation independently. Useful for host harnesses and controls.
pub fn judge(
    case: &Case,
    value: &JsonValue,
    outcome: &ValueOutcome,
    permitted_refusal: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    let tag = match outcome {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    };
    let permitted_refusal = permitted_refusal
        && case.optional_capability.is_some()
        && matches!(outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::UnsupportedCapability);
    if tag != case.expected && !(tag == "no-verdict" && permitted_refusal) {
        failures.push(format!("expected {}, observed {tag}", case.expected));
    }
    if let ValueOutcome::NoVerdict { detail } = outcome {
        let reason = serde_json::to_value(detail.reason).expect("reason serializes");
        if case.refusal_kind.as_deref() != reason.as_str() {
            failures
                .push("refusal reason differs from the fixture's established distinction".into());
        }
        if detail.code.trim().is_empty() || detail.message.trim().is_empty() {
            failures.push("refusal lacks a stable code or useful explanation".into());
        }
        if matches!(
            detail.reason,
            NoVerdictReason::Cancelled
                | NoVerdictReason::EvaluatorFailure
                | NoVerdictReason::LimitExceeded
                | NoVerdictReason::Undefined
        ) {
            failures.push("these finite fixtures do not authorize this refusal reason".into());
        }
    }
    if let ValueOutcome::Fails {
        problems,
        problems_complete,
    } = outcome
    {
        if problems.is_empty() {
            failures.push("failure lacks an actual failing instance location".into());
        }
        if !problems_complete {
            failures
                .push("these small fixture diagnostics unexpectedly exhausted their limit".into());
        }
        let paths: BTreeSet<_> = problems
            .iter()
            .map(|p| p.instance_pointer.clone())
            .collect();
        if problems
            .iter()
            .any(|p| value.at(&p.instance_pointer).is_none())
        {
            failures.push("problem location does not exist in the instance".into());
        }
        if let Some(choices) = &case.acceptable_paths
            && !choices
                .iter()
                .any(|choice| choice.iter().cloned().collect::<BTreeSet<_>>() == paths)
        {
            failures.push(format!("unexpected problem locations: {paths:?}"));
        }
    }
    failures
}

// Only explicit declarations impose these obligations. Undeclared custom evaluators
// keep their existing verdict/refusal contract without inheriting default policy.
fn judge_resource_declaration(
    declaration: ResourceCompleteness<'_>,
    outcome: &ValueOutcome,
) -> Option<&'static str> {
    match declaration {
        ResourceCompleteness::Complete if matches!(outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable) => {
            Some("complete resource declaration returned ResourceUnavailable")
        }
        ResourceCompleteness::Incomplete { evidence, .. }
            if evidence.reason != NoVerdictReason::ResourceUnavailable
                || evidence.location.is_none() =>
        {
            Some("incomplete declaration requires located ResourceUnavailable evidence")
        }
        _ => None,
    }
}
#[cfg(test)]
mod resource_declaration_tests {
    use super::*;
    #[test]
    fn declarations_are_checked_without_assigning_policy_to_undeclared_evaluators() {
        let detail = NoVerdict::new(NoVerdictReason::ResourceUnavailable, "missing", "missing");
        let missing = ValueOutcome::NoVerdict {
            detail: detail.clone(),
        };
        assert!(judge_resource_declaration(ResourceCompleteness::Undeclared, &missing).is_none());
        assert!(judge_resource_declaration(ResourceCompleteness::Complete, &missing).is_some());
        assert!(
            judge_resource_declaration(
                ResourceCompleteness::incomplete(&detail),
                &ValueOutcome::Satisfies
            )
            .is_some()
        );
        let mut located = detail;
        located.location = Some(SchemaLocation {
            resource: None,
            pointer: "/input".into(),
        });
        assert!(
            judge_resource_declaration(
                ResourceCompleteness::incomplete(&located),
                &ValueOutcome::Satisfies
            )
            .is_none()
        );
        assert!(
            judge_resource_declaration(ResourceCompleteness::incomplete(&located), &missing)
                .is_none()
        );
        let mut wrong = NoVerdict::new(NoVerdictReason::Cancelled, "cancelled", "cancelled");
        wrong.location = Some(SchemaLocation {
            resource: None,
            pointer: "/input".into(),
        });
        assert!(
            judge_resource_declaration(ResourceCompleteness::incomplete(&wrong), &missing)
                .is_some()
        );
        assert!(
            judge_resource_declaration(
                ResourceCompleteness::Complete,
                &ValueOutcome::NoVerdict { detail: wrong }
            )
            .is_none()
        );
    }
}

#[cfg(test)]
mod policy_translation_tests {
    use super::*;

    #[test]
    fn policy_translation_requires_exact_historical_inputs() {
        let original: Suite = serde_json::from_str(include_str!("../fixtures/cases.json")).unwrap();
        for field in [
            "id",
            "expected",
            "refusal_kind",
            "value",
            "document",
            "resource_uri",
            "resource_document",
        ] {
            let mut altered = original.clone();
            let group = altered
                .groups
                .iter_mut()
                .find(|g| g.id == "adversarial/resource-uri-names-another-id")
                .unwrap();
            let case = &mut group.cases[0];
            match field {
                "id" => case.id = "changed".into(),
                "expected" => case.expected = "satisfies".into(),
                "refusal_kind" => case.refusal_kind = None,
                "value" => case.value = "2".into(),
                "document" => group.document = "{}".into(),
                "resource_uri" => {
                    altered.resources.get_mut(&group.resources[0]).unwrap().uri =
                        "https://changed.invalid/".into()
                }
                "resource_document" => {
                    altered
                        .resources
                        .get_mut(&group.resources[0])
                        .unwrap()
                        .document = "true".into()
                }
                _ => unreachable!(),
            }
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| translate_policy(
                    &mut altered
                )))
                .is_err(),
                "{field}"
            );
        }
        let group = original
            .groups
            .iter()
            .find(|g| g.id == "adversarial/resource-uri-names-another-id")
            .unwrap();
        assert_eq!(group.cases[0].expected, "no-verdict");
        let translated = suite().groups.iter().find(|g| g.id == group.id).unwrap();
        assert_eq!(translated.cases[0].expected, "satisfies");
        assert!(translated.cases[0].refusal_kind.is_none());
    }
}

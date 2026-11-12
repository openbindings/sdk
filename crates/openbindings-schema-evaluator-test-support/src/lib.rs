#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
use openbindings::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Clone, Deserialize)]
/// One frozen evaluator observation. Exact JSON text avoids host numeric conversion; the reviewed current ledger fixes expected outcomes.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Case {
    /// Stable case identity, retained across historical adjudication.
    pub id: String,
    /// Exact instance JSON text parsed independently for this case.
    pub value: String,
    /// Expected current outcome tag; frozen fixture spellings are translated at load.
    pub expected: String,
    /// Alternative complete sets of instance pointers accepted for an established failure; absent means no specific set constraint.
    pub acceptable_paths: Option<Vec<Vec<String>>>,
}
impl std::fmt::Debug for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Case").finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize)]
/// Related cases sharing one original document, operation `op` input schema and supplied resource set.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Group {
    /// Stable group identity.
    pub id: String,
    /// Exact OpenBindings document JSON containing operation `op`.
    pub document: String,
    /// Keys of explicitly supplied resources in Suite::resources.
    pub resources: Vec<String>,
    /// Finite observations sharing this prepared schema.
    pub cases: Vec<Case>,
}
impl std::fmt::Debug for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Group")
            .field("cases", &self.cases)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize)]
/// One exact schema resource in the packaged fixture registry; no acquisition is implied.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Resource {
    /// Absolute resource identity used when building the explicit resource set.
    pub uri: String,
    #[serde(rename = "documentJson")]
    /// Exact resource JSON text; serialized fixture key is `documentJson`.
    pub document: String,
}
impl std::fmt::Debug for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resource").finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize)]
/// Frozen finite evaluator contract suite. This is a qualification input, not a claim of complete JSON Schema coverage.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Suite {
    /// Named resource fixtures available to groups.
    pub resources: BTreeMap<String, Resource>,
    /// All finite groups in the pinned suite.
    pub groups: Vec<Group>,
    /// Embedded meta-schema identities/text used to check original diagnostic locations.
    pub builtin_resources: BTreeMap<String, String>,
}
impl std::fmt::Debug for Suite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Suite")
            .field("groups", &self.groups)
            .finish_non_exhaustive()
    }
}

/// Current adjudicated inputs. Historical cases and their dispositions are
/// packaged alongside them; no network or sibling checkout is needed.
pub fn suite() -> &'static Suite {
    static SUITE: OnceLock<Suite> = OnceLock::new();
    SUITE.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/current-suite.json"))
            .expect("packaged current fixture JSON")
    })
}

#[derive(Deserialize)]
struct SuppliedGroup {
    uri: String,
    #[serde(flatten)]
    group: Group,
}
#[derive(Deserialize)]
struct SuppliedSuite {
    resources: BTreeMap<String, Resource>,
    groups: Vec<SuppliedGroup>,
}
fn supplied_suite() -> &'static SuppliedSuite {
    static SUITE: OnceLock<SuppliedSuite> = OnceLock::new();
    SUITE.get_or_init(|| {
        serde_json::from_str(include_str!("../fixtures/supplied-roots.json"))
            .expect("packaged supplied-root fixture JSON")
    })
}
#[derive(Clone, Serialize)]
/// Receipt for one case, preserving verdicts and refusals separately from contract-check failures.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct Observation {
    /// Case identity from the fixture.
    pub id: String,
    /// Expected fixture outcome, distinct from the actual observation.
    pub expected: String,
    /// Setup refusal when no prepared owner was produced; both outcomes are absent in this case.
    pub preparation_refusal: Option<PreparationFailure>,
    /// Actual cheap-check outcome when preparation succeeded, including evaluation refusal.
    pub outcome: Option<ValueOutcome>,
    /// Actual explanation outcome for the same value when preparation succeeded.
    pub explanation: Option<ValueOutcome>,
    /// Contract-check failures; empty means the expected semantic answer or
    /// explicit missing-root preparation refusal was established.
    pub failures: Vec<String>,
}
impl std::fmt::Debug for Observation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Observation")
            .field("preparation_refusal", &self.preparation_refusal)
            .field("outcome", &self.outcome)
            .field("explanation", &self.explanation)
            .field("failure_count", &self.failures.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Serialize)]
/// Finite suite receipt. A refusal remains a qualification gap when a case expects a semantic answer.
///
/// Only [`run`] builds reports. Later versions may add fields: read them by name
/// and match with `..`.
/// ```compile_fail,E0638
/// use openbindings_schema_evaluator_test_support::Report;
/// fn read(report: &Report) {
///     let Report { observations, configuration_failures } = report;
/// }
/// ```
/// ```
/// use openbindings_schema_evaluator_test_support::Report;
/// fn read(report: &Report) {
///     let Report { observations, configuration_failures, .. } = report;
/// }
/// ```
#[non_exhaustive]
pub struct Report {
    /// Actual observations in fixture order.
    pub observations: Vec<Observation>,
    /// Setup invariant failures and invalid fixture configuration.
    pub configuration_failures: Vec<String>,
}
impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Report")
            .field("observations", &self.observations)
            .field(
                "configuration_failure_count",
                &self.configuration_failures.len(),
            )
            .finish_non_exhaustive()
    }
}

impl Report {
    /// Whether configuration is valid, every evaluator case was observed and no
    /// observation violates the kit contract. A refusal remains a gap when the
    /// case requires a semantic answer. Missing-root cases require the specific
    /// preparation refusal instead.
    pub fn is_success(&self) -> bool {
        self.configuration_failures.is_empty()
            && self.observations.iter().all(|o| o.failures.is_empty())
            && self.observations.len()
                == suite().groups.iter().map(|g| g.cases.len()).sum::<usize>()
                    + supplied_suite()
                        .groups
                        .iter()
                        .map(|g| g.group.cases.len())
                        .sum::<usize>()
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
    /// Count preparation or evaluation no-verdict observations, without treating either as a semantic verdict.
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
/// Apply the kit through public contained and supplied-root preparation routes.
/// Each group's schema is prepared once with the borrowed evaluator and its
/// supplied resources, then every value is both checked and explained. A missing
/// supplied URI must return a `ResourceUnavailable` preparation failure.
/// Implementations
/// must keep preparations independent, preserve borrowed input snapshots, and
/// return honest unsupported/limit outcomes. Both calls must reach the same
/// verdict; a cheap check reports no problems and never claims completeness.
/// Every failing value is also explained under cancellation after successively
/// more control observations (see [`judge_interruptions`]), and again as a
/// member of a larger text, which must not change its problems: instance
/// pointers start at the value passed.
pub fn run<E: SchemaEvaluator + ?Sized>(evaluator: &E) -> Report {
    let suite = suite();
    let supplied = supplied_suite();
    let mut report = Report {
        observations: Vec::new(),
        configuration_failures: Vec::new(),
    };
    let groups = suite
        .groups
        .iter()
        .map(|group| (group, &suite.resources, None))
        .chain(
            supplied
                .groups
                .iter()
                .map(|entry| (&entry.group, &supplied.resources, Some(entry.uri.as_str()))),
        );
    for (group, registry, uri) in groups {
        let document = Document::parse(&group.document).expect("packaged document JSON");
        let resources = ExternalSchemas::new(group.resources.iter().map(|id| {
            let r = &registry[id];
            ExternalSchema {
                uri: r.uri.clone(),
                document: JsonValue::parse(&r.document).expect("packaged resource JSON"),
            }
        }))
        .expect("packaged resource set");
        let validated = document
            .validate()
            .expect("supported fixture version")
            .conformant()
            .expect("conformant fixture document");
        let prepared = if let Some(uri) = uri {
            resources.prepare(uri, evaluator)
        } else {
            let operation = validated.resolve_operation("op");
            let Some(input) = operation.as_ref().and_then(|operation| operation.input()) else {
                report.configuration_failures.push(format!(
                    "{}: expected an input schema on operation op",
                    group.id
                ));
                continue;
            };
            input.prepare(evaluator, &resources)
        };
        for case in &group.cases {
            if case.expected == "resource-unavailable" {
                let expected = matches!(&prepared, Err(failure) if matches!(
                    &failure.kind,
                    PreparationFailureKind::Resolution {
                        cause: ResolutionFailureKind::ResourceUnavailable { uri: missing }
                    } if Some(missing.as_str()) == uri
                ));
                report.observations.push(Observation {
                    id: case.id.clone(),
                    expected: case.expected.clone(),
                    outcome: None,
                    explanation: None,
                    preparation_refusal: prepared.as_ref().err().cloned(),
                    failures: if expected { Vec::new() } else {
                        vec!["missing supplied root did not produce the required preparation refusal".into()]
                    },
                });
                continue;
            }
            let value = JsonValue::parse(&case.value).expect("packaged instance JSON");
            let original = value.text().to_owned();
            let contract = match &prepared {
                Ok(contract) => contract,
                Err(failure) => {
                    report.observations.push(Observation {
                        id: case.id.clone(),
                        expected: case.expected.clone(),
                        outcome: None,
                        explanation: None,
                        preparation_refusal: Some(failure.clone()),
                        failures: vec![
                            "preparation failed; the required semantic answer was not established"
                                .into(),
                        ],
                    });
                    continue;
                }
            };
            let outcome = contract.evaluate(&value);
            let explanation = contract.explain(&value);
            let mut failures = judge(case, &value, &outcome);
            for failure in judge(case, &value, &explanation) {
                if !failures.contains(&failure) {
                    failures.push(failure);
                }
            }
            failures.extend(judge_detail(&outcome, &explanation));
            if let (Ok(contract), ValueOutcome::Fails { .. }) = (&prepared, &explanation) {
                failures.extend(judge_interruptions(
                    &explanation,
                    &interrupted(contract, &value),
                ));
                // Instance pointers start at the value passed, also when it is
                // part of a larger text.
                let wrapper = JsonValue::parse(format!("{{\"value\":{}}}", case.value))
                    .expect("packaged instance JSON");
                let nested = contract.explain(&wrapper.get("value").expect("wrapped").to_owned());
                if serde_json::to_string(&nested).ok() != serde_json::to_string(&explanation).ok() {
                    failures
                        .push("an explanation of the value within a larger text differs".into());
                }
            }
            if value.text() != original {
                failures.push("evaluator changed an immutable input".into());
            }
            let locations: Vec<_> = [&outcome, &explanation]
                .into_iter()
                .flat_map(|outcome| match outcome {
                    ValueOutcome::Fails { problems, .. } => {
                        problems.iter().map(|p| &p.schema).collect()
                    }
                    ValueOutcome::Undefined { detail } => detail.location.iter().collect(),
                    _ => Vec::new(),
                })
                .collect();
            for location in locations {
                // The document and supplied resources locate their own nodes;
                // a packaged meta-schema is read from its text.
                let exists = match &location.source_uri {
                    None => validated.locate(location).is_some(),
                    Some(uri) => {
                        resources.locate(location).is_some()
                            || suite
                                .builtin_resources
                                .get(uri)
                                .and_then(|text| JsonValue::parse(text).ok())
                                .is_some_and(|source| {
                                    location
                                        .pointer
                                        .as_str()
                                        .is_some_and(|pointer| source.at(pointer).is_some())
                                })
                    }
                };
                if !exists {
                    failures.push(
                        "reported schema location does not exist in the original source".into(),
                    );
                }
                if location
                    .source_uri
                    .as_deref()
                    .is_some_and(|s| s.starts_with("https://sdk-program.openbindings.invalid/"))
                {
                    failures.push("generated resource identity leaked through diagnostics".into());
                }
            }
            report.observations.push(Observation {
                id: case.id.clone(),
                expected: case.expected.clone(),
                outcome: Some(outcome),
                explanation: Some(explanation),
                preparation_refusal: None,
                failures,
            });
        }
    }
    report
}
/// Judge how a cheap check and an explanation of the same value relate: they
/// reach the same verdict, and the cheap check reports no problems and does not
/// claim complete diagnostics. Useful for host harnesses and controls.
pub fn judge_detail(outcome: &ValueOutcome, explanation: &ValueOutcome) -> Vec<String> {
    let tag = |outcome: &ValueOutcome| std::mem::discriminant(outcome);
    let mut failures = Vec::new();
    if tag(outcome) != tag(explanation) {
        failures.push("the cheap check and the explanation reach different verdicts".into());
    }
    if let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = outcome
        && (!problems.is_empty() || *problems_complete)
    {
        failures.push("a cheap check reported failure diagnostics".into());
    }
    failures
}
// A control that cancels after a number of observations, counting them all.
struct StopAfter(AtomicUsize, usize);
impl WorkControl for StopAfter {
    fn is_cancelled(&self) -> bool {
        self.0.fetch_add(1, Ordering::Relaxed) >= self.1
    }
}
// Explanations of one value cancelled after 0, 1, 2, 3, 5, 8 ... observations,
// and finally after every observation an uninterrupted explanation makes.
fn interrupted<V: SchemaValidator>(
    contract: &PreparedSchema<V>,
    value: &JsonValue,
) -> Vec<(usize, ValueOutcome)> {
    let all = StopAfter(AtomicUsize::new(0), usize::MAX);
    let _ = contract.explain_with_control(value, &all);
    let total = all.0.load(Ordering::Relaxed);
    let (mut previous, mut stop) = (1, 0);
    let mut stops = Vec::new();
    while stop < total {
        stops.push(stop);
        (previous, stop) = (stop, stop + previous);
    }
    stops.push(total);
    stops
        .into_iter()
        .map(|stop| {
            let control = StopAfter(AtomicUsize::new(0), stop);
            (stop, contract.explain_with_control(value, &control))
        })
        .collect()
}
/// Judge explanations of one failing value cancelled after increasing numbers of
/// control observations, given as `(observations allowed, outcome)` in that
/// order, the last allowing every observation an uninterrupted explanation
/// makes. Before its verdict an evaluator may answer no verdict. An interrupted
/// explanation never reaches another verdict; once one answers fails, every
/// later one does, since cutting detail work short never undoes an established
/// failure; and the last reproduces the uninterrupted explanation.
pub fn judge_interruptions(
    explanation: &ValueOutcome,
    interrupted: &[(usize, ValueOutcome)],
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut established = false;
    for (_, outcome) in interrupted {
        match outcome {
            ValueOutcome::Fails { .. } => established = true,
            ValueOutcome::NoVerdict { .. } if !established => {}
            ValueOutcome::NoVerdict { .. } => failures.push(
                "an interrupted explanation turned an established failure into no verdict"
                    .to_owned(),
            ),
            _ => failures.push("an interrupted explanation reached another verdict".to_owned()),
        }
    }
    // Outcomes compare by their serialized text, which carries every field,
    // including pointers through names with unpaired units.
    let same = |a: &ValueOutcome, b: &ValueOutcome| {
        serde_json::to_string(a)
            .ok()
            .is_some_and(|a| serde_json::to_string(b).ok() == Some(a))
    };
    if interrupted
        .last()
        .is_some_and(|(_, last)| !same(last, explanation))
    {
        failures.push("an explanation allowed every observation differs from the first".into());
    }
    failures.dedup();
    failures
}
/// Judge one observation independently. Useful for host harnesses and controls.
/// Each problem's instance pointer must name a node of `value`, from `value`
/// itself.
pub fn judge(case: &Case, value: &JsonValue, outcome: &ValueOutcome) -> Vec<String> {
    let mut failures = Vec::new();
    let tag = match outcome {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no-verdict",
    };
    if tag != case.expected {
        failures.push(format!("expected {}, observed {tag}", case.expected));
    }
    if let ValueOutcome::NoVerdict { detail } = outcome {
        if detail.code.trim().is_empty() || detail.message.trim().is_empty() {
            failures.push("refusal lacks a stable code or useful explanation".into());
        }
        if matches!(
            detail.reason,
            NoVerdictReason::Cancelled
                | NoVerdictReason::EvaluatorFailure
                | NoVerdictReason::LimitExceeded
        ) {
            failures.push("these finite fixtures do not authorize this refusal reason".into());
        }
    }
    if let ValueOutcome::Undefined { detail } = outcome
        && (detail.code.trim().is_empty() || detail.message.trim().is_empty())
    {
        failures.push("undefined result lacks a stable code or useful explanation".into());
    }
    if let ValueOutcome::Fails {
        problems,
        problems_complete,
        ..
    } = outcome
    {
        if *problems_complete && problems.is_empty() {
            failures.push(
                "complete failure diagnostics lack an actual failing instance location".into(),
            );
        }
        let paths: BTreeSet<_> = problems.iter().map(|p| p.instance.to_string()).collect();
        if problems
            .iter()
            .any(|p| value.at_pointer(&p.instance).is_none())
        {
            failures.push("problem location does not exist in the instance".into());
        }
        if *problems_complete
            && let Some(choices) = &case.acceptable_paths
            && !choices
                .iter()
                .any(|choice| choice.iter().cloned().collect::<BTreeSet<_>>() == paths)
        {
            failures.push(format!("unexpected problem locations: {paths:?}"));
        }
    }
    failures
}

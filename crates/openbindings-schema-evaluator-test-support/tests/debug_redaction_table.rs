//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::*;
use openbindings_schema_evaluator_test_support as kit;
use std::collections::BTreeMap;
use std::fmt::Debug;
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}
fn location() -> SchemaOrigin {
    SchemaOrigin::new(Some(SECRET.into()), format!("/{SECRET}").parse().unwrap())
}
/// Refuses every preparation, so the kit records a receipt for every case.
struct Refuse;
impl SchemaEvaluator for Refuse {
    type Validator = Refuse;
    fn prepare(
        &self,
        _: EvaluationRequest<'_>,
        _: &PreparationLimits,
        _: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        Err(preparation_failure())
    }
}
// The kit builds its suite and receipts; a caller receives them and can
// reassign their public fields, so these probes fill actual records with
// caller text.
fn kit_rows(outcome: &ValueOutcome) -> Vec<Row> {
    let mut suite = kit::suite().clone();
    let mut group = suite.groups[0].clone();
    let mut case = group.cases[0].clone();
    case.id = SECRET.into();
    case.value = SECRET.into();
    case.expected = SECRET.into();
    case.acceptable_paths = Some(vec![vec![SECRET.into()]]);
    group.id = SECRET.into();
    group.document = SECRET.into();
    group.resources = vec![SECRET.into()];
    group.cases = vec![case.clone()];
    let mut resource = suite.resources.values().next().unwrap().clone();
    resource.uri = SECRET.into();
    resource.document = SECRET.into();
    suite.resources = BTreeMap::from([(SECRET.into(), resource.clone())]);
    suite.groups = vec![group.clone()];
    suite.builtin_resources = BTreeMap::from([(SECRET.into(), SECRET.into())]);
    let mut report = kit::run(&Refuse);
    let mut observation = report.observations[0].clone();
    observation.id = SECRET.into();
    observation.expected = SECRET.into();
    observation.preparation_refusal = Some(preparation_failure());
    observation.outcome = Some(outcome.clone());
    observation.explanation = Some(outcome.clone());
    observation.failures = vec![SECRET.into()];
    report.observations = vec![observation.clone()];
    report.configuration_failures = vec![SECRET.into()];
    vec![
        row("kit Case", &case),
        row("kit Group", &group),
        row("kit Resource", &resource),
        row("kit Suite", &suite),
        row("kit Observation", &observation),
        row("kit Report", &report),
    ]
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let problem = ValueProblem::new(
        format!("/{SECRET}").parse().unwrap(),
        location(),
        SECRET,
        SECRET,
    )
    .with_details(ValueProblemDetails::Required {
        member: SECRET.into(),
    });
    let outcome = ValueOutcome::Fails {
        message: SECRET.into(),
        problems: vec![problem.clone()],
        problems_complete: true,
    };
    let rows = kit_rows(&outcome);

    check_rows(rows, 6);
}

fn preparation_failure() -> PreparationFailure {
    PreparationFailure::new(PreparationFailureKind::Implementation {
        message: SECRET.into(),
    })
    .with_origin(location())
}
impl SchemaValidator for Refuse {
    fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
        unreachable!()
    }
}

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
fn location() -> SchemaLocation {
    SchemaLocation {
        resource: Some(SECRET.into()),
        pointer: format!("/{SECRET}").parse().unwrap(),
    }
}
fn refusal() -> NoVerdict {
    NoVerdict {
        reason: NoVerdictReason::EvaluatorFailure,
        code: SECRET.into(),
        message: format!("provider refused schema operand {SECRET}"),
        location: Some(location()),
    }
}
fn kit_rows(outcome: &ValueOutcome) -> Vec<Row> {
    let case = kit::Case {
        id: SECRET.into(),
        value: SECRET.into(),
        expected: SECRET.into(),
        acceptable_paths: Some(vec![vec![SECRET.into()]]),
        optional_capability: Some(SECRET.into()),
        refusal_reason: SECRET.into(),
        refusal_kind: Some(SECRET.into()),
    };
    let group = kit::Group {
        id: SECRET.into(),
        document: SECRET.into(),
        resources: vec![SECRET.into()],
        cases: vec![case.clone()],
    };
    let resource = kit::Resource {
        uri: SECRET.into(),
        document: SECRET.into(),
    };
    let observation = kit::Observation {
        id: SECRET.into(),
        expected: SECRET.into(),
        preparation_refusal: Some(refusal()),
        outcome: Some(outcome.clone()),
        explanation: Some(outcome.clone()),
        failures: vec![SECRET.into()],
    };
    vec![
        row("kit Case", &case),
        row("kit Group", &group),
        row("kit Resource", &resource),
        row(
            "kit Suite",
            &kit::Suite {
                resources: BTreeMap::from([(SECRET.into(), resource)]),
                groups: vec![group],
                builtin_resources: BTreeMap::from([(SECRET.into(), SECRET.into())]),
            },
        ),
        row(
            "kit Options",
            &kit::Options {
                permitted_refusals: BTreeMap::from([(SECRET.into(), SECRET.into())]),
            },
        ),
        row("kit Observation", &observation),
        row(
            "kit Report",
            &kit::Report {
                observations: vec![observation],
                configuration_failures: vec![SECRET.into()],
            },
        ),
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
    let problem = ValueProblem {
        instance: SourceLocation {
            pointer: Some(format!("/{SECRET}").parse().unwrap()),
            byte_offset: 42,
            line: 2,
            byte_column: 3,
        },
        schema: location(),
        code: SECRET.into(),
        message: SECRET.into(),
        details: Some(ValueProblemDetails::Required {
            member: SECRET.into(),
        }),
    };
    let outcome = ValueOutcome::Fails {
        message: SECRET.into(),
        problems: vec![problem.clone()],
        problems_complete: true,
    };
    let rows = kit_rows(&outcome);

    check_rows(rows, 7);
}

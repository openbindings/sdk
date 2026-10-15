//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings_internal_schema::{self as schema, JsonValue};
use std::fmt::Debug;
use std::{collections::BTreeMap, sync::Arc};
// Keep the independent review's exact credential and pointer probe value.
const SECRET: &str = "REVIEW-BEARER-TOKEN-7391";
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
fn internal_rows(context: &JsonValue) -> Vec<Row> {
    let location = schema::Location {
        resource: Some(SECRET.into()),
        pointer: format!("/{SECRET}"),
    };
    let failure = schema::Failure {
        reason: schema::FailureReason::EvaluatorFailure,
        code: SECRET.into(),
        message: SECRET.into(),
        location: Some(location.clone()),
        reference_defect: Some(schema::StaticReferenceDefect::MalformedReference),
    };
    let reference = schema::Reference {
        location: location.clone(),
        keyword: SECRET.into(),
        spelling: Some(SECRET.into()),
        resolution: schema::ReferenceResolution::Unresolved {
            detail: failure.clone(),
        },
    };
    let problem = schema::fixed_schema::Problem {
        at: context.view(),
        message: SECRET.into(),
    };
    vec![
        row("internal Location", &location),
        row("internal Failure", &failure),
        row(
            "internal Target",
            &schema::Target {
                node: 0,
                dynamic_name: Some(SECRET.into()),
            },
        ),
        row("internal Reference", &reference),
        row(
            "internal ReferenceReport",
            &schema::ReferenceReport {
                references: vec![reference],
                complete: false,
                limitation: Some(failure),
            },
        ),
        row(
            "internal ProgramOrigin",
            &schema::ProgramOrigin::Exact(location.clone()),
        ),
        row(
            "internal Program",
            &schema::Program {
                entry_uri: SECRET.into(),
                resources: vec![schema::ProjectedResource {
                    uri: SECRET.into(),
                    document: context.clone(),
                }],
                locations: Arc::new(BTreeMap::from([(
                    SECRET.into(),
                    schema::ProgramOrigin::Prefix(location),
                )])),
            },
        ),
        row("internal Problem", &problem),
        row(
            "internal Problems",
            &schema::fixed_schema::Problems {
                entries: vec![problem],
                violated: true,
                truncated: false,
            },
        ),
        row(
            "internal RawDocument",
            &schema::RawDocument::new(context.clone()),
        ),
        row(
            "internal ProjectedResource",
            &schema::ProjectedResource {
                uri: SECRET.into(),
                document: context.clone(),
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
    let context = JsonValue::parse(
        serde_json::json!({"authorization":format!("Bearer {SECRET}"), SECRET: SECRET}).to_string(),
    )
    .unwrap();
    let rows = internal_rows(&context);

    check_rows(rows, 11);
}

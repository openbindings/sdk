//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::{Example, JsonValue, Operation};
use openbindings_source_inspection as inspection;
use std::collections::BTreeMap;
use std::fmt::Debug;
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
fn evidence() -> BTreeMap<String, JsonValue> {
    BTreeMap::from([(
        format!("credential-{SECRET}"),
        JsonValue::string(SECRET).unwrap(),
    )])
}
fn inspection_rows(context: &JsonValue, operation: &Operation) -> Vec<Row> {
    let error = inspection::InspectionError::new(
        inspection::InspectionErrorKind::ProviderFailure,
        SECRET,
        format!("provider refused schema operand {SECRET}"),
    );
    assert_eq!(error.code(), SECRET);
    assert!(error.to_string().contains(SECRET));
    assert!(format!("{error:?}").contains("ProviderFailure"));
    let fragment = inspection::BindingFragment {
        content: Some(context.clone()),
        description: Some(SECRET.into()),
        extensions: BTreeMap::from([(format!("x-{SECRET}"), context.clone())]),
        ..Default::default()
    };
    let target = inspection::BindableTarget {
        source_ref: SECRET.into(),
        binding: fragment.clone(),
        operation_key: Some(SECRET.into()),
        operation: Some(operation.clone()),
    };
    let limitation = inspection::InspectionLimitation {
        code: SECRET.into(),
        message: SECRET.into(),
        details: Some(evidence()),
        additional_fields: evidence(),
    };
    assert!(serde_json::to_string(&limitation).unwrap().contains(SECRET));
    vec![
        row("InspectionError", &error),
        row(
            "inspection SupportedKind",
            &inspection::SupportedKind {
                kind: SECRET.into(),
                description: Some(SECRET.into()),
            },
        ),
        row(
            "inspection KindSupportVerdict",
            &inspection::KindSupportVerdict {
                kind: SECRET.into(),
                supported: true,
            },
        ),
        row("BindingFragment", &fragment),
        row("BindableTarget", &target),
        row("InspectionLimitation", &limitation),
        row(
            "SourceInspection",
            &inspection::SourceInspection::partial(vec![target], limitation),
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
    let operation = Operation {
        description: Some(SECRET.into()),
        tags: Some(vec![SECRET.into()]),
        aliases: Some(vec![SECRET.into()]),
        input: Some(context.clone()),
        output: Some(context.clone()),
        examples: Some(BTreeMap::from([(
            SECRET.into(),
            Example {
                description: Some(SECRET.into()),
                input: Some(context.clone()),
                output: Some(context.clone()),
                additional_fields: evidence(),
            },
        )])),
        additional_fields: evidence(),
        ..Default::default()
    };
    let rows = inspection_rows(&context, &operation);

    check_rows(rows, 7);
}

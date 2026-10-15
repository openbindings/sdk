//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings_internal_json::*;
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
    let source_location = SourceLocation {
        pointer: Some(format!("/{SECRET}")),
        byte_offset: 42,
        line: 2,
        byte_column: 3,
    };
    let rows = vec![
        row("JsonValue", &context),
        row("JsonRef", &context.view()),
        row(
            "JsonMember",
            &context.view().members().unwrap().next().unwrap(),
        ),
        row(
            "JsonStringRef",
            &context.view().get(SECRET).unwrap().as_string().unwrap(),
        ),
        row("SourceLocation", &source_location),
        row(
            "InputError",
            &InputError {
                kind: InputErrorKind::Syntax,
                byte_offset: 42,
                code: SECRET,
            },
        ),
        row(
            "Decimal",
            &openbindings_internal_json::decimal::Decimal::parse(NUMBER),
        ),
    ];

    check_rows(rows, 7);
}

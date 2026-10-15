//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings::{JsonValue, Source};
use openbindings_invocation as invocation;
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
fn invocation_rows(source: &Source, context: &JsonValue) -> Vec<Row> {
    let binding = invocation::BindingValue {
        content: Some(context.clone()),
        ..Default::default()
    };
    let input = invocation::BindingInvocationInput {
        source: source.clone(),
        binding: binding.clone(),
        context: Some(context.clone()),
    };
    let error = invocation::InvocationError::new(SECRET, Some(context.clone())).unwrap();
    assert_eq!(error.code(), SECRET);
    assert!(error.to_string().contains(SECRET));
    assert!(error.to_json().unwrap().text().contains(SECRET));
    vec![
        row("BindingValue", &binding),
        row("BindingRequest", &input.request()),
        row("BindingInvocationInput", &input),
        row(
            "operation Request",
            &invocation::operation::Request::operation(SECRET).with_context(context),
        ),
        row("InvocationError", &error),
        row(
            "invocation SupportedKind",
            &invocation::binding::SupportedKind {
                kind: SECRET.into(),
                description: Some(SECRET.into()),
            },
        ),
        row(
            "invocation KindSupportVerdict",
            &invocation::binding::KindSupportVerdict {
                kind: SECRET.into(),
                supported: true,
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
    let source = Source {
        kind: SECRET.into(),
        content: Some(context.clone()),
        description: Some(SECRET.into()),
        additional_fields: BTreeMap::from([(format!("x-{SECRET}"), context.clone())]),
    };
    let rows = invocation_rows(&source, &context);

    check_rows(rows, 7);
}

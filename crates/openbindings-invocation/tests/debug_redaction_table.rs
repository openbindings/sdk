//! Debug remains structural across current carriers in four rendering modes.
use openbindings::{Document, JsonValue, PreparationFailure, PreparationFailureKind};
use openbindings_invocation::{
    BindingRequest, Context, ContextChallenge, InvocationError, OperationAddress, OperationRequest,
    operation::{BindingChoice, OperationPreparationFailure, ValidationEvidence},
};
use std::fmt::Debug;
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
    let value = JsonValue::parse(serde_json::json!({"authorization": format!("Bearer {SECRET}"), SECRET: SECRET, "number":739173917391_u64}).to_string()).unwrap();
    let context = Context::try_from(value.clone()).unwrap();
    let interface = Document::parse(serde_json::json!({"openbindings":"0.2.0", "operations":{SECRET:{}},
        "sources":{"s":{"kind":SECRET,"data":NUMBER}}, "bindings":{"b":{"operation":SECRET,"source":"s"}}}).to_string()).unwrap().validate().unwrap().conformant().unwrap();
    let binding = BindingRequest::new(interface.clone(), SECRET).with_context(context.clone());
    let operation = OperationRequest::operation(interface, SECRET).with_context(context.clone());
    let challenge = ContextChallenge::try_from(
        JsonValue::parse(
            serde_json::json!({"message":SECRET,"schema":{"title":SECRET}}).to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let error = InvocationError::new(SECRET, Some(value.clone())).unwrap();
    assert_eq!(error.code(), SECRET);
    assert!(!error.to_string().contains(SECRET));
    let admission = BindingRequest::from_json(&value).unwrap_err();
    let evidence = ValidationEvidence::Preparation(PreparationFailure::new(
        PreparationFailureKind::Implementation {
            message: SECRET.into(),
        },
    ));
    let rows = vec![
        row("BindingRequest", &binding),
        row("OperationRequest", &operation),
        row(
            "OperationAddress",
            &OperationAddress::Binding(SECRET.into()),
        ),
        row("Context", &context),
        row("ContextChallenge", &challenge),
        row("InvocationError", &error),
        row("BoundaryFailure", &admission),
        row("ValidationEvidence", &evidence),
        row("BindingChoice", &BindingChoice::Binding(SECRET.into())),
        row(
            "OperationPreparationFailure",
            &OperationPreparationFailure::Resolution(error.clone()),
        ),
    ];
    check_rows(rows, 10);
}

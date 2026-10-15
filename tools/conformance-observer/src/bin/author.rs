use openbindings::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut draft = Document::new();
    draft.set_operation(
        "lookup",
        &Operation {
            input: Some(JsonValue::parse(r#"{"type":"integer"}"#)?),
            ..Default::default()
        },
    )?;
    draft.set_source(
        "local",
        &Source {
            kind: "example.native@1".into(),
            content: Some(JsonValue::null()),
            ..Default::default()
        },
    )?;
    draft.set_binding(
        "call",
        &Binding {
            operation: "lookup".into(),
            source: "local".into(),
            ..Default::default()
        },
    )?;
    let snapshot = draft.clone();
    let checked = snapshot.validate()?;
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    let valid = checked.conformant().unwrap();
    drop(draft);
    assert_eq!(
        valid.as_document().operation_bindings("lookup")?,
        vec!["call"]
    );
    println!("external typed authoring, exact content, validation and immutable snapshot passed");
    Ok(())
}

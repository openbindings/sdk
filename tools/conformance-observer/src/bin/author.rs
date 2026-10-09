use openbindings::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut draft = DocumentBuilder::new();
    draft.operations.insert(
        "lookup".into(),
        Operation {
            input: Some(JsonValue::parse(r#"{"type":"integer"}"#)?),
            ..Default::default()
        },
    );
    draft.sources = Some(
        [(
            "local".into(),
            Source {
                kind: "example.native@1".into(),
                content: Some(JsonValue::null()),
                ..Default::default()
            },
        )]
        .into(),
    );
    draft.bindings = Some(
        [(
            "call".into(),
            Binding {
                operation: "lookup".into(),
                source: "local".into(),
                ..Default::default()
            },
        )]
        .into(),
    );
    let snapshot = draft.build()?;
    let checked = snapshot.assess()?;
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    let valid = checked.validated().unwrap();
    drop(draft);
    assert_eq!(valid.parsed().operation_bindings("lookup")?, vec!["call"]);
    println!("external typed authoring, exact content, assessment and immutable snapshot passed");
    Ok(())
}

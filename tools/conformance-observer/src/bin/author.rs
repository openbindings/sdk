use openbindings::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let draft = Document::new().edit(&[
        Edit::set(
            &["operations"],
            JsonValue::parse(r#"{"lookup":{"input":{"type":"integer"}}}"#)?,
        ),
        Edit::set(
            &["sources"],
            JsonValue::parse(r#"{"local":{"kind":"example.native@1","data":null}}"#)?,
        ),
        Edit::set(
            &["bindings"],
            JsonValue::parse(r#"{"call":{"operation":"lookup","source":"local"}}"#)?,
        ),
    ])?;
    let snapshot = draft.clone();
    let checked = snapshot.validate()?;
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    let valid = checked.conformant().unwrap();
    drop(draft);
    let operation = valid.operation("lookup").unwrap();
    assert_eq!(
        operation
            .bindings()
            .iter()
            .map(Binding::key)
            .collect::<Vec<_>>(),
        vec!["call"]
    );
    println!(
        "external exact authoring, canonical entries, validation and immutable snapshot passed"
    );
    Ok(())
}

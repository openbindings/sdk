//! Navigation and the two schema placement routes described in the guide.
use openbindings::{
    Document, JsonMember, JsonRef, JsonValue, ReferenceReport, ReferenceResolution,
};

#[test]
fn navigate_exact_json_and_retain_a_subtree() {
    let data =
        JsonValue::parse(r#"{"items":[{"n":123456789012345678901234567890}],"text":"\uD800"}"#)
            .unwrap();
    let root: JsonRef<'_> = data.view();
    let items = root.get("items").unwrap();
    let item = items.elements().unwrap().next().unwrap();
    let JsonMember { name, value } = item.members().unwrap().next().unwrap();
    assert_eq!(name.as_str(), Some("n"));
    assert_eq!(value.number_text(), Some("123456789012345678901234567890"));
    assert_eq!(root.at("/items/0/n").unwrap().text(), value.text());
    assert!(root.get("text").unwrap().as_str().is_none());
    assert_eq!(
        root.get("text")
            .unwrap()
            .as_string()
            .unwrap()
            .utf16_units()
            .collect::<Vec<_>>(),
        [0xD800]
    );

    let owned = item.to_owned(); // Retains the entire arena, not just this object.
    assert!(std::ptr::eq(
        owned.text().as_bytes(),
        item.text().as_bytes()
    ));
    drop(data);
    assert_eq!(
        owned.get("n").unwrap().number_text(),
        Some("123456789012345678901234567890")
    );
}

#[test]
fn schemas_can_use_obi_positions_or_absolute_resource_identity() {
    // An adapter has extracted Node from a protocol artifact. This is its output,
    // not a general schema rewriter: rewriting must track schema positions.
    let relocated = r##"{
        "openbindings":"0.2.0",
        "operations":{"read":{"output":{"$ref":"#/schemas/Node"}}},
        "schemas":{"Node":{"type":"object","properties":{
            "next":{"$ref":"#/schemas/Node"}
        }}}
    }"##;
    let identified = r##"{
        "openbindings":"0.2.0",
        "operations":{"read":{"output":{"$ref":"https://example.test/schemas/node"}}},
        "schemas":{"Node":{"$id":"https://example.test/schemas/node","type":"object","properties":{
            "next":{"$ref":"#"}
        }}}
    }"##;
    for text in [relocated, identified] {
        let proof = Document::parse(text)
            .unwrap()
            .validate()
            .unwrap()
            .conformant()
            .unwrap();
        let ReferenceReport::Complete { references } = proof.document().references() else {
            panic!("reference inspection did not finish");
        };
        assert_eq!(references.len(), 2);
        assert!(references.iter().all(|reference| matches!(
            &reference.resolution,
            ReferenceResolution::Located { target, .. }
                if target.pointer.as_str() == Some("/schemas/Node")
        )));
    }
}

#[test]
fn source_data_handles_share_the_retained_text() {
    let proof = Document::parse(
        r#"{
        "openbindings":"0.2.0","operations":{},
        "sources":{"s":{"kind":"example.memory@1","data":{"channel":"local"}}}
    }"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let source = proof.source("s").unwrap();
    let retained = source.data().unwrap();
    let next = source.data().unwrap();
    assert!(std::ptr::eq(
        retained.text().as_bytes(),
        next.text().as_bytes()
    ));
    // A cache can retain the handle and compare its text without a String copy.
    assert_eq!(retained.text(), next.text());
}

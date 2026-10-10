//! Compose an opaque-object edit using only the public SDK and serde_json.
use openbindings::{Conformance, JsonValue, ParsedDocument};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Application recipe for replacing one immediate object member.
///
/// Requires unique names throughout the subtree and UTF-8-representable names.
/// Borrows both inputs; the result is independent. It visits every member and
/// serializes/parses the complete resulting object. Formatting/order may change.
/// This is not an arbitrary JSON Patch or source-preserving text editor.
pub fn replace_exact_object_member(
    object: &JsonValue,
    key: &str,
    replacement: &JsonValue,
) -> Result<JsonValue> {
    if object.has_duplicate_names() {
        return Err("this recipe requires unique member names".into());
    }
    let members = object.view().members().ok_or("expected an exact object")?;
    let mut composed = BTreeMap::new();
    for member in members {
        let name = member
            .name
            .as_str()
            .ok_or("member name is not UTF-8 representable")?;
        composed.insert(name, member.value.to_owned());
    }
    let target = composed
        .get_mut(key)
        .ok_or("replacement member is absent")?;
    *target = replacement.clone();
    // JsonValue's Serde representation carries raw JSON tokens. Do not convert
    // these exact values through ordinary serde_json::Value or floating point.
    Ok(JsonValue::parse(serde_json::to_vec(&composed)?)?)
}

/// Use the ordinary typed authoring path around the changed opaque leaf.
pub fn edit_extension_member(
    document: &ParsedDocument,
    extension: &str,
    member: &str,
    replacement: &JsonValue,
) -> Result<ParsedDocument> {
    let mut draft = document.to_authoring()?;
    let leaf = draft
        .additional_fields
        .get(extension)
        .ok_or("extension is absent")?;
    let changed = replace_exact_object_member(leaf, member, replacement)?;
    draft.additional_fields.insert(extension.into(), changed);
    Ok(draft.build()?)
}

const DOCUMENT: &str = r#"{"openbindings":"0.2.0","operations":{},"x-editor":{
    "annotation":null,"large":900719925474099312345,"tiny":1e-1000,
    "negative":-0,"é":1e500,"e\u0301":2,"__proto__":{"value":3.00}
}}"#;

fn main() -> Result<()> {
    let revised = {
        let original = ParsedDocument::parse(DOCUMENT)?;
        let replacement = JsonValue::string("Reviewed")?;
        edit_extension_member(&original, "x-editor", "annotation", &replacement)?
    }; // Original, draft and replacement can all be dropped before using this.
    assert_eq!(
        revised.assess()?.report().conclusion,
        Conformance::Conformant
    );
    for (key, token) in [
        ("large", "900719925474099312345"),
        ("tiny", "1e-1000"),
        ("negative", "-0"),
        ("é", "1e500"),
        ("e\u{301}", "2"),
    ] {
        assert_eq!(
            revised
                .value()
                .at(&format!("/x-editor/{key}"))
                .unwrap()
                .text(),
            token
        );
    }
    assert_eq!(
        revised
            .value()
            .at("/x-editor/__proto__/value")
            .unwrap()
            .text(),
        "3.00"
    );
    assert_eq!(
        revised.value().at("/x-editor/annotation").unwrap().as_str(),
        Some("Reviewed")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_edit_preserves_exact_siblings_after_input_drop() {
        main().unwrap();
    }

    #[test]
    fn input_errors_leave_the_original_usable() {
        let replacement = JsonValue::boolean(true);
        for text in [
            "null",
            r#"{"other":0}"#,
            r#"{"x":0,"x":1}"#,
            r#"{"x":0,"nested":{"a":1,"a":2}}"#,
            r#"{"x":0,"\ud800":1}"#,
        ] {
            let original = JsonValue::parse(text).unwrap();
            assert!(replace_exact_object_member(&original, "x", &replacement).is_err());
            assert_eq!(original.text(), text);
        }
    }
}

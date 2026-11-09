//! Negative drafts never acquire canonical domain entries; raw JSON stays exact.
use openbindings::*;
fn reject(text: &str) {
    let doc = Document::parse(text).unwrap();
    assert_eq!(doc.as_bytes(), text.as_bytes());
    assert!(doc.validate().unwrap().conformant().is_none(), "{text}");
}
#[test]
fn malformed_original_drafts_remain_unproved() {
    for text in [
        r#"{"openbindings":"0.2.0","name":"\ud800","operations":{"op":{"description":"\udc00","tags":["\ud800"]}},"sources":{"s":{"kind":"\udc00"}},"dependencies":{"d":{"operation":"op","kinds":["\ud800"]}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"\ud800":{"aliases":["shadow"]},"ok":{}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"ok","source":"s"}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"ok":{}},"sources":{"s":{"kind":"k"}},"bindings":{"\ud800":{"operation":"ok","source":"s"},"b":{"operation":"ok","source":"s"}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"ok":{}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"\ud800","source":"s"},"c":{"operation":"ok","source":"s"}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"unrelated":{"aliases":[false]}},"sources":{"good":{"kind":"good-kind","description":"ok"},"bad":{"kind":"valid-kind","description":42},"shape":null},"dependencies":{"good":{"operation":"absent"},"bad":{"kinds":42}},"bindings":{"good":{"operation":"absent","source":"missing","description":false}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"a":{"tags":["ok",null],"deprecated":null},"b":{"tags":[],"deprecated":false},"c":{}}}"#,
        r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["alias"],"tags":["one"],"examples":{"z":{"description":42,"input":null},"a":{"input":9007199254740993,"output":0.29000000000000001},"bad":false}},"empty":{"examples":{}},"none":{}}}"#,
    ] {
        reject(text);
    }
}
#[test]
fn unpaired_draft_text_is_exact_without_a_domain_proof() {
    let doc = Document::parse(r#"{"openbindings":"0.2.0","name":"\ud800","operations":{"op":{"description":"\udc00","tags":["\ud800"]}},"sources":{"s":{"kind":"\udc00"}},"dependencies":{"d":{"operation":"op","kinds":["\ud800"]}}}"#).unwrap();
    for (pointer, unit) in [
        ("/name", 0xd800),
        ("/operations/op/description", 0xdc00),
        ("/operations/op/tags/0", 0xd800),
        ("/sources/s/kind", 0xdc00),
        ("/dependencies/d/kinds/0", 0xd800),
    ] {
        let value = doc.value().at(pointer).unwrap();
        let read = value.as_string().unwrap();
        assert_eq!(read.as_str(), None);
        assert_eq!(read.utf16_units().collect::<Vec<_>>(), [unit]);
    }
    assert!(doc.validate().unwrap().conformant().is_none());
}
#[test]
fn malformed_namespaces_and_declared_fields_remain_conformance_negatives() {
    for field in [
        "operations",
        "sources",
        "bindings",
        "dependencies",
        "schemas",
    ] {
        for value in ["null", "[]", "false", "7", r#""object""#] {
            let operations = if field == "operations" {
                String::new()
            } else {
                String::from(r#", "operations":{}"#)
            };
            reject(&format!(
                r#"{{"openbindings":"0.2.0"{operations},"{field}":{value}}}"#
            ));
        }
    }
    for fields in [
        r#""sources":{"none":{},"null":{"kind":null}},"bindings":{"none":{},"bad":{"operation":false,"source":null,"idempotent":null,"deprecated":0}}"#,
        r#""dependencies":{"empty":{"operation":"x","kinds":[]},"bad":{"operation":"x","kinds":["kind",false]}}"#,
        r#""sources":{"bad":{"kind":"k","description":42}}"#,
        r#""sources":{"s":{},"s":{}}"#,
    ] {
        reject(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},{fields}}}"#
        ));
    }
    reject(r#"{"openbindings":1,"sources":{}}"#);
}

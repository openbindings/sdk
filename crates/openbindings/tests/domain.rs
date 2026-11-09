//! The canonical public document surface: exact original fields and same-snapshot relations.
use openbindings::*;
use std::fmt::Debug;

fn proof(text: &str) -> ConformantDocument {
    let assessment = Document::parse(text).unwrap().validate().unwrap();
    assert_eq!(
        assessment.report().conclusion,
        Conformance::Conformant,
        "{text}"
    );
    assessment.conformant().unwrap()
}
fn empty(fields: &str) -> ConformantDocument {
    proof(&format!(
        r#"{{"openbindings":"0.2.0","operations":{{}}{fields}}}"#
    ))
}
fn populated() -> ConformantDocument {
    proof(include_str!("fixtures/canonical-domain.json"))
}
#[test]
fn all_32_declared_fields_have_canonical_access() {
    let doc = populated();
    assert_eq!(doc.openbindings(), "0.2.0");
    assert_eq!(doc.name(), Some("interface"));
    assert_eq!(doc.version(), Some("release-label"));
    assert_eq!(doc.description(), Some("document"));
    assert_eq!(
        doc.operations()
            .iter()
            .map(Operation::key)
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    let op = doc.operation("z").unwrap();
    assert_eq!(op.description(), Some("operation"));
    assert_eq!(op.deprecated(), Some(false));
    assert_eq!(op.tags().unwrap().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(op.aliases().unwrap().collect::<Vec<_>>(), ["alias"]);
    assert_eq!(op.input().unwrap().json().text(), "true");
    assert_eq!(op.output().unwrap().json().text(), "false");
    assert_eq!(
        op.examples()
            .unwrap()
            .iter()
            .map(Example::key)
            .collect::<Vec<_>>(),
        ["null", "sample"]
    );
    let example = op.example("sample").unwrap();
    assert_eq!(example.description(), Some("example"));
    assert_eq!(example.input().unwrap().text(), "9007199254740993");
    assert_eq!(example.output().unwrap().text(), "0.29000000000000001");
    let source = doc.source("s").unwrap();
    assert_eq!(source.kind(), "example.Kind@1");
    assert_eq!(source.description(), Some("source"));
    assert_eq!(source.data().unwrap().text(), "null");
    let binding = doc.binding("z").unwrap();
    assert_eq!(binding.operation_key(), "z");
    assert_eq!(binding.source_key(), "s");
    assert_eq!(binding.description(), Some("binding"));
    assert_eq!(binding.preference(), Some(-1));
    assert_eq!(binding.idempotent(), Some(false));
    assert_eq!(binding.deprecated(), Some(true));
    assert_eq!(
        binding.data().unwrap().get("wide").unwrap().text(),
        "9007199254740993"
    );
    let dependency = doc.dependency("d").unwrap();
    assert_eq!(dependency.operation_key(), "z");
    assert_eq!(dependency.description(), Some("dependency"));
    assert_eq!(
        dependency.kinds().unwrap().collect::<Vec<_>>(),
        ["example.Kind@1", "other"]
    );
    assert_eq!(doc.sources().unwrap().len(), 1);
    assert_eq!(doc.bindings().unwrap().len(), 2);
    assert_eq!(doc.dependencies().unwrap().len(), 1);
    assert_eq!(
        doc.schemas()
            .unwrap()
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(doc.schema("a").unwrap().location().pointer, "/schemas/a");
    assert_eq!(doc.schema("z").unwrap().json().text(), "false");
}
#[test]
fn absent_empty_false_and_null_remain_distinct() {
    let absent = empty("");
    assert_eq!(
        (absent.name(), absent.version(), absent.description()),
        (None, None, None)
    );
    assert!(absent.operations().is_empty());
    assert!(absent.bindings().is_none());
    assert!(absent.sources().is_none());
    assert!(absent.dependencies().is_none());
    assert!(absent.schemas().is_none());
    assert!(absent.binding("missing").is_none());
    assert!(absent.source("missing").is_none());
    assert!(absent.dependency("missing").is_none());
    assert!(absent.schema("missing").is_none());
    let empty = empty(
        r#", "name":"","version":"label","description":"","bindings":{},"sources":{},"dependencies":{},"schemas":{}"#,
    );
    assert_eq!(
        (empty.name(), empty.version(), empty.description()),
        (Some(""), Some("label"), Some(""))
    );
    assert!(empty.bindings().unwrap().is_empty());
    assert!(empty.sources().unwrap().is_empty());
    assert!(empty.dependencies().unwrap().is_empty());
    assert!(empty.schemas().unwrap().is_empty());
    let doc = proof(
        r#"{"openbindings":"0.2.0","operations":{"none":{},"empty":{"tags":[],"aliases":[],"deprecated":false,"examples":{}},"present":{"deprecated":true,"examples":{"sample":{"input":null,"output":null}}}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"none","source":"s"}},"dependencies":{"d":{"operation":"none"}}}"#,
    );
    let none = doc.operation("none").unwrap();
    assert_eq!(none.description(), None);
    assert_eq!(none.deprecated(), None);
    assert!(none.tags().is_none());
    assert!(none.aliases().is_none());
    assert!(none.input().is_none());
    assert!(none.output().is_none());
    assert!(none.examples().is_none());
    assert!(none.example("missing").is_none());
    let empty = doc.operation("empty").unwrap();
    assert_eq!(empty.tags().unwrap().len(), 0);
    assert_eq!(empty.aliases().unwrap().len(), 0);
    assert_eq!(empty.deprecated(), Some(false));
    assert!(empty.examples().unwrap().is_empty());
    let present = doc.operation("present").unwrap();
    assert_eq!(present.deprecated(), Some(true));
    let example = present.example("sample").unwrap();
    assert_eq!(example.description(), None);
    assert_eq!(example.input().unwrap().kind(), JsonKind::Null);
    assert_eq!(example.output().unwrap().kind(), JsonKind::Null);
    let source = doc.source("s").unwrap();
    assert_eq!(source.description(), None);
    assert!(source.data().is_none());
    let binding = doc.binding("b").unwrap();
    assert_eq!(binding.description(), None);
    assert_eq!(binding.preference(), None);
    assert_eq!(binding.idempotent(), None);
    assert_eq!(binding.deprecated(), None);
    assert!(binding.data().is_none());
    let dependency = doc.dependency("d").unwrap();
    assert_eq!(dependency.description(), None);
    assert!(dependency.kinds().is_none());
    assert!(dependency.accepts_kind("anything"));
}
#[test]
fn exact_keys_aliases_kinds_and_unranked_relations() {
    let doc = populated();
    assert!(doc.operation("alias").is_none());
    assert_eq!(doc.resolve_operation("alias").unwrap().key(), "z");
    assert_eq!(doc.resolve_operation("z").unwrap().key(), "z");
    assert!(doc.resolve_operation("Z").is_none());
    let binding = doc.binding("z").unwrap();
    assert_eq!(binding.operation().key(), "z");
    assert_eq!(binding.source().kind(), "example.Kind@1");
    // Lexical enumeration is presentation policy, not preference or deprecation ranking.
    assert_eq!(
        binding
            .operation()
            .bindings()
            .iter()
            .map(Binding::key)
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert!(doc.operation("a").unwrap().bindings().is_empty());
    let dependency = doc.dependency("d").unwrap();
    assert_eq!(dependency.operation().key(), "z");
    assert!(dependency.accepts_kind("example.Kind@1"));
    assert!(!dependency.accepts_kind("example.kind@1"));
    assert!(!dependency.accepts_kind("example.Kind@1 "));
}
#[test]
fn owners_keep_original_entries_and_relations_across_drop_and_edits() {
    fn owner<T: Clone + Debug + Send + Sync + 'static>() {}
    owner::<Operation>();
    owner::<Binding>();
    owner::<Source>();
    owner::<Dependency>();
    owner::<Example>();
    let doc = populated();
    let binding = doc.binding("z").unwrap();
    let dependency = doc.dependency("d").unwrap();
    let operation = doc.operation("z").unwrap();
    let sample = operation.example("sample").unwrap();
    let other = doc.operation("a").unwrap().example("sample").unwrap();
    let schema = doc.schema("a").unwrap();
    let source_json = binding.source().value().to_owned();
    let changed = doc
        .document()
        .edit(&[
            Edit::set(
                &["sources", "s", "kind"],
                JsonValue::string("changed").unwrap(),
            ),
            Edit::set(
                &["operations", "z", "description"],
                JsonValue::string("changed").unwrap(),
            ),
            Edit::set(
                &["operations", "z", "examples", "sample", "output"],
                JsonValue::parse("null").unwrap(),
            ),
        ])
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap();
    assert_eq!(changed.binding("z").unwrap().source().kind(), "changed");
    drop((doc, changed, operation));
    assert_eq!(binding.source().kind(), "example.Kind@1");
    assert_eq!(dependency.operation().description(), Some("operation"));
    assert_eq!(
        sample.value().location().pointer.unwrap().as_str(),
        Some("/operations/z/examples/sample")
    );
    assert_eq!(
        other.value().location().pointer.unwrap().as_str(),
        Some("/operations/a/examples/sample")
    );
    assert_eq!(other.output().unwrap().text(), "\"another\"");
    assert_eq!(sample.value().get("x-exact").unwrap().text(), "1.00");
    let input = sample.input().unwrap();
    let output = sample.output().unwrap();
    drop((sample, other, binding, dependency));
    assert_eq!(input.text(), "9007199254740993");
    assert_eq!(output.text(), "0.29000000000000001");
    assert_eq!(source_json.get("x-exact").unwrap().text(), "1e2");
    assert_eq!(schema.json().text(), r#"{"type":"string"}"#);
    assert_eq!(schema.document().name(), Some("interface"));
}
#[test]
fn preference_reads_proved_mathematical_integers_not_token_syntax() {
    for (token, expected) in [
        ("1", 1),
        ("1.0", 1),
        ("-0", 0),
        ("1e2", 100),
        ("9007199254740991", 9_007_199_254_740_991),
        ("-9007199254740991", -9_007_199_254_740_991),
        ("90071992547409910e-1", 9_007_199_254_740_991),
        ("0e999999999999999999999999999999999999999999", 0),
        ("-0e-999999999999999999999999999999999999999999", 0),
    ] {
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{"x":{{}}}},"sources":{{"s":{{"kind":"k"}}}},"bindings":{{"b":{{"operation":"x","source":"s","preference":{token}}}}}}}"#
        );
        let doc = proof(&text);
        let binding = doc.binding("b").unwrap();
        assert_eq!(binding.preference(), Some(expected), "{token}");
        assert_eq!(binding.value().get("preference").unwrap().text(), token);
    }
    for token in [
        "null",
        "true",
        r#""1""#,
        "1.1",
        "1e-1",
        "9007199254740992",
        "-9007199254740992",
        "1e999999999999999999999999999",
        "1e-999999999999999999999999999",
    ] {
        let doc = Document::parse(format!(r#"{{"openbindings":"0.2.0","operations":{{"x":{{}}}},"sources":{{"s":{{"kind":"k"}}}},"bindings":{{"b":{{"operation":"x","source":"s","preference":{token}}}}}}}"#)).unwrap();
        assert!(doc.validate().unwrap().conformant().is_none(), "{token}");
        assert_eq!(
            doc.value().at("/bindings/b/preference").unwrap().text(),
            token
        );
    }
}
#[test]
fn paired_escapes_and_noncharacters_are_scalar_text_without_normalization() {
    let doc = proof(
        r#"{"openbindings":"0.2.0","operations":{"op":{"aliases":["alias"],"description":"\ud83d\ude00","tags":["\uffff"]}},"sources":{"s":{"kind":"\ufffe"}}}"#,
    );
    let operation = doc.resolve_operation("alias").unwrap();
    assert_eq!(operation.key(), "op");
    assert_eq!(operation.description(), Some("😀"));
    assert_eq!(operation.tags().unwrap().next(), Some("\u{ffff}"));
    assert_eq!(doc.source("s").unwrap().kind(), "\u{fffe}");
    assert_eq!(
        operation.value().get("description").unwrap().text(),
        r#""\ud83d\ude00""#
    );
}

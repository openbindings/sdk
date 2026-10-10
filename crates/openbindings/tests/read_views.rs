use openbindings::*;
use std::fmt::Debug;

fn document(fields: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{{}}{fields}}}"#
    ))
    .unwrap()
}
fn location(error: InterpretationError, code: &str, pointer: &str) {
    assert_eq!(error.code(), code);
    assert_eq!(
        error.source_location().unwrap().pointer.as_deref(),
        Some(pointer)
    );
}

#[test]
fn namespaces_preserve_absence_empty_shape_and_lexical_order() {
    let absent = document("");
    assert!(absent.bindings().unwrap().is_none());
    assert!(absent.sources().unwrap().is_none());
    assert!(absent.dependencies().unwrap().is_none());
    assert!(absent.binding("missing").unwrap().is_none());
    assert!(absent.source("missing").unwrap().is_none());
    assert!(absent.dependency("missing").unwrap().is_none());
    let empty = document(r#", "bindings":{},"sources":{},"dependencies":{}"#);
    assert!(empty.bindings().unwrap().unwrap().is_empty());
    assert!(empty.sources().unwrap().unwrap().is_empty());
    assert!(empty.dependencies().unwrap().unwrap().is_empty());
    for value in ["null", "[]", "false", "7", r#""object""#] {
        let malformed = document(&format!(
            r#", "bindings":{value},"sources":{value},"dependencies":{value}"#
        ));
        location(
            malformed.bindings().unwrap_err(),
            "invalid-bindings-object",
            "/bindings",
        );
        location(
            malformed.source("absent").unwrap_err(),
            "invalid-sources-object",
            "/sources",
        );
        location(
            malformed.dependencies().unwrap_err(),
            "invalid-dependencies-object",
            "/dependencies",
        );
    }
    let names = document(r#", "sources":{"z":{"kind":"z"},"a":{"kind":"a"}}"#);
    assert_eq!(
        names
            .sources()
            .unwrap()
            .unwrap()
            .iter()
            .map(|v| v.key())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
}

#[test]
fn keyed_lookup_and_each_field_are_local() {
    let doc = ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"unrelated":{"aliases":[false]}},"sources":{"good":{"kind":"good-kind","description":"ok"},"bad":{"kind":"valid-kind","description":42},"shape":null},"dependencies":{"good":{"operation":"absent"},"bad":{"kinds":42}},"bindings":{"good":{"operation":"absent","source":"missing","description":false}}}"#).unwrap();
    assert!(doc.operations().is_err());
    let good = doc.source("good").unwrap().unwrap();
    assert_eq!(good.kind().unwrap(), "good-kind");
    assert_eq!(good.description().unwrap(), Some("ok"));
    let bad = doc.source("bad").unwrap().unwrap();
    assert_eq!(bad.kind().unwrap(), "valid-kind");
    location(
        bad.description().unwrap_err(),
        "invalid-source-description",
        "/sources/bad/description",
    );
    assert!(doc.sources().is_err()); // wrong object shape is checked by enumeration
    location(
        doc.source("shape").unwrap_err(),
        "invalid-source-object",
        "/sources/shape",
    );
    let dep = doc.dependency("good").unwrap().unwrap();
    assert_eq!(dep.operation().unwrap(), "absent");
    assert!(dep.kinds().unwrap().is_none());
    let binding = doc.binding("good").unwrap().unwrap();
    assert_eq!(binding.operation().unwrap(), "absent");
    assert_eq!(binding.source().unwrap(), "missing");
    assert!(doc.source(binding.source().unwrap()).unwrap().is_none());
    location(
        binding.description().unwrap_err(),
        "invalid-binding-description",
        "/bindings/good/description",
    );
    let metadata_only = document(r#", "sources":{"bad":{"kind":"k","description":42}}"#);
    assert_eq!(metadata_only.sources().unwrap().unwrap().len(), 1);
}

#[test]
fn required_scalars_optional_flags_and_exact_payloads_are_distinct() {
    let doc = document(
        r#", "sources":{"none":{},"null":{"kind":null}},"bindings":{"none":{},"explicit":{"operation":"x","source":"none","preference":0,"idempotent":false,"deprecated":false,"content":null,"x-wide":9007199254740993},"bad":{"operation":false,"source":null,"idempotent":null,"deprecated":0}}"#,
    );
    let none = doc.binding("none").unwrap().unwrap();
    assert!(none.content().is_none());
    assert_eq!(none.preference().unwrap(), None);
    assert_eq!(none.idempotent().unwrap(), None);
    assert_eq!(none.deprecated().unwrap(), None);
    location(
        none.operation().unwrap_err(),
        "missing-binding-operation",
        "/bindings/none",
    );
    location(
        none.source().unwrap_err(),
        "missing-binding-source",
        "/bindings/none",
    );
    let explicit = doc.binding("explicit").unwrap().unwrap();
    assert_eq!(explicit.preference().unwrap().unwrap().get(), 0);
    assert_eq!(explicit.idempotent().unwrap(), Some(false));
    assert_eq!(explicit.deprecated().unwrap(), Some(false));
    assert_eq!(explicit.content().unwrap().kind(), JsonKind::Null);
    assert_eq!(
        explicit.value().get("x-wide").unwrap().number_text(),
        Some("9007199254740993")
    );
    let bad = doc.binding("bad").unwrap().unwrap();
    location(
        bad.operation().unwrap_err(),
        "invalid-binding-operation",
        "/bindings/bad/operation",
    );
    location(
        bad.source().unwrap_err(),
        "invalid-binding-source",
        "/bindings/bad/source",
    );
    location(
        bad.idempotent().unwrap_err(),
        "invalid-binding-idempotent",
        "/bindings/bad/idempotent",
    );
    location(
        bad.deprecated().unwrap_err(),
        "invalid-binding-deprecated",
        "/bindings/bad/deprecated",
    );
    location(
        doc.source("none").unwrap().unwrap().kind().unwrap_err(),
        "missing-source-kind",
        "/sources/none",
    );
    location(
        doc.source("null").unwrap().unwrap().kind().unwrap_err(),
        "invalid-source-kind",
        "/sources/null/kind",
    );
}

#[test]
fn preference_reuses_exact_authoring_rules() {
    for (token, expected) in [
        ("1.0", 1),
        ("-0", 0),
        ("1e2", 100),
        ("9007199254740991", 9_007_199_254_740_991),
        ("-9007199254740991", -9_007_199_254_740_991),
    ] {
        let doc = document(&format!(
            r#", "bindings":{{"b":{{"operation":"x","source":"s","preference":{token}}}}}"#
        ));
        assert_eq!(
            doc.binding("b")
                .unwrap()
                .unwrap()
                .preference()
                .unwrap()
                .unwrap()
                .get(),
            expected
        );
        assert_eq!(
            doc.to_authoring().unwrap().bindings.unwrap()["b"]
                .preference
                .unwrap()
                .get(),
            expected
        );
    }
    for token in ["null", "true", r#""1""#, "1.1", "9007199254740992", "1e999"] {
        let doc = document(&format!(
            r#", "bindings":{{"b":{{"operation":"x","source":"s","preference":{token}}}}}"#
        ));
        location(
            doc.binding("b").unwrap().unwrap().preference().unwrap_err(),
            "invalid-binding-preference",
            "/bindings/b/preference",
        );
        assert!(doc.to_authoring().is_err());
    }
}

#[test]
fn lists_are_checked_before_any_items_escape() {
    let doc = document(
        r#", "dependencies":{"none":{"operation":"x"},"empty":{"operation":"x","kinds":[]},"one":{"operation":"x","kinds":["kind"]},"bad":{"operation":"x","kinds":["kind",false]}}"#,
    );
    assert_eq!(
        doc.dependency_accepts_kind("none", "anything").unwrap(),
        Some(true)
    );
    assert_eq!(
        doc.dependency_accepts_kind("empty", "anything").unwrap(),
        Some(false)
    );
    assert_eq!(
        doc.dependency_accepts_kind("one", "kind").unwrap(),
        Some(true)
    );
    assert_eq!(
        doc.dependency_accepts_kind("one", "KIND").unwrap(),
        Some(false)
    );
    assert_eq!(
        doc.dependency_accepts_kind("missing", "kind").unwrap(),
        None
    );
    let error = match doc.dependency("bad").unwrap().unwrap().kinds() {
        Err(e) => e,
        Ok(_) => panic!("invalid list returned"),
    };
    location(
        error,
        "invalid-dependency-kind",
        "/dependencies/bad/kinds/1",
    );
    // Even a matching first item cannot hide the invalid later item.
    assert!(doc.dependency_accepts_kind("bad", "kind").is_err());
    let opdoc = ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"a":{"tags":["ok",null],"deprecated":null},"b":{"tags":[],"deprecated":false},"c":{}}}"#).unwrap();
    let ops = opdoc.operations().unwrap();
    let error = match ops[0].tags() {
        Err(e) => e,
        Ok(_) => panic!("invalid tags returned"),
    };
    location(error, "invalid-operation-tag", "/operations/a/tags/1");
    location(
        ops[0].deprecated().unwrap_err(),
        "invalid-operation-deprecated",
        "/operations/a/deprecated",
    );
    assert_eq!(ops[1].tags().unwrap().unwrap().len(), 0);
    assert_eq!(ops[1].deprecated().unwrap(), Some(false));
    assert!(ops[2].tags().unwrap().is_none());
    assert_eq!(ops[2].deprecated().unwrap(), None);
}

#[test]
fn examples_share_operation_namespace_and_retain_exact_instances() {
    let doc=ParsedDocument::parse(r#"{"openbindings":"0.2.0","operations":{"a":{"aliases":["alias"],"tags":["one"],"examples":{"z":{"description":42,"input":null},"a":{"input":9007199254740993,"output":0.29000000000000001},"bad":false}},"empty":{"examples":{}},"none":{}}}"#).unwrap();
    let OperationSelection::Found(op) = doc.resolve_operation("alias").unwrap() else {
        panic!()
    };
    assert_eq!(op.tags().unwrap().unwrap().collect::<Vec<_>>(), ["one"]);
    assert!(op.examples().is_err());
    let example = op.example("a").unwrap().unwrap();
    assert_eq!(example.key(), "a");
    assert_eq!(example.input().unwrap().text(), "9007199254740993");
    let exact = example.output().unwrap().to_owned();
    let z = op.example("z").unwrap().unwrap();
    assert_eq!(z.input().unwrap().kind(), JsonKind::Null);
    assert!(z.output().is_none());
    location(
        z.description().unwrap_err(),
        "invalid-example-description",
        "/operations/a/examples/z/description",
    );
    assert!(op.example("missing").unwrap().is_none());
    let empty = doc
        .operations()
        .unwrap()
        .into_iter()
        .find(|o| o.key() == "empty")
        .unwrap();
    assert!(empty.examples().unwrap().unwrap().is_empty());
    let none = doc
        .operations()
        .unwrap()
        .into_iter()
        .find(|o| o.key() == "none")
        .unwrap();
    assert!(none.examples().unwrap().is_none());
    drop(op);
    drop(doc);
    drop(example);
    assert_eq!(exact.text(), "0.29000000000000001");
    assert_eq!(z.input().unwrap().text(), "null");
}

#[test]
fn owned_traits_document_drop_and_global_refusals() {
    fn owner<T: Clone + Debug + Send + Sync + 'static>() {}
    owner::<BindingView>();
    owner::<SourceView>();
    owner::<DependencyView>();
    owner::<ExampleView>();
    let doc = document(r#", "sources":{"s":{"kind":"k","content":{"large":9007199254740993}}}"#);
    let view = doc.source("s").unwrap().unwrap();
    let retained = view.clone();
    let kind = retained.kind().unwrap();
    drop(doc);
    drop(view);
    assert_eq!(kind, "k");
    let exact = retained.content().unwrap().to_owned();
    drop(retained);
    assert_eq!(exact.get("large").unwrap().text(), "9007199254740993");
    for (text, code) in [
        (
            r#"{"openbindings":"0.2.0","sources":{"s":{},"s":{}}}"#,
            "duplicate-members",
        ),
        (
            r#"{"openbindings":"0.2.0","sources":{},"x":"\ud800"}"#,
            "unpaired-string",
        ),
        (r#"{"openbindings":1,"sources":{}}"#, "malformed-version"),
    ] {
        let error = ParsedDocument::parse(text).unwrap().sources().unwrap_err();
        assert_eq!(error.code(), code);
        assert!(error.source_location().is_none());
    }
}

use openbindings::*;

fn proof(text: &str) -> Conformant<Document> {
    let validation = validate_document(text).unwrap();
    validation
        .conformant()
        .unwrap_or_else(|| panic!("{:?}", validation.report()))
}

#[test]
fn decoded_text_and_exact_tokens_survive_in_retained_conformant_views() {
    let document = proof(
        r#"{"openbindings":"0.2.0","name":"\ud800\udc00","version":"\uffff","description":"\ud800\udc00","operations":{"run":{"description":"\ud800\udc00","tags":["\ud800\udc00","𝄞"],"aliases":["go"],"deprecated":false,"input":true,"output":false,"examples":{"e":{"description":"\ud800\udc00","input":null}}}},"sources":{"s":{"kind":"\ud800\udc00","description":"\uffff","content":null}},"bindings":{"b":{"operation":"run","source":"s","description":"\ud800\udc00","preference":0,"idempotent":false,"deprecated":false,"content":null}},"dependencies":{"d":{"operation":"run","kinds":["\uD800\uDC00"],"description":"\ud800\udc00"}},"schemas":{"z":false,"a":{}}}"#,
    );
    assert_eq!(document.openbindings(), "0.2.0");
    let name = document.name().unwrap();
    assert_eq!(name, "𐀀");
    assert_eq!(document.version(), Some("\u{ffff}"));
    assert_eq!(Some(name), document.description());
    assert_ne!(name, "�");
    let exact_name = document.value().get("name").unwrap();
    assert_eq!(exact_name.text(), r#""\ud800\udc00""#);
    assert_eq!(exact_name.location().pointer.as_deref(), Some("/name"));
    assert_eq!(
        document
            .schemas()
            .unwrap()
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(document.schema("z").unwrap().as_bool(), Some(false));
    let operation = document.resolve_operation("go").unwrap();
    let binding = document.binding("b").unwrap();
    let source = document.source("s").unwrap();
    let dependency = document.dependency("d").unwrap();
    let example = operation.example("e").unwrap();
    assert!(dependency.accepts_kind(source.kind()));
    assert_eq!(document.as_document().description().unwrap().unwrap(), name);
    assert_eq!(
        document
            .as_document()
            .source("s")
            .unwrap()
            .unwrap()
            .kind()
            .unwrap(),
        name
    );
    let retained = exact_name.to_owned();
    drop(document);
    assert_eq!(operation.key(), "run");
    assert_eq!(operation.bindings(), ["b"]);
    assert_eq!(operation.aliases().unwrap().collect::<Vec<_>>(), ["go"]);
    assert_eq!(operation.tags().unwrap().collect::<Vec<&str>>(), ["𐀀", "𝄞"]);
    assert_eq!(operation.deprecated(), Some(false));
    assert_eq!(operation.input().unwrap().schema().as_bool(), Some(true));
    assert_eq!(operation.output().unwrap().schema().as_bool(), Some(false));
    assert_eq!(binding.source(), "s");
    assert_eq!(binding.operation(), "run");
    assert_eq!(binding.preference().unwrap().get(), 0);
    assert_eq!(binding.idempotent(), Some(false));
    assert_eq!(binding.deprecated(), Some(false));
    for value in [binding.content(), source.content(), example.input()] {
        assert_eq!(value.unwrap().kind(), JsonKind::Null);
    }
    assert!(example.output().is_none());
    assert_eq!(dependency.operation(), "run");
    assert_eq!(
        example.description().unwrap(),
        retained.view().as_str().unwrap()
    );
    assert_eq!(
        binding.description().unwrap(),
        retained.view().as_str().unwrap()
    );
}

#[test]
fn presence_and_field_local_editor_errors_remain_distinct() {
    let absent = proof(r#"{"openbindings":"0.2.0","operations":{"run":{}}}"#);
    assert!(absent.name().is_none());
    assert!(absent.version().is_none());
    assert!(absent.description().is_none());
    assert!(absent.schemas().is_none());
    assert!(absent.bindings().is_none());
    assert!(absent.sources().is_none());
    assert!(absent.dependencies().is_none());
    assert!(absent.resolve_operation("missing").is_none());
    let op = absent.resolve_operation("run").unwrap();
    assert!(op.tags().is_none());
    assert!(op.aliases().is_none());
    assert!(op.deprecated().is_none());
    assert!(op.input().is_none());
    assert!(op.output().is_none());
    assert!(op.examples().is_none());
    let empty = proof(
        r#"{"openbindings":"0.2.0","name":"","schemas":{},"bindings":{},"sources":{},"dependencies":{},"operations":{"run":{"input":{},"tags":[],"aliases":[],"examples":{}}}}"#,
    );
    assert_eq!(empty.name(), Some(""));
    assert!(empty.schemas().unwrap().is_empty());
    assert!(empty.bindings().unwrap().is_empty());
    assert!(empty.sources().unwrap().is_empty());
    assert!(empty.dependencies().unwrap().is_empty());
    let op = empty.resolve_operation("run").unwrap();
    assert_eq!(op.input().unwrap().schema().kind(), JsonKind::Object);
    assert_eq!(op.tags().unwrap().len(), 0);
    assert_eq!(op.aliases().unwrap().len(), 0);
    assert!(op.examples().unwrap().is_empty());
    let parsed = Document::parse(r#"{"openbindings":"0.2.0","name":7,"operations":{"run":{"input":null,"description":false}}}"#).unwrap();
    assert_eq!(
        parsed
            .name()
            .unwrap_err()
            .source_location()
            .unwrap()
            .pointer
            .as_deref(),
        Some("/name")
    );
    let op = parsed.operations().unwrap().remove(0);
    assert_eq!(op.input().unwrap().kind(), JsonKind::Null);
    assert!(op.description().is_err());
    assert!(parsed.validate().unwrap().conformant().is_none());
}

#[test]
fn rejected_and_undetermined_documents_cannot_produce_proof() {
    assert!(
        validate_document(r#"{"openbindings":"0.2.0"}"#)
            .unwrap()
            .conformant()
            .is_none()
    );
    assert!(validate_document(r#"{"openbindings":"0.3.0","operations":{}}"#).is_err());
    let result = validate_document_with_limits(
        r#"{"openbindings":"0.2.0","operations":{}}"#,
        JsonLimits {
            max_nodes: 2,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    assert_eq!(result.report().conclusion, Conformance::Undetermined);
    assert!(result.conformant().is_none());
}

#[test]
fn migrated_unreached_uncopied_target_cases_reject_at_obi12() {
    // Both historical instance observations are unreachable through normative contracts.
    let document = include_str!("fixtures/local-reference-target.json");
    for (id, instance) in [
        ("adversarial/unreached-uncopied-targets/0", "\"s\""),
        ("adversarial/unreached-uncopied-targets/1", "1"),
    ] {
        let validation = validate_document(document).unwrap();
        assert_eq!(
            validation.report().evidence["OBI-12"],
            Evidence::Violated,
            "{id}"
        );
        assert!(validation.conformant().is_none(), "{id}: {instance}");
        assert!(
            validation
                .report()
                .findings
                .iter()
                .any(|f| f.rule == "OBI-12" && f.location.is_some())
        );
    }
}

#[test]
fn exact_string_equality_is_decoded_and_non_strings_are_distinct() {
    let escaped = JsonValue::parse(r#""\u0061\ud834\udd1e""#).unwrap();
    let literal = JsonValue::parse("\"a𝄞\"").unwrap();
    assert_eq!(escaped.view().as_string(), literal.view().as_string());
    assert!(escaped.view().as_string().unwrap().eq_str("a𝄞"));
    assert!(JsonValue::null().view().as_string().is_none());
}

#[test]
fn every_conformant_text_read_decodes_escapes_and_preserves_original_locations() {
    // OBI-01 proves scalar text, including supplementary characters and noncharacters.
    for (token, expected) in [
        (r#""\u00e9""#, "é"),
        (r#""\uD834\uDD1E""#, "𝄞"),
        (r#""\uffff""#, "\u{ffff}"),
    ] {
        let json = r#"{"openbindings":"0.2.0","name":$text,"version":$text,"description":$text,"operations":{"op":{"description":$text,"tags":[$text],"examples":{"e":{"description":$text}}}},"sources":{"s":{"kind":$text,"description":$text}},"bindings":{"b":{"operation":"op","source":"s","description":$text}},"dependencies":{"d":{"operation":"op","description":$text,"kinds":[$text]}}}"#.replace("$text", token);
        let doc = proof(&json);
        let op = doc.resolve_operation("op").unwrap();
        let binding = doc.binding("b").unwrap();
        let source = doc.source("s").unwrap();
        let dep = doc.dependency("d").unwrap();
        let example = op.example("e").unwrap();
        for read in [
            doc.name(),
            doc.version(),
            doc.description(),
            op.description(),
            binding.description(),
            source.description(),
            dep.description(),
            example.description(),
        ] {
            let read: Option<&str> = read;
            assert_eq!(read, Some(expected));
        }
        assert_eq!(op.tags().unwrap().collect::<Vec<&str>>(), [expected]);
        assert_eq!(source.kind(), expected);
        assert_eq!(dep.kinds().unwrap().collect::<Vec<&str>>(), [expected]);
        assert!(dep.accepts_kind(expected));
        // Exact token access still reports the authored spelling and byte coordinates.
        for (value, pointer) in [
            (doc.value(), "/description"),
            (op.value(), "/operations/op/description"),
            (binding.value(), "/bindings/b/description"),
            (source.value(), "/sources/s/description"),
            (dep.value(), "/dependencies/d/description"),
            (example.value(), "/operations/op/examples/e/description"),
        ] {
            let exact = value.get("description").unwrap();
            assert_eq!(exact.text(), token);
            let at = exact.location();
            assert_eq!(at.pointer.as_deref(), Some(pointer));
            assert_eq!(&json[at.byte_offset..at.byte_offset + token.len()], token);
            assert_eq!(at.line, 1);
            assert_eq!(at.byte_column, at.byte_offset + 1);
        }
    }
}

#[test]
fn conformant_kind_filters_compare_text_independent_of_escape_spelling() {
    let doc = proof(
        r#"{"openbindings":"0.2.0","operations":{"op":{}},"sources":{"s":{"kind":"caf\u00e9\uD834\uDD1E"}},"dependencies":{"escaped":{"operation":"op","kinds":["caf\u00E9\ud834\udd1e"]},"literal":{"operation":"op","kinds":["café𝄞"]},"all":{"operation":"op"},"none":{"operation":"op","kinds":["other"]}}}"#,
    );
    let source = doc.source("s").unwrap();
    for key in ["escaped", "literal", "all"] {
        assert!(doc.dependency(key).unwrap().accepts_kind(source.kind()));
        assert_eq!(doc.dependency_accepts_kind(key, "café𝄞"), Some(true));
    }
    assert!(!doc.dependency("none").unwrap().accepts_kind(source.kind()));
    assert_eq!(doc.dependency_accepts_kind("none", "café𝄞"), Some(false));
    assert_eq!(doc.dependency_accepts_kind("escaped", "cafe𝄞"), Some(false));
    assert_eq!(doc.dependency_accepts_kind("missing", "café𝄞"), None);
}

use openbindings::*;

fn proof(text: &str) -> ValidatedDocument {
    let validation = validate_document(text).unwrap();
    validation
        .validated()
        .unwrap_or_else(|| panic!("{:?}", validation.report()))
}

#[test]
fn exact_strings_and_retained_validated_views_preserve_every_utf16_unit() {
    let document = proof(
        r#"{"openbindings":"0.2.0","name":"\ud800","version":"\udc00","description":"\ud800","operations":{"run":{"description":"\ud800","tags":["\ud800","𝄞"],"aliases":["go"],"deprecated":false,"input":true,"output":false,"examples":{"e":{"description":"\ud800","input":null}}}},"sources":{"s":{"kind":"\ud800","description":"\udc00","content":null}},"bindings":{"b":{"operation":"run","source":"s","description":"\ud800","preference":0,"idempotent":false,"deprecated":false,"content":null}},"dependencies":{"d":{"operation":"run","kinds":["\uD800"],"description":"\ud800"}},"schemas":{"z":false,"a":{}}}"#,
    );
    assert_eq!(document.openbindings(), "0.2.0");
    let name = document.name().unwrap();
    assert_eq!(name.as_str(), None);
    assert_eq!(name.utf16_units().collect::<Vec<_>>(), [0xd800]);
    assert_eq!(
        document
            .version()
            .unwrap()
            .utf16_units()
            .collect::<Vec<_>>(),
        [0xdc00]
    );
    assert_eq!(name, document.description().unwrap());
    assert!(!name.eq_str("�"));
    assert_eq!(name.json().location().pointer.as_deref(), Some("/name"));
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
    assert_eq!(document.parsed().description().unwrap().unwrap(), name);
    assert_eq!(
        document
            .parsed()
            .source("s")
            .unwrap()
            .unwrap()
            .kind()
            .unwrap(),
        name
    );
    let retained = name.json().to_owned();
    drop(document);
    assert_eq!(operation.key(), "run");
    assert_eq!(operation.bindings(), ["b"]);
    assert_eq!(operation.aliases().unwrap().collect::<Vec<_>>(), ["go"]);
    assert_eq!(
        operation
            .tags()
            .unwrap()
            .nth(1)
            .unwrap()
            .utf16_units()
            .collect::<Vec<_>>(),
        [0xd834, 0xdd1e]
    );
    assert_eq!(operation.deprecated(), Some(false));
    assert_eq!(operation.input().unwrap().as_bool(), Some(true));
    assert_eq!(operation.output().unwrap().as_bool(), Some(false));
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
        retained.view().as_string().unwrap()
    );
    assert_eq!(
        binding.description().unwrap(),
        retained.view().as_string().unwrap()
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
    assert!(empty.name().unwrap().eq_str(""));
    assert!(empty.schemas().unwrap().is_empty());
    assert!(empty.bindings().unwrap().is_empty());
    assert!(empty.sources().unwrap().is_empty());
    assert!(empty.dependencies().unwrap().is_empty());
    let op = empty.resolve_operation("run").unwrap();
    assert_eq!(op.input().unwrap().kind(), JsonKind::Object);
    assert_eq!(op.tags().unwrap().len(), 0);
    assert_eq!(op.aliases().unwrap().len(), 0);
    assert!(op.examples().unwrap().is_empty());
    let parsed = ParsedDocument::parse(r#"{"openbindings":"0.2.0","name":7,"operations":{"run":{"input":null,"description":false}}}"#).unwrap();
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
    assert!(parsed.validate().unwrap().validated().is_none());
}

#[test]
fn rejected_and_undetermined_documents_cannot_produce_proof() {
    assert!(
        validate_document(r#"{"openbindings":"0.2.0"}"#)
            .unwrap()
            .validated()
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
    assert!(result.validated().is_none());
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
        assert!(validation.validated().is_none(), "{id}: {instance}");
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

use openbindings::*;

#[test]
fn kind_data_is_exact_opaque_and_presence_sensitive() {
    let document = Document::parse(r##"{"openbindings":"0.2.0","operations":{"go":{}},"sources":{"empty":{"kind":"k"},"null":{"kind":"k","data":null},"exact":{"kind":"k","data":{"n":9007199254740993,"$dynamicAnchor":"ignored","$ref":"#/absent"}}},"bindings":{"b":{"operation":"go","source":"exact","data":1e999999}}}"##).unwrap();
    let proof = document.validate().unwrap().conformant().unwrap();
    assert!(proof.source("empty").unwrap().data().is_none());
    assert_eq!(
        proof.source("null").unwrap().data().unwrap().kind(),
        JsonKind::Null
    );
    assert_eq!(
        proof
            .source("exact")
            .unwrap()
            .data()
            .unwrap()
            .get("n")
            .unwrap()
            .text(),
        "9007199254740993"
    );
    assert_eq!(
        proof.binding("b").unwrap().data().unwrap().text(),
        "1e999999"
    );
    for old_field in ["content", "bindingSpec"] {
        let text = format!(
            r#"{{"openbindings":"0.2.0","operations":{{}},"sources":{{"s":{{"kind":"k","{old_field}":null}}}}}}"#
        );
        assert_eq!(
            validate_document(text).unwrap().report().evidence["OBI-02"],
            Evidence::Violated
        );
    }
}

#[test]
fn document_pointers_reach_contained_schemas_but_not_primitive_schema_slots() {
    for (target, expected) in [("true", Evidence::Met), ("3", Evidence::Violated)] {
        let text = format!(
            r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"Target":{target},"Use":{{"$ref":"#/schemas/Target"}}}}}}"##
        );
        assert_eq!(
            validate_document(text).unwrap().report().evidence["OBI-12"],
            expected
        );
    }
    let text = r##"{"openbindings":"0.2.0","operations":{"go":{"input":{"$ref":"#/schemas/R/properties/x"}}},"schemas":{"R":{"$id":"https://example.test/r","properties":{"x":false}}}}"##;
    assert_eq!(
        validate_document(text).unwrap().report().conclusion,
        Conformance::Conformant
    );
}

#[test]
fn deeply_nested_relative_identifiers_and_references_remain_admissible() {
    // Each resource refers to its own anchor. Fully expanding and retaining
    // every nested URI would grow quadratically with this small source.
    let mut text = String::from(
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"R":{"$id":"https://example.test/","not":"#,
    );
    for _ in 0..4000 {
        text.push_str(r##"{"$id":"x/","$anchor":"n","$ref":"#n","not":"##);
    }
    text.push_str("true");
    text.extend(std::iter::repeat_n('}', 4003));
    let checked = validate_document(text).unwrap();
    assert_eq!(checked.report().conclusion, Conformance::Conformant);
    assert!(checked.report().findings.is_empty());
}

#[test]
fn adjudicated_historical_cases_are_rejected_at_document_admission() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/foundation-admission.json")).unwrap();
    assert_eq!(fixture["specRevision"], APPLIED_SPEC_REVISION);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let checked = validate_document(case["documentText"].as_str().unwrap()).unwrap();
        assert_eq!(
            checked.report().conclusion,
            Conformance::NonConformant,
            "{}",
            case["id"]
        );
        for rule in case["violates"].as_array().unwrap() {
            assert_eq!(
                checked.report().evidence[rule.as_str().unwrap()],
                Evidence::Violated,
                "{} {rule}",
                case["id"]
            );
        }
        assert!(checked.conformant().is_none());
    }
}

#[test]
fn duplicate_resource_and_anchor_names_never_choose_a_winner() {
    // OBI-13 does not suppress OBI-12: no unique target is identified.
    for reference in ["https://example.test/r", "https://example.test/r#/absent"] {
        let text = format!(
            r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"A":{{"$id":"https://example.test/r"}},"B":{{"$id":"https://example.test/r"}},"Use":{{"$ref":"{reference}"}}}}}}"##
        );
        let checked = validate_document(text).unwrap();
        for rule in ["OBI-12", "OBI-13"] {
            assert_eq!(checked.report().evidence[rule], Evidence::Violated);
        }
    }
    let text = r##"{"openbindings":"0.2.0","operations":{},"schemas":{"A":{"$anchor":"n"},"B":{"$anchor":"n"},"Use":{"$ref":"#n"}}}"##;
    let checked = validate_document(text).unwrap();
    for rule in ["OBI-12", "OBI-13"] {
        assert_eq!(checked.report().evidence[rule], Evidence::Violated);
    }
}

//! References through member names holding U+E000, the character the library
//! view escapes beside encoded unpaired units: each names the member written
//! with exactly the markers it encodes, in a document and in supplied
//! resources, whether or not a resource holds an unpaired unit.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;

const M: char = '\u{e000}';

fn prepare(input: &str, resources: &ResourceSet) -> Result<PreparedValueContract, NoVerdict> {
    let text = format!(
        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{input}}}}},"schemas":{{"s":{{"$defs":{{"{M}":{{"type":"integer"}}}}}}}}}}"#
    );
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), resources)
}

fn outcome(contract: &PreparedValueContract, value: &str) -> &'static str {
    match contract.validate(&JsonValue::parse(value).unwrap()) {
        ValueOutcome::Satisfies => "satisfies",
        ValueOutcome::Fails { .. } => "fails",
        ValueOutcome::Undefined { .. } => "undefined",
        ValueOutcome::NoVerdict { .. } => "no verdict",
    }
}

// Whether the contract checks values as an integer schema.
fn integer(contract: &PreparedValueContract) -> bool {
    match (outcome(contract, "5"), outcome(contract, r#""x""#)) {
        ("satisfies", "fails") => true,
        ("fails", "satisfies") => false,
        other => panic!("neither integer nor string: {other:?}"),
    }
}

#[test]
fn a_reference_names_the_member_with_the_markers_it_encodes() {
    let defs = format!(r#""{M}":{{"type":"string"}},"{M}{M}":{{"type":"integer"}}"#);
    for (reference, expected_integer) in [
        ("#/$defs/%EE%80%80", false),
        ("#/$defs/%ee%80%80", false),
        ("#/$defs/%EE%80%80%EE%80%80", true),
    ] {
        let input =
            format!(r#"{{"$id":"https://e.test/r","$defs":{{{defs}}},"$ref":"{reference}"}}"#);
        let contract = prepare(&input, &ResourceSet::default()).expect(reference);
        assert_eq!(integer(&contract), expected_integer, "{reference}");
    }
    // A name holding the marker inside other text.
    let input = format!(
        r##"{{"$id":"https://e.test/r","$defs":{{"a{M}b":{{"type":"integer"}}}},"$ref":"#/$defs/a%EE%80%80b"}}"##
    );
    assert!(integer(&prepare(&input, &ResourceSet::default()).unwrap()));
    // Through the document root, and from one embedded resource into another.
    assert!(integer(
        &prepare(
            r##"{"$ref":"#/schemas/s/$defs/%EE%80%80"}"##,
            &ResourceSet::default()
        )
        .unwrap()
    ));
    let input = format!(
        r#"{{"$id":"https://e.test/a","$defs":{{"b":{{"$id":"https://e.test/b","$defs":{{"{M}":{{"type":"integer"}}}}}}}},"$ref":"https://e.test/b#/$defs/%EE%80%80"}}"#
    );
    assert!(integer(&prepare(&input, &ResourceSet::default()).unwrap()));
}

#[test]
fn a_reference_to_an_absent_marker_name_identifies_no_schema() {
    // Only the name with two markers exists; the reference encodes one.
    let input = format!(
        r##"{{"$id":"https://e.test/r","$defs":{{"{M}{M}":{{"type":"integer"}}}},"$ref":"#/$defs/%EE%80%80"}}"##
    );
    let refused = prepare(&input, &ResourceSet::default()).unwrap_err();
    assert_eq!(refused.code, "non-schema-target", "{refused:?}");
}

#[test]
fn supplied_resources_resolve_marker_names_with_or_without_unpaired_units() {
    // Each resource defines the two names; one also holds an unpaired unit,
    // so the library view encodes units in it and not in the other.
    let resource = |uri: &str, extra: &str| {
        SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse(format!(
            r#"{{"$id":"{uri}"{extra},"$defs":{{"{M}":{{"type":"string"}},"{M}{M}":{{"type":"integer"}}}}}}"#
        ))
        .unwrap(),
    }
    };
    let resources = ResourceSet::new([
        resource("https://e.test/plain", ""),
        resource("https://e.test/unpaired", r#","$comment":"\ud800""#),
    ])
    .unwrap();
    for uri in ["https://e.test/plain", "https://e.test/unpaired"] {
        for (fragment, expected_integer) in [
            ("/$defs/%EE%80%80", false),
            ("/$defs/%EE%80%80%EE%80%80", true),
        ] {
            let input = format!(r#"{{"$ref":"{uri}#{fragment}"}}"#);
            let contract = prepare(&input, &resources).expect(&input);
            assert_eq!(integer(&contract), expected_integer, "{input}");
        }
    }
    // A contract reaching both resources resolves each the same way.
    let input = r#"{"allOf":[{"$ref":"https://e.test/plain#/$defs/%EE%80%80%EE%80%80"},{"$ref":"https://e.test/unpaired#/$defs/%EE%80%80%EE%80%80"}]}"#;
    assert!(integer(&prepare(input, &resources).unwrap()));
}

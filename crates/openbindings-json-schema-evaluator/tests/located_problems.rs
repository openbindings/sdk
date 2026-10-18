//! Explained problems locate the failing node in the value's text and the
//! failing keyword in its original source, which its holder maps to a line and
//! column on demand.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::{Arc, Mutex};

fn document(schema: &str) -> Conformant<Document> {
    let text = format!(
        "{{\n  \"openbindings\": \"0.2.0\",\n  \"operations\": {{\n    \"op\": {{\n      \"input\": {schema}\n    }}\n  }}\n}}"
    );
    Document::parse(text)
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .expect("conformant document")
}
fn contract(document: &Conformant<Document>, resources: &ResourceSet) -> PreparedContract {
    document
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), resources)
        .expect("prepared")
}
fn problems(outcome: ValueOutcome) -> Vec<ValueProblem> {
    match outcome {
        ValueOutcome::Fails { problems, .. } => problems,
        other => panic!("expected a failure, observed {other:?}"),
    }
}
// One-based line and byte column of a byte offset, counted independently.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = 1 + before.bytes().filter(|&b| b == b'\n').count();
    (line, offset - before.rfind('\n').map_or(0, |i| i + 1) + 1)
}

#[test]
fn explained_problems_locate_the_failing_node_and_the_documents_keyword() {
    let document = document(r#"{"properties": {"items": {"items": {"type": "integer"}}}}"#);
    let contract = contract(&document, &ResourceSet::default());
    let text = "{\n  \"items\": [\n    1,\n    \"two\"\n  ]\n}";
    let value = JsonValue::parse(text).unwrap();
    // The cheap check locates nothing.
    assert!(problems(contract.validate(&value)).is_empty());
    let problems = problems(contract.explain(&value));
    assert_eq!(problems.len(), 1);
    let problem = &problems[0];
    assert_eq!(problem.instance, "/items/1");
    // The value holds its text, so it finds the line and column when asked.
    let offset = text.find("\"two\"").unwrap();
    let located = value.locate(&problem.instance).unwrap();
    assert_eq!(located, value.at("/items/1").unwrap().location());
    assert_eq!(located.byte_offset, offset);
    assert_eq!((located.line, located.byte_column), position(text, offset));
    assert_eq!(
        located.to_string(),
        format!("/items/1 (line 4, column 5, byte {offset})")
    );
    // The schema side names the keyword; the document finds its line and column.
    let keyword = "/operations/op/input/properties/items/items/type";
    assert_eq!(problem.schema.resource, None);
    assert_eq!(problem.schema.pointer, keyword);
    let located = document.locate(&problem.schema).unwrap();
    let source = std::str::from_utf8(document.as_bytes()).unwrap();
    let offset = source.find("\"integer\"").unwrap();
    assert_eq!(located.pointer.as_ref().unwrap(), keyword);
    assert_eq!(located.byte_offset, offset);
    assert_eq!(
        (located.line, located.byte_column),
        position(source, offset)
    );
    assert_eq!(located, document.value().at(keyword).unwrap().location());
    // Nothing else holds it.
    assert_eq!(ResourceSet::default().locate(&problem.schema), None);
    let absent = SchemaLocation::new(None, "/operations/op/absent".parse().unwrap());
    assert_eq!(document.locate(&absent), None);
    assert_eq!(value.locate(&"/items/2".parse().unwrap()), None);
}

#[test]
fn a_root_failure_displays_its_coordinates_without_the_empty_pointer() {
    let document = document(r#"{"type": "string"}"#);
    let contract = contract(&document, &ResourceSet::default());
    let value = JsonValue::parse("  7").unwrap();
    let problems = problems(contract.explain(&value));
    assert_eq!(problems[0].instance, "");
    let located = value.locate(&problems[0].instance).unwrap();
    assert_eq!(located.to_string(), "line 1, column 3, byte 2");
}

#[test]
fn member_names_holding_the_views_marker_are_named_as_the_caller_spells_them() {
    // The evaluator's library view encodes unpaired units with U+E000; a
    // pointer names the caller's own member.
    for schema in [
        r#"{"properties": {"a\ue000b": {"type": "integer"}}}"#,
        r#"{"additionalProperties": {"type": "integer"}}"#,
        r#"{"patternProperties": {"^a": {"type": "integer"}}}"#,
    ] {
        let document = document(schema);
        let contract = contract(&document, &ResourceSet::default());
        let text = r#"{"x":1,"a\ue000b":"s"}"#;
        let value = JsonValue::parse(text).unwrap();
        let problems = problems(contract.explain(&value));
        assert_eq!(problems.len(), 1, "{schema}");
        assert_eq!(problems[0].instance, "/a\u{e000}b", "{schema}");
        assert_eq!(
            value.locate(&problems[0].instance),
            Some(value.get("a\u{e000}b").unwrap().location())
        );
    }
}

#[test]
fn failures_beneath_unpaired_names_never_report_a_library_spelling() {
    // Reaching a name with an unpaired unit leaves the evaluator no truthful
    // reading of it: a failure beneath one is established with no problem,
    // and problems elsewhere name their nodes exactly.
    for (schema, text, expected) in [
        (
            r#"{"additionalProperties": {"type": "integer"}}"#,
            r#"{"\ud800":"s"}"#,
            &[][..],
        ),
        (
            r#"{"additionalProperties": {"required": ["b"]}}"#,
            r#"{"\ud800":{"a":1}}"#,
            &[],
        ),
        (
            r#"{"properties": {"x": {"type": "integer"}}}"#,
            r#"{"\ud800":"s","x":"t"}"#,
            &["/x"],
        ),
        (r#"{"maxProperties": 0}"#, r#"{"\ud800":"s"}"#, &[""]),
    ] {
        let document = document(schema);
        let contract = contract(&document, &ResourceSet::default());
        let value = JsonValue::parse(text).unwrap();
        let ValueOutcome::Fails {
            problems,
            problems_complete: false,
            ..
        } = contract.explain(&value)
        else {
            panic!("{schema}: expected an established failure")
        };
        let pointers: Vec<_> = problems
            .iter()
            .map(|problem| {
                assert!(value.at_pointer(&problem.instance).is_some());
                problem.instance.as_str().unwrap()
            })
            .collect();
        assert_eq!(pointers, expected, "{schema}");
    }
}

#[test]
fn a_subtree_value_is_named_from_itself_and_located_in_its_text() {
    let document = document(r#"{"properties": {"n": {"type": "integer"}}}"#);
    let contract = contract(&document, &ResourceSet::default());
    let text = "{\"request\": {\n  \"n\": \"x\"\n}}";
    let whole = JsonValue::parse(text).unwrap();
    let request = whole.get("request").unwrap().to_owned();
    let problems = problems(contract.explain(&request));
    // The instance's root is the value passed, as for a value parsed alone.
    assert_eq!(problems[0].instance, "/n");
    let alone = JsonValue::parse(request.text()).unwrap();
    assert_eq!(
        serde_json::to_string(&contract.explain(&alone)).unwrap(),
        serde_json::to_string(&contract.explain(&request)).unwrap()
    );
    // Its coordinates are in the text it was parsed from.
    let offset = text.find("\"x\"").unwrap();
    let located = request.locate(&problems[0].instance).unwrap();
    assert_eq!(located.pointer.as_ref().unwrap(), "/n");
    assert_eq!(located.byte_offset, offset);
    assert_eq!((located.line, located.byte_column), position(text, offset));
}

#[test]
fn supplied_resources_locate_their_own_keywords() {
    let uri = "https://example.test/schema";
    let resource_text = "{\n  \"$id\": \"https://example.test/schema\",\n  \"minimum\": 10\n}";
    let resources = ResourceSet::new([SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse(resource_text).unwrap(),
    }])
    .unwrap();
    let document = document(&format!(r#"{{"$ref": "{uri}"}}"#));
    let contract = contract(&document, &resources);
    let problems = problems(contract.explain(&JsonValue::parse("3").unwrap()));
    let schema = &problems[0].schema;
    assert_eq!(schema.resource.as_deref(), Some(uri));
    assert_eq!(schema.pointer, "/minimum");
    let located = resources.locate(schema).unwrap();
    let offset = resource_text.find("10").unwrap();
    assert_eq!(located.pointer.as_ref().unwrap(), "/minimum");
    assert_eq!(located.byte_offset, offset);
    assert_eq!((located.line, located.byte_column), (3, 14));
    // The document holds no resource text, and a set holds no document text.
    assert_eq!(document.locate(schema), None);
    let other = SchemaLocation::new(
        Some("https://example.test/other".into()),
        "/minimum".parse().unwrap(),
    );
    assert_eq!(resources.locate(&other), None);
    let in_document = SchemaLocation::new(None, "/minimum".parse().unwrap());
    assert_eq!(resources.locate(&in_document), None);
}

#[test]
fn requests_locate_the_schema_nodes_an_evaluator_was_given() {
    #[derive(Default)]
    struct Capture(Mutex<Option<SchemaRequest>>);
    impl SchemaEvaluator for Capture {
        fn prepare(
            &self,
            request: &SchemaRequest,
            control: &dyn WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            *self.0.lock().unwrap() = Some(request.clone());
            DefaultEvaluator::new().prepare(request, control)
        }
    }
    let uri = "https://example.test/schema";
    // A resource whose member name holds an unpaired unit, which only an
    // exact pointer can name.
    let resources = ResourceSet::new([SchemaResource {
        uri: uri.into(),
        document: JsonValue::parse(r#"{"$defs":{"\ud800":{"type":"string"}}}"#).unwrap(),
    }])
    .unwrap();
    let document = document(r#"{"properties": {"a": {"type": "string"}}}"#);
    let capture = Capture::default();
    document
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&capture, &resources)
        .unwrap();
    let request = capture.0.lock().unwrap().take().unwrap();
    assert!(request.location(request.entry().view()) == Some(request.entry_location()));
    let keyword = request.entry().at("/properties/a/type").unwrap();
    let at = request.location(keyword).unwrap();
    assert_eq!(at.resource, None);
    assert_eq!(at.pointer, "/operations/op/input/properties/a/type");
    assert_eq!(document.locate(&at), Some(keyword.location()));
    let supplied = request.supplied_resources().iter().next().unwrap();
    let node = supplied
        .document
        .get("$defs")
        .unwrap()
        .members()
        .unwrap()
        .next()
        .unwrap()
        .value;
    let at = request.location(node).unwrap();
    assert_eq!(at.resource.as_deref(), Some(uri));
    assert_eq!(at.pointer.as_str(), None);
    assert_eq!(at.pointer.to_string(), "/$defs/\\ud800");
    assert_eq!(resources.locate(&at), Some(node.location()));
    // A node of another source, including a projection, has no original location.
    let foreign = JsonValue::parse(r#"{"type":"string"}"#).unwrap();
    assert_eq!(request.location(foreign.view()), None);
    let program = request
        .evaluation_program(&std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        request.location(program.resources()[0].document.view()),
        None
    );
}

// A pointer whose `\0` units stand for one unpaired unit, U+D800.
fn unpaired(text: &str) -> Pointer {
    let units: Vec<u16> = text
        .encode_utf16()
        .map(|unit| if unit == 0 { 0xd800 } else { unit })
        .collect();
    Pointer::from_utf16(&units).unwrap()
}

#[test]
fn schema_locations_beneath_unpaired_names_are_exact() {
    let uri = "https://loc.test/r";
    let anchored = format!(r#"{{"$ref": "{uri}#an"}}"#);
    for (input, resource, value, expected) in [
        (
            anchored.as_str(),
            "{\"minimum\":0,\n\"$defs\":{\"\\ud800\":{\"$anchor\":\"an\",\n\"minimum\":3}}}",
            "1",
            "/$defs/\0/minimum",
        ),
        (
            r#"{"$ref": "https://loc.test/e"}"#,
            r#"{"minimum":0,"$defs":{"\ud800":{"$id":"https://loc.test/e","minimum":3}}}"#,
            "1",
            "/$defs/\0/minimum",
        ),
        (
            anchored.as_str(),
            r#"{"$defs":{"\ud800":{"$anchor":"an","properties":{"p":{"minimum":3}}}}}"#,
            r#"{"p":1}"#,
            "/$defs/\0/properties/p/minimum",
        ),
        (
            anchored.as_str(),
            r##"{"$defs":{"\ud800":{"$anchor":"an","$ref":"#/$defs/k"},"k":{"minimum":3}}}"##,
            "1",
            "/$defs/k/minimum",
        ),
    ] {
        let resources = ResourceSet::new([SchemaResource {
            uri: uri.into(),
            document: JsonValue::parse(resource).unwrap(),
        }])
        .unwrap();
        let document = document(input);
        let contract = contract(&document, &resources);
        let problems = problems(contract.explain(&JsonValue::parse(value).unwrap()));
        let expected = unpaired(expected);
        assert_eq!(
            problems[0].schema,
            SchemaLocation::new(Some(uri.into()), expected.clone()),
            "{resource}"
        );
        // The set finds the failing keyword itself, `minimum: 3`.
        let supplied = &resources.iter().next().unwrap().document;
        let keyword = supplied.at_pointer(&expected).unwrap();
        assert_eq!(keyword.text(), "3");
        let located = resources.locate(&problems[0].schema).unwrap();
        assert_eq!(located, keyword.location());
    }
}

#[test]
fn draft_references_beneath_unpaired_names_are_exact() {
    let draft = Document::parse(
        r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"\ud800":{"properties":{"q":{"$ref":"#/missing"}}}}}}}}"##,
    )
    .unwrap();
    let report = draft.references().unwrap();
    let reference = &report.references[0];
    let at = unpaired("/operations/op/input/properties/\0/properties/q/$ref");
    assert_eq!(reference.location, SchemaLocation::new(None, at.clone()));
    assert_eq!(
        draft.value().at_pointer(&at).unwrap().text(),
        r##""#/missing""##
    );
    let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
        panic!("expected an unresolved reference")
    };
    // The refusal names an exact node of the draft, never an invented root.
    let location = detail.location.as_ref().unwrap();
    assert_eq!(location.resource, None);
    assert!(location.pointer.as_str().is_none());
    assert!(draft.value().at_pointer(&location.pointer).is_some());
}

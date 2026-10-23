//! Owned and borrowed input admit and judge every document alike: the owned
//! entry points take the caller's buffer as the document's text instead of
//! copying it, and change nothing else.
use openbindings::*;

fn inputs() -> Vec<Vec<u8>> {
    let mut inputs: Vec<Vec<u8>> = [
        r#"{"openbindings":"0.2.0","operations":{}}"#,
        " \n {\"openbindings\":\"0.2.0\",\"operations\":{\"run\":{\"input\":{\"type\":\"integer\"}}}}\t\n",
        r#"{"openbindings":"0.2.0","operations":{"op!":{}},"x":{"a":1,"a":2}}"#,
        r#"{"openbindings":"0.2.0","operations":{"ok":{"input":{"type":7}}}}"#,
        r#"{"openbindings":"0.2.0","operations":{},"name":"caf\u00e9 \ud834\udd1e","x-u":"\ud800"}"#,
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"s":{"$id":"s://a/q/a/../b/c/","allOf":[{"$id":"s://b/x/../y/?#f"}]}}}"#,
        r#"{"openbindings":"9.0.0","operations":{}}"#,
        r#"{"openbindings":"0.2.0-beta","operations":{}}"#,
        r#"{"operations":{}}"#,
        r#"[1,2,3]"#,
        "7",
        "",
        "{",
        r#"{"a" "#,
        "[1 2]",
        "{\"openbindings\":\"0.2.0\",\n\"operations\":{}} x",
        r#""\q""#,
    ]
    .iter()
    .map(|text| text.as_bytes().to_vec())
    .collect();
    inputs.push(b"\xef\xbb\xbf{}".to_vec());
    inputs.push(b"{\"openbindings\":\"0.2.0\",\"x\":\"\xc3\"}".to_vec());
    inputs.push(b"\xff".to_vec());
    let deep = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x":{}0{}}}"#,
        "[".repeat(40),
        "]".repeat(40)
    );
    inputs.push(deep.into_bytes());
    let wide = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x":[{}]}}"#,
        vec!["0"; 200].join(",")
    );
    inputs.push(wide.into_bytes());
    inputs
}

fn limits() -> Vec<JsonLimits> {
    let default = JsonLimits::default();
    vec![
        default,
        JsonLimits {
            max_depth: usize::MAX,
            ..default
        },
        JsonLimits {
            max_bytes: 48,
            ..default
        },
        JsonLimits {
            max_nodes: 20,
            ..default
        },
        JsonLimits {
            max_depth: 3,
            ..default
        },
    ]
}

// Everything a validation shows: the report, version refusal, and the document
// it keeps, with the limits it keeps them under.
type Validation = Result<(ValidationReport, Option<(Vec<u8>, JsonLimits)>, bool), VersionRefusal>;
fn validation(result: Result<DocumentValidation, VersionRefusal>) -> Validation {
    result.map(|validation| {
        (
            validation.report().clone(),
            validation
                .document()
                .map(|document| (document.as_bytes().to_vec(), document.limits())),
            validation.conformant().is_some(),
        )
    })
}
type Parse = Result<(Vec<u8>, JsonLimits), (InputError, String)>;
fn parse(result: Result<Document, InputError>) -> Parse {
    result
        .map(|document| (document.as_bytes().to_vec(), document.limits()))
        .map_err(|error| {
            let message = error.to_string();
            (error, message)
        })
}

#[test]
fn owned_and_borrowed_validation_agree() {
    for input in inputs() {
        assert_eq!(
            validation(validate_document_owned(input.clone())),
            validation(validate_document(&input)),
            "{input:?}"
        );
        for limits in limits() {
            assert_eq!(
                validation(validate_document_owned_with_limits(input.clone(), limits)),
                validation(validate_document_with_limits(&input, limits)),
                "{input:?} {limits:?}"
            );
        }
    }
}

#[test]
fn owned_and_borrowed_parsing_agree() {
    for input in inputs() {
        assert_eq!(
            parse(Document::parse_owned(input.clone())),
            parse(Document::parse(&input)),
            "{input:?}"
        );
        for limits in limits() {
            assert_eq!(
                parse(Document::parse_owned_with_limits(input.clone(), limits)),
                parse(Document::parse_with_limits(&input, limits)),
                "{input:?} {limits:?}"
            );
            let value = |result: Result<JsonValue, InputError>| {
                result.map(|value| (value.text().to_owned(), value.original_source().to_vec()))
            };
            assert_eq!(
                value(JsonValue::parse_owned_with_limits(input.clone(), limits)),
                value(JsonValue::parse_with_limits(&input, limits)),
                "{input:?} {limits:?}"
            );
        }
    }
}

#[test]
fn owned_input_becomes_the_text_without_a_copy() {
    let text = r#"{"openbindings":"0.2.0","operations":{"run":{}}}"#;
    let owned = || text.as_bytes().to_vec();
    let bytes = owned();
    let at = bytes.as_ptr();
    assert_eq!(
        Document::parse_owned(bytes).unwrap().as_bytes().as_ptr(),
        at
    );
    let bytes = owned();
    let at = bytes.as_ptr();
    let validation = validate_document_owned(bytes).unwrap();
    assert_eq!(validation.document().unwrap().as_bytes().as_ptr(), at);
    assert_eq!(validation.conformant().unwrap().as_bytes().as_ptr(), at);
    let bytes = owned();
    let at = bytes.as_ptr();
    let value = JsonValue::parse_owned_with_limits(bytes, JsonLimits::default()).unwrap();
    assert_eq!(value.original_source().as_ptr(), at);
}

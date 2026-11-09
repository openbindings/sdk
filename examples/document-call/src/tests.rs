use super::*;

#[tokio::test]
async fn typed_success_and_caller_failures() {
    assert_eq!(
        call(DOCUMENT, r#"{"count":7}"#).await.unwrap(),
        Reply { count: 7 }
    );
    for (document, input, expected) in [
        ("{", "null", "JSON admission"),
        ("{}", "null", "document conformance"),
        (DOCUMENT, "{", "input JSON admission"),
        (
            DOCUMENT,
            r#"{"count":"seven"}"#,
            "ERR_OPERATION_VALIDATION_FAILED",
        ),
        (DOCUMENT, r#"{"count":70000}"#, "typed output decoding"),
    ] {
        assert!(
            call(document, input)
                .await
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[tokio::test]
async fn selection_provider_and_output_failures() {
    for (document, expected) in [
        (DOCUMENT.replace("0.2.0", "0.3.0"), "unsupported version"),
        (
            DOCUMENT.replace("repeat", "another-alias"),
            "operation is missing",
        ),
        (
            DOCUMENT.replace("#/operations/echo/input", "https://missing.example/schema"),
            "ERR_SCHEMA_UNRESOLVED",
        ),
        (
            DOCUMENT.replace("example.echo@1", "example.other@1"),
            "ERR_BINDING_NOT_FOUND",
        ),
        (
            DOCUMENT.replace(
                r#""kind":"example.echo@1""#,
                r#""kind":"example.echo@1","data":null"#,
            ),
            "ERR_REFUSED",
        ),
        (
            DOCUMENT.replace(
                r##""output":{"$ref":"#/operations/echo/input"}"##,
                r#""output":false"#,
            ),
            "ERR_OPERATION_VALIDATION_FAILED",
        ),
    ] {
        assert!(
            call(&document, r#"{"count":7}"#)
                .await
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

//! Debug is structural. Explicit display/accessors/serialization retain payloads.
use openbindings_internal_json::*;
use std::fmt::Debug;
// A credential-shaped probe value; Debug output must never contain it.
const SECRET: &str = "PROBE-BEARER-TOKEN-7391";
const NUMBER: &str = "739173917391";

struct Row {
    label: &'static str,
    text: [String; 4],
}
fn row(label: &'static str, value: &(impl Debug + ?Sized)) -> Row {
    Row {
        label,
        text: [
            format!("{value:?}"),
            format!("{value:#?}"),
            format!("{:?}", Some(value)),
            format!("{:#?}", Some(value)),
        ],
    }
}

fn check_rows(rows: Vec<Row>, expected: usize) {
    assert_eq!(rows.len(), expected);
    for Row { label, text } in &rows {
        for (mode, text) in ["ordinary", "pretty", "nested", "nested pretty"]
            .into_iter()
            .zip(text)
        {
            for marker in [SECRET, NUMBER] {
                assert!(!text.contains(marker), "{label}/{mode} leaked: {text}");
            }
        }
    }
    println!("{} redaction rows, four renderings each", rows.len());
}

#[test]
fn debug_redacts_caller_text_at_every_boundary() {
    let context = JsonValue::parse(
        serde_json::json!({"authorization":format!("Bearer {SECRET}"), SECRET: SECRET}).to_string(),
    )
    .unwrap();
    // Records reach callers built; reassigning public fields probes caller text.
    let mut source_location = context.location();
    source_location.pointer = Some(format!("/{SECRET}").parse().unwrap());
    source_location.byte_offset = 42;
    source_location.line = 2;
    source_location.byte_column = 3;
    let mut secret_error = JsonValue::parse("]").unwrap_err();
    secret_error.code = SECRET;
    secret_error.byte_offset = 42;
    let mut custom_prefix = JsonValue::parse_with_limits(
        "[0]",
        JsonLimits {
            max_bytes: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    custom_prefix.code = "byte-limit-PROBE-BEARER-TOKEN-7391";
    let mut rows = vec![
        row("JsonValue", &context),
        row("JsonRef", &context.view()),
        row(
            "JsonMember",
            &context.view().members().unwrap().next().unwrap(),
        ),
        row(
            "JsonStringRef",
            &context.view().get(SECRET).unwrap().as_string().unwrap(),
        ),
        row("SourceLocation", &source_location),
        row("Pointer", source_location.pointer.as_ref().unwrap()),
        row("InputError", &secret_error),
        row(
            "Decimal",
            &openbindings_internal_json::decimal::Decimal::parse(NUMBER),
        ),
    ];

    // Exercise every current parser reason with malformed input, not constructed codes.
    for (input, code) in [
        (b"\xff".as_slice(), "utf8"),
        (b"\xef\xbb\xbf{}".as_slice(), "bom"),
        (b"[0 1]".as_slice(), "comma-or-close"),
        (b"{1:0}".as_slice(), "object-key"),
        (b"{\"a\" 0}".as_slice(), "colon"),
        (b"0 0".as_slice(), "trailing-token"),
        (b"-x".as_slice(), "number"),
        (b"1.".as_slice(), "fraction"),
        (b"1e".as_slice(), "exponent"),
        (b"]".as_slice(), "value"),
        (b"nulx".as_slice(), "literal"),
        (b"\"".as_slice(), "unterminated-string"),
        (b"\"\x00\"".as_slice(), "string-control"),
        (b"\"\\q\"".as_slice(), "escape"),
        (b"\"\\uGGGG\"".as_slice(), "unicode-escape"),
    ] {
        record_error(&mut rows, JsonValue::parse(input).unwrap_err(), code);
    }
    // Limit probes for each JSON limit; byte and depth failures share offset zero.
    for (code, limits) in [
        (
            "byte-limit",
            JsonLimits {
                max_bytes: 0,
                ..Default::default()
            },
        ),
        (
            "depth-limit",
            JsonLimits {
                max_depth: 0,
                ..Default::default()
            },
        ),
        (
            "node-limit",
            JsonLimits {
                max_nodes: 0,
                ..Default::default()
            },
        ),
    ] {
        record_error(
            &mut rows,
            JsonValue::parse_with_limits("[0]", limits).unwrap_err(),
            code,
        );
    }
    assert_eq!(
        (secret_error.kind.clone(), custom_prefix.kind.clone()),
        (InputErrorKind::Syntax, InputErrorKind::Limit)
    );
    rows.push(row_with_fields(
        "InputError custom prefix",
        &custom_prefix,
        &[("code", "<redacted>")],
    ));

    check_rows(rows, 45);
}

fn row_with_fields(label: &'static str, value: &impl Debug, fields: &[(&str, &str)]) -> Row {
    let item = row(label, value);
    for (field, expected) in fields {
        let expected = format!("{field}: {expected:?}");
        assert!(
            item.text.iter().all(|text| text.contains(&expected)),
            "{label}: missing {expected}"
        );
    }
    item
}

fn record_error(rows: &mut Vec<Row>, mut error: InputError, code: &str) {
    assert_eq!(error.code, code);
    let item = row_with_fields("InputError actual reason", &error, &[("code", code)]);
    let kind = format!("kind: {:?}", error.kind);
    assert!(item.text.iter().all(|text| text.contains(&kind)));
    rows.push(item);
    error.code = SECRET;
    rows.push(row_with_fields(
        "InputError replaced reason",
        &error,
        &[("code", "<redacted>")],
    ));
}

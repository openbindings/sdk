use openbindings::{InputError, InputErrorKind, JsonLimits, JsonPrefixError, JsonValue};

#[test]
fn final_input_uses_whole_text_admission_after_incomplete() {
    let mut cases = vec![b"".to_vec(), b" \n\t".to_vec(), b"\"\xf0\x9f".to_vec()];
    for text in [
        "null",
        "true",
        "false",
        "-12.345e+678",
        "0",
        "-0",
        "12",
        "12.00",
        "1e99",
        "-1E-9999",
        r#""plain café 😀""#,
        r#""\"\\\/\b\f\n\r\t\u0041\ud834\udd1e\ud800x\udc00""#,
        r#"[null,true,false,-12.3e-4,"é",[{}]]"#,
        r#"{"a":1,"a":2,"\ud800":"\udfff","nested":{"k":[false]}}"#,
        "x",
        "nulx",
        "trux",
        "falsx",
        "--",
        "1.x",
        "1e+x",
        r#""\q""#,
        r#""\u0z""#,
        "[}",
        "[1,]",
        "{x",
        "{\"a\"x",
        "{\"a\":}",
        "\"\n",
    ] {
        for end in 0..=text.len() {
            cases.push(text.as_bytes()[..end].to_vec());
        }
    }
    let limits = JsonLimits::default();
    for input in cases {
        // This fixture matrix matched the former explicit-EOF route before
        // its removal; retain the same exact values and refusal coordinates.
        let expected = JsonValue::parse_with_limits(&input, limits)
            .map(|value| (value, input.len()))
            .map_err(JsonPrefixError::Invalid);
        let actual = final_prefix(&input, limits);
        let evidence = |result: Result<(JsonValue, usize), JsonPrefixError>| {
            result.map(|(value, end)| {
                (
                    value.text().to_owned(),
                    value.original_source().to_vec(),
                    end,
                )
            })
        };
        assert_eq!(evidence(actual), evidence(expected));
    }
}

fn final_prefix(
    input: impl AsRef<[u8]>,
    limits: JsonLimits,
) -> Result<(JsonValue, usize), JsonPrefixError> {
    let input = input.as_ref();
    match JsonValue::parse_prefix_with_limits(input, limits) {
        Err(JsonPrefixError::Incomplete) => JsonValue::parse_with_limits(input, limits)
            .map(|value| (value, input.len()))
            .map_err(JsonPrefixError::Invalid),
        result => result,
    }
}
fn invalid(input: &[u8]) -> InputError {
    match JsonValue::parse_prefix(input).unwrap_err() {
        JsonPrefixError::Invalid(error) => error,
        other => panic!("expected invalid input, got {other:?}"),
    }
}

#[test]
fn every_token_kind_and_every_internal_chunk_boundary() {
    for text in [
        "null",
        "true",
        "false",
        "-12.345e+678",
        r#""plain café 😀""#,
        r#""\"\\\/\b\f\n\r\t\u0041\ud834\udd1e\ud800x\udc00""#,
        r#"[null,true,false,-12.3e-4,"é",[{}]]"#,
        r#"{"a":1,"a":2,"\ud800":"\udfff","nested":{"k":[false]}}"#,
    ] {
        let bytes = text.as_bytes();
        for cut in 0..bytes.len() {
            assert_eq!(
                JsonValue::parse_prefix(&bytes[..cut]).unwrap_err(),
                JsonPrefixError::Incomplete,
                "token kind {:?}, cut {cut}",
                JsonValue::parse(text).unwrap().kind(),
            );
            // Appending the next chunk to the retained buffer recovers the
            // exact source, including escapes, duplicates and number spelling.
            let mut accumulated = bytes[..cut].to_vec();
            accumulated.extend_from_slice(&bytes[cut..]);
            let (value, end) = final_prefix(&accumulated, JsonLimits::default()).unwrap();
            assert_eq!(value.text(), text);
            assert_eq!(end, bytes.len());
            assert_eq!(value.original_source(), bytes);
        }
    }
}

#[test]
fn numeric_lookahead_and_whole_text_eof_fallback() {
    for number in ["0", "-0", "12", "12.00", "1e99", "-1E-9999"] {
        assert_eq!(
            JsonValue::parse_prefix(number).unwrap_err(),
            JsonPrefixError::Incomplete
        );
        let input = format!("{number} x");
        let (value, end) = JsonValue::parse_prefix(input).unwrap();
        assert_eq!((value.text(), end), (number, number.len()));
        assert_eq!(
            final_prefix(number, JsonLimits::default()).unwrap().1,
            number.len()
        );
    }
    for text in ["null", "true", "false", "\"s\"", "[]", "{}"] {
        assert_eq!(JsonValue::parse_prefix(text).unwrap().1, text.len());
    }
    for text in [
        "", " \n\t", "n", "tru", "fals", "-", "1.", "1e", "1e+", "\"", "[", "{\"k\":",
    ] {
        assert_eq!(
            JsonValue::parse_prefix(text).unwrap_err(),
            JsonPrefixError::Incomplete
        );
        assert_eq!(
            JsonValue::parse(text).unwrap_err().kind(),
            InputErrorKind::Syntax
        );
    }
}

#[test]
fn adjacent_values_have_no_sdk_separator_rule() {
    let mut input = br#"{}[]"a""b"truefalse null 12-3 01"#.as_slice();
    for expected in [
        "{}", "[]", "\"a\"", "\"b\"", "true", "false", "null", "12", "-3", "0", "1",
    ] {
        let (value, end) = final_prefix(input, JsonLimits::default()).unwrap();
        assert_eq!(value.text(), expected);
        input = &input[end..];
    }
    assert!(input.is_empty());
    assert_eq!(
        JsonValue::parse("{}[]").unwrap_err().code(),
        "trailing-token"
    );
}

#[test]
fn offsets_whitespace_and_locations_are_buffer_relative() {
    let input = b" \n\t{\"a\": [12]} \r\n[]";
    let (value, end) = JsonValue::parse_prefix(input).unwrap();
    assert_eq!(&input[end..], b" \r\n[]");
    assert_eq!(value.original_source(), &input[..end]);
    let location = value.locate(&"/a/0".parse().unwrap()).unwrap();
    assert_eq!(
        (location.byte_offset, location.line, location.byte_column),
        (10, 2, 9)
    );
    let error = invalid(b" \n[1 x]");
    assert_eq!(
        (
            error.code(),
            error.byte_offset(),
            error.line(),
            error.byte_column()
        ),
        ("comma-or-close", 5, 2, 4)
    );
}

#[test]
fn malformed_input_is_invalid_without_waiting_for_eof() {
    for text in [
        "x",
        "nulx",
        "trux",
        "falsx",
        "--",
        "1.x",
        "1e+x",
        r#""\q""#,
        r#""\u0z""#,
        "[}",
        "[1,]",
        "{x",
        "{\"a\"x",
        "{\"a\":}",
        "\"\n",
    ] {
        assert_eq!(invalid(text.as_bytes()).kind(), InputErrorKind::Syntax);
    }
    assert_eq!(
        invalid(b"\xef\xbb\xbf{}").kind(),
        InputErrorKind::ByteOrderMark
    );
}

#[test]
fn invalid_utf8_after_a_complete_value_is_deferred() {
    for text in ["12", "true", "\"é\"", "{}", "[]"] {
        for suffix in [b"\xff".as_slice(), b"\xf0\x9f", b" \xff"] {
            let mut input = text.as_bytes().to_vec();
            input.extend_from_slice(suffix);
            let (value, end) = JsonValue::parse_prefix(&input).unwrap();
            assert_eq!((value.text(), end), (text, text.len()));
            assert_eq!(
                JsonValue::parse(&input[end..]).unwrap_err().kind(),
                InputErrorKind::InvalidUtf8
            );
        }
    }
    assert_eq!(
        JsonValue::parse_prefix(b"\"\xf0\x9f").unwrap_err(),
        JsonPrefixError::Incomplete
    );
    for bytes in [b"\"\xff".as_slice(), b"tru\xc3", b"\xc3", b"\"\\\xc3"] {
        assert_eq!(invalid(bytes).kind(), InputErrorKind::InvalidUtf8);
    }
    assert_eq!(
        JsonValue::parse(b"\"\xf0\x9f").unwrap_err().kind(),
        InputErrorKind::InvalidUtf8
    );
}

#[test]
fn limits_match_exact_admission_and_apply_to_the_first_prefix() {
    let defaults = JsonLimits::default();
    for text in ["null", "12", "\"é\"", "[1]", "{\"k\":1}", "[[[]]]"] {
        for limits in [
            JsonLimits {
                max_bytes: text.len() - 1,
                ..defaults
            },
            JsonLimits {
                max_depth: 0,
                ..defaults
            },
            JsonLimits {
                max_nodes: 0,
                ..defaults
            },
            JsonLimits {
                max_nodes: 2,
                ..defaults
            },
            defaults,
        ] {
            let expected = JsonValue::parse_with_limits(text, limits);
            let actual = final_prefix(text, limits);
            match (expected, actual) {
                (Ok(value), Ok((prefix, end))) => {
                    assert_eq!(prefix.text(), value.text());
                    assert_eq!(end, text.len());
                }
                (Err(error), Err(JsonPrefixError::Invalid(prefix))) => {
                    assert_eq!((prefix.kind(), prefix.code()), (error.kind(), error.code()));
                }
                other => panic!("admission disagreement: {other:?}"),
            }
        }
    }
    for text in ["12", "\"é\"", "[1]", "{\"k\":1}"] {
        let limits = JsonLimits {
            max_bytes: text.len(),
            ..defaults
        };
        let input = format!("{text} {}", " ".repeat(1024));
        let (value, end) = JsonValue::parse_prefix_with_limits(input, limits).unwrap();
        assert_eq!((value.text(), end), (text, text.len()));
    }
    let limits = JsonLimits {
        max_bytes: 4,
        ..defaults
    };
    for input in ["12345", "\"abcd", "     ", "\"éé"] {
        let error = JsonValue::parse_prefix_with_limits(input, limits).unwrap_err();
        let JsonPrefixError::Invalid(error) = error else {
            panic!("expected byte limit")
        };
        assert_eq!(
            (error.kind(), error.code(), error.byte_offset()),
            (InputErrorKind::Limit, "byte-limit", 4)
        );
    }
    let error = JsonValue::parse_prefix_with_limits(
        "  12 ",
        JsonLimits {
            max_bytes: 3,
            ..defaults
        },
    )
    .unwrap_err();
    assert!(matches!(error, JsonPrefixError::Invalid(error) if error.code() == "byte-limit"));
    let deep = format!("{}0{}", "[".repeat(10_001), "]".repeat(10_001));
    let error = final_prefix(&deep, JsonLimits::default()).unwrap_err();
    assert!(matches!(error, JsonPrefixError::Invalid(error) if error.code() == "depth-limit"));
    let (value, end) = final_prefix(
        &deep,
        JsonLimits {
            max_depth: 10_001,
            ..defaults
        },
    )
    .unwrap();
    assert_eq!(value.text(), deep);
    assert_eq!(end, deep.len());
}

#[test]
fn prefix_diagnostics_are_redacted_and_keep_the_input_cause() {
    let secret = "private-token";
    let error = JsonValue::parse_prefix(format!("{{\"{secret}\":x}}")).unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));
    assert!(std::error::Error::source(&error).is_some());
    assert!(std::error::Error::source(&JsonPrefixError::Incomplete).is_none());
}

#[test]
fn indexed_members_duplicates_and_retained_children_match_whole_value_parsing() {
    let mut members = (0..64)
        .map(|index| format!(r#""common-{index:03}":{{"n":{index}}}"#))
        .collect::<Vec<_>>();
    members.extend([
        r#""common-001":7"#.to_owned(),
        r#""\ud800":"\udfff""#.to_owned(),
        r#""common-\u0030\u0030\u0032":8"#.to_owned(),
    ]);
    let text = format!("{{{}}}", members.join(","));
    let whole = JsonValue::parse(&text).unwrap();
    let input = format!(" \n{text} []");
    let (prefix, end) = JsonValue::parse_prefix(input).unwrap();
    assert_eq!(end, text.len() + 2);
    assert!(prefix.has_duplicate_names());
    assert_eq!(
        prefix
            .view()
            .members()
            .unwrap()
            .map(|member| (member.name.text(), member.value.text()))
            .collect::<Vec<_>>(),
        whole
            .view()
            .members()
            .unwrap()
            .map(|member| (member.name.text(), member.value.text()))
            .collect::<Vec<_>>(),
    );
    for index in 0..64 {
        let name = format!("common-{index:03}");
        assert_eq!(
            prefix.get(&name).unwrap().text(),
            whole.get(&name).unwrap().text()
        );
    }
    let child = prefix.at("/common-002/n").unwrap().to_owned();
    drop(prefix);
    assert_eq!(child.text(), "2");
    assert_eq!(child.original_source().len(), end);
}

#[test]
fn prefix_admission_and_destruction_keep_the_iterative_stack_bound() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let input = format!("{}0{}[]", "[".repeat(12_000), "]".repeat(12_000));
            let error = JsonValue::parse_prefix(&input).unwrap_err();
            assert!(
                matches!(error, JsonPrefixError::Invalid(error) if error.code() == "depth-limit")
            );
            let limits = JsonLimits {
                max_depth: 12_000,
                ..JsonLimits::default()
            };
            let (value, end) = JsonValue::parse_prefix_with_limits(&input, limits).unwrap();
            assert_eq!(end, input.len() - 2);
            drop(value);
        })
        .unwrap()
        .join()
        .unwrap();
}

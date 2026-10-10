use openbindings::*;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{collections::BTreeMap, sync::Arc};
fn parsed(operations: &str) -> ParsedDocument {
    ParsedDocument::parse(format!(
        r#"{{"openbindings":"0.2.0","operations":{operations}}}"#
    ))
    .unwrap()
}
#[test]
fn typed_inspection_checks_the_namespace_and_retains_unrelated_metadata() {
    let doc = parsed(
        r#"{"z":{"description":"last","aliases":["go"],"input":false,"output":null},"a":{}}"#,
    );
    let keys = doc
        .operations()
        .unwrap()
        .iter()
        .map(|v| v.key().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "z"]);
    let OperationSelection::Found(op) = doc.resolve_operation("go").unwrap() else {
        panic!()
    };
    drop(doc);
    assert_eq!(op.key(), "z");
    assert_eq!(op.description().unwrap(), Some("last"));
    assert_eq!(op.aliases().unwrap().unwrap().collect::<Vec<_>>(), ["go"]);
    assert_eq!(op.input().unwrap().as_bool(), Some(false));
    assert_eq!(op.output().unwrap().kind(), JsonKind::Null);
    assert_eq!(
        op.input().unwrap().location().pointer.as_deref(),
        Some("/operations/z/input")
    );
    let held = op.input().unwrap().to_owned();
    drop(op);
    assert_eq!(held.text(), "false");
    let unrelated = parsed(r#"{"good":{"description":12},"bad":{}}"#);
    let OperationSelection::Found(good) = unrelated.resolve_operation("good").unwrap() else {
        panic!()
    };
    assert_eq!(
        good.description()
            .unwrap_err()
            .source_location()
            .unwrap()
            .pointer
            .as_deref(),
        Some("/operations/good/description")
    );
    assert!(unrelated.operations().is_ok());
    let malformed = parsed(r#"{"good":{"description":12},"bad":false}"#);
    for name in ["good", "bad", "absent"] {
        assert_eq!(
            malformed.resolve_operation(name).unwrap_err().code(),
            "invalid-operation-object"
        );
    }
    assert!(matches!(
        unrelated.resolve_operation("absent").unwrap(),
        OperationSelection::Missing
    ));
    for operations in ["null", "[]", "false"] {
        assert!(parsed(operations).operations().is_err());
    }
    assert!(
        ParsedDocument::parse(r#"{"openbindings":"0.2.0"}"#)
            .unwrap()
            .resolve_operation("x")
            .is_err()
    );
}
#[test]
fn namespace_counts_occurrences_and_refuses_malformed_aliases_everywhere() {
    for (ops, name, expected) in [
        (r#"{"a":{"aliases":["x","x"]}}"#, "x", vec!["a"]),
        (r#"{"a":{"aliases":["a"]}}"#, "a", vec!["a"]),
        (
            r#"{"z":{"aliases":["x"]},"a":{"aliases":["x"]}}"#,
            "x",
            vec!["a", "z"],
        ),
    ] {
        let OperationSelection::Ambiguous { candidates } =
            parsed(ops).resolve_operation(name).unwrap()
        else {
            panic!()
        };
        assert_eq!(candidates, expected);
    }
    for aliases in ["null", "false", r#"["valid",2]"#] {
        let doc = parsed(&format!(r#"{{"good":{{}},"bad":{{"aliases":{aliases}}}}}"#));
        assert!(doc.resolve_operation("good").is_err());
        assert!(doc.operations().is_err());
    }
}
#[test]
fn authoring_collision_has_native_draft_path_and_source_is_separate() {
    let mut b = DocumentBuilder::new();
    let mut op = Operation::default();
    let mut ex = OperationExample::default();
    ex.additional_fields
        .insert("input".into(), JsonValue::null());
    op.examples = Some(BTreeMap::from([("a~/b".into(), ex)]));
    b.operations.insert("x~/y".into(), op);
    let e = b.to_json().unwrap_err();
    assert_eq!(e.kind(), AuthoringErrorKind::FieldCollision);
    assert_eq!(
        e.draft_pointer(),
        Some("/operations/x~0~1y/examples/a~0~1b/additional_fields/input")
    );
    assert!(e.source_location().is_none());
    let wrong =
        JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{"x":{"aliases":false}}}"#)
            .unwrap();
    let e = DocumentBuilder::from_json(&wrong).unwrap_err();
    assert_eq!(e.kind(), AuthoringErrorKind::InvalidField);
    assert!(e.draft_pointer().is_none());
    assert_eq!(
        e.source_location().unwrap().pointer.as_deref(),
        Some("/operations/x/aliases")
    );
    let mut b = DocumentBuilder::new();
    b.additional_fields
        .insert("unknown".into(), JsonValue::null());
    assert_eq!(
        b.build().unwrap().assess().unwrap().report().conclusion,
        Conformance::NonConformant
    );
    let op = Operation {
        additional_fields: BTreeMap::from([("input".into(), JsonValue::null())]),
        ..Default::default()
    };
    let mut b = DocumentBuilder::new();
    b.operations.insert("x".repeat(5000), op);
    let e = b.build().unwrap_err();
    assert!(e.draft_pointer().is_none());
    assert!(e.path_omitted_for_limit());
}
#[test]
fn errors_are_owned_send_sync_without_bounds_on_serialized_values() {
    fn bound<T: std::error::Error + Send + Sync + 'static>() {}
    bound::<AuthoringError>();
    bound::<InterpretationError>();
    bound::<ValueConversionError>();
    let local = std::rc::Rc::new(std::cell::Cell::new(7));
    struct Local<'a>(&'a std::cell::Cell<i32>);
    impl Serialize for Local<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_i32(self.0.get())
        }
    }
    assert_eq!(
        JsonValue::from_serializable(&Local(&local)).unwrap().text(),
        "7"
    );
}
#[test]
fn serde_profile_preserves_numbers_enums_unicode_and_refuses_loss() {
    #[derive(Serialize)]
    enum Choice {
        Unit,
        New(u128),
        Tuple(bool, char),
        Object { signed: i128 },
    }
    for (v, expected) in [
        (Choice::Unit, r#""Unit""#.to_owned()),
        (
            Choice::New(u128::MAX),
            format!(r#"{{"New":{}}}"#, u128::MAX),
        ),
        (Choice::Tuple(true, '☃'), r#"{"Tuple":[true,"☃"]}"#.into()),
        (
            Choice::Object { signed: i128::MIN },
            format!(r#"{{"Object":{{"signed":{}}}}}"#, i128::MIN),
        ),
    ] {
        assert_eq!(
            JsonValue::from_serializable(&v)
                .unwrap()
                .semantic_eq(&JsonValue::parse(expected).unwrap()),
            Some(true)
        );
    }
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            JsonValue::from_serializable(&n).unwrap_err().kind(),
            ValueConversionErrorKind::NonFiniteNumber
        );
    }
    let e = JsonValue::from_serializable(&BTreeMap::from([(1u32, true)])).unwrap_err();
    assert_eq!(e.kind(), ValueConversionErrorKind::NonStringKey);
    let exact = JsonValue::parse("1e999999999999999999999999").unwrap();
    assert_eq!(
        JsonValue::from_serializable(&exact).unwrap_err().kind(),
        ValueConversionErrorKind::UnsupportedRepresentation
    );
    assert_eq!(exact.text(), "1e999999999999999999999999");
    let mut draft = DocumentBuilder::new();
    draft
        .additional_fields
        .insert("x-number".into(), exact.clone());
    assert_eq!(
        draft
            .build()
            .unwrap()
            .value()
            .get("x-number")
            .unwrap()
            .text(),
        exact.text()
    );
}
#[test]
fn emitted_limits_duplicates_and_diagnostics_are_bounded_and_located() {
    struct Duplicate;
    impl Serialize for Duplicate {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut map = s.serialize_map(Some(usize::MAX))?;
            map.serialize_entry("x~/y", &1)?;
            map.serialize_entry("x~/y", &2)?;
            map.end()
        }
    }
    let e = JsonValue::from_serializable(&Duplicate).unwrap_err();
    assert_eq!(e.kind(), ValueConversionErrorKind::DuplicateKey);
    assert_eq!(e.pointer(), Some("/x~0~1y"));
    let nested = BTreeMap::from([("x~/y", vec![f64::INFINITY])]);
    let e = JsonValue::from_serializable(&nested).unwrap_err();
    assert_eq!(e.pointer(), Some("/x~0~1y/0"));
    for limits in [
        JsonLimits {
            max_bytes: 4,
            ..Default::default()
        },
        JsonLimits {
            max_nodes: 2,
            ..Default::default()
        },
        JsonLimits {
            max_depth: 0,
            ..Default::default()
        },
    ] {
        assert_eq!(
            JsonValue::from_serializable_with_limits(&vec![1, 2], limits)
                .unwrap_err()
                .kind(),
            ValueConversionErrorKind::Limit
        );
    }
    assert!(
        JsonValue::from_serializable_with_limits(
            &vec![1, 2],
            JsonLimits {
                max_bytes: 5,
                max_nodes: 3,
                max_depth: 1
            }
        )
        .is_ok()
    );
    struct Failed;
    impl Serialize for Failed {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("private".repeat(2000)))
        }
    }
    let e =
        JsonValue::from_serializable(&BTreeMap::from([("x".repeat(5000), Failed)])).unwrap_err();
    assert_eq!(e.message().len(), 4096);
    assert!(e.message_truncated());
    assert!(e.pointer().is_none());
    assert!(e.path_omitted_for_limit());
    assert!(!e.to_string().contains("private"));
    assert!(!format!("{e:?}").contains("private"));
    assert!(e.message().starts_with("private"));
    // Output work stops at admission: the second array item is never entered.
    struct Count(Arc<std::sync::atomic::AtomicUsize>);
    impl Serialize for Count {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            s.serialize_u8(7)
        }
    }
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _ = JsonValue::from_serializable_with_limits(
        &[Count(count.clone()), Count(count.clone())],
        JsonLimits {
            max_bytes: 2,
            ..Default::default()
        },
    );
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn ignored_serializer_and_formatter_errors_cannot_resume_admission() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Key<'a>(&'a AtomicUsize, usize);
    impl Serialize for Key<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            s.serialize_str(&self.1.to_string())
        }
    }
    struct KeepsGoing<'a>(&'a AtomicUsize);
    impl Serialize for KeepsGoing<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut map = s.serialize_map(None)?;
            for i in 0..10000 {
                let _ = map.serialize_entry(&Key(self.0, i), &true);
            }
            map.end()
        }
    }
    let count = AtomicUsize::new(0);
    assert_eq!(
        JsonValue::from_serializable_with_limits(
            &KeepsGoing(&count),
            JsonLimits {
                max_bytes: 2,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind(),
        ValueConversionErrorKind::Limit
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "keys must not be entered again after refusal"
    );
    struct Overflow;
    impl std::fmt::Display for Overflow {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let _ = f.write_str("0123456789");
            let _ = f.write_str("x");
            Ok(())
        }
    }
    struct Format<T>(T);
    impl<T: std::fmt::Display> Serialize for Format<T> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.collect_str(&self.0)
        }
    }
    let e = JsonValue::from_serializable_with_limits(
        &Format(Overflow),
        JsonLimits {
            max_bytes: 4,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.kind(), ValueConversionErrorKind::Limit);
    struct Fails;
    impl std::fmt::Display for Fails {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Err(std::fmt::Error)
        }
    }
    assert_eq!(
        JsonValue::from_serializable(&Format(Fails))
            .unwrap_err()
            .kind(),
        ValueConversionErrorKind::Serialization
    );
    struct FormattedKey;
    impl Serialize for FormattedKey {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut map = s.serialize_map(None)?;
            map.serialize_entry(&Format("string-key"), &7)?;
            map.end()
        }
    }
    assert_eq!(
        JsonValue::from_serializable(&FormattedKey).unwrap().text(),
        r#"{"string-key":7}"#
    );
}

#[test]
fn serde_profile_depth_is_explicit_and_smaller_limits_apply() {
    fn nested(depth: usize) -> serde_json::Value {
        let mut v = serde_json::Value::Bool(true);
        for _ in 0..depth {
            v = serde_json::Value::Array(vec![v]);
        }
        v
    }
    assert!(JsonValue::from_serializable(&nested(128)).is_ok());
    assert_eq!(
        JsonValue::from_serializable(&nested(129))
            .unwrap_err()
            .kind(),
        ValueConversionErrorKind::Limit
    );
    assert_eq!(
        JsonValue::from_serializable_with_limits(
            &nested(5),
            JsonLimits {
                max_depth: 4,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind(),
        ValueConversionErrorKind::Limit
    );
    let text = format!("{}true{}", "[".repeat(200), "]".repeat(200));
    assert!(
        JsonValue::parse(&text).is_ok(),
        "iterative exact parse retains its separate depth profile"
    );
}

#[test]
fn serde_json_numbers_preserve_tokens_and_protocol_cannot_inject_json() {
    use serde::ser::SerializeStruct;
    let text = r#"{"small":1,"wide":9007199254740993,"decimal":0.12345678901234567890123456789,"huge":1e99999999999999999999999999999999999}"#;
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let admitted = JsonValue::from_serializable(&value).unwrap();
    for (key, token) in [
        ("small", "1"),
        ("wide", "9007199254740993"),
        ("decimal", "0.12345678901234567890123456789"),
        ("huge", "1e+99999999999999999999999999999999999"),
    ] {
        assert_eq!(admitted.get(key).unwrap().number_text(), Some(token));
    }
    struct Protocol<'a> {
        token: &'a str,
        mode: u8,
    }
    impl Serialize for Protocol<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut st = s.serialize_struct("$serde_json::private::Number", 1)?;
            match self.mode {
                1 => {}
                2 => st.serialize_field("wrong", self.token)?,
                3 => {
                    st.serialize_field("$serde_json::private::Number", self.token)?;
                    st.serialize_field("$serde_json::private::Number", self.token)?;
                }
                4 => st.serialize_field("$serde_json::private::Number", &7)?,
                _ => st.serialize_field("$serde_json::private::Number", self.token)?,
            }
            st.end()
        }
    }
    for p in [
        Protocol {
            token: "1",
            mode: 1,
        },
        Protocol {
            token: "1",
            mode: 2,
        },
        Protocol {
            token: "1",
            mode: 3,
        },
        Protocol {
            token: "1",
            mode: 4,
        },
        Protocol {
            token: "NaN",
            mode: 0,
        },
        Protocol {
            token: "true",
            mode: 0,
        },
        Protocol {
            token: "1,\"injected\":false",
            mode: 0,
        },
        Protocol {
            token: " 1 ",
            mode: 0,
        },
    ] {
        let e = JsonValue::from_serializable(&BTreeMap::from([("n", p)])).unwrap_err();
        assert_eq!(
            e.kind(),
            ValueConversionErrorKind::UnsupportedRepresentation
        );
        assert_eq!(e.pointer(), Some("/n"));
    }
    assert!(
        JsonValue::from_serializable_with_limits(
            &serde_json::json!(7),
            JsonLimits {
                max_nodes: 1,
                max_bytes: 1,
                max_depth: 0
            }
        )
        .is_ok()
    );
    let e = JsonValue::from_serializable_with_limits(
        &Protocol {
            token: "123456",
            mode: 0,
        },
        JsonLimits {
            max_bytes: 5,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.kind(), ValueConversionErrorKind::Limit);
    let raw: Box<serde_json::value::RawValue> = serde_json::from_str("7").unwrap();
    assert_eq!(
        JsonValue::from_serializable(&raw).unwrap_err().kind(),
        ValueConversionErrorKind::UnsupportedRepresentation
    );
}

#[test]
fn batched_strings_preserve_utf8_escaping_and_every_byte_limit_boundary() {
    for value in [
        "plain",
        "é☃😀",
        "quote\"slash\\",
        "\u{0000}\n\r\t\u{001f}",
        "é\"☃\\😀\nend",
    ] {
        let admitted = JsonValue::from_serializable(value).unwrap();
        let expected = JsonValue::parse(serde_json::to_vec(value).unwrap()).unwrap();
        assert_eq!(admitted.semantic_eq(&expected), Some(true));
        for max_bytes in 0..=admitted.bytes().len() {
            let result = JsonValue::from_serializable_with_limits(
                value,
                JsonLimits {
                    max_bytes,
                    ..Default::default()
                },
            );
            if max_bytes == admitted.bytes().len() {
                assert_eq!(result.unwrap().bytes(), admitted.bytes());
            } else {
                assert_eq!(result.unwrap_err().kind(), ValueConversionErrorKind::Limit);
            }
        }
    }
    // A successfully encoded member can have an omitted path; that omission must
    // not leak into a later sibling's refusal when the shared path buffer resets.
    let values = BTreeMap::from([("a".repeat(5000), 1.0), ("z~/é".into(), f64::NAN)]);
    let error = JsonValue::from_serializable(&values).unwrap_err();
    assert_eq!(error.pointer(), Some("/z~0~1é"));
    assert!(!error.path_omitted_for_limit());
    #[derive(Serialize)]
    enum Branch {
        First(Vec<u8>),
        Second { bad: f64 },
    }
    let error = JsonValue::from_serializable(&[
        Branch::First(vec![1, 2]),
        Branch::Second { bad: f64::INFINITY },
    ])
    .unwrap_err();
    assert_eq!(error.pointer(), Some("/1/Second/bad"));
    // Exactly 4 KiB of an escaped UTF-8 pointer is truthful and retained; one
    // additional byte omits the whole pointer instead of returning a prefix.
    let key = format!("{}aaa", "é/~".repeat(682));
    let expected = format!("/{}aaa", "é~1~0".repeat(682));
    assert_eq!(expected.len(), 4096);
    let error =
        JsonValue::from_serializable(&BTreeMap::from([(key.clone(), f64::NAN)])).unwrap_err();
    assert_eq!(error.pointer(), Some(expected.as_str()));
    assert!(!error.path_omitted_for_limit());
    let error = JsonValue::from_serializable(&BTreeMap::from([(key + "b", f64::NAN)])).unwrap_err();
    assert_eq!(error.pointer(), None);
    assert!(error.path_omitted_for_limit());
}

#[test]
fn static_struct_fields_keep_duplicate_and_fused_admission_checks() {
    use serde::ser::{SerializeStruct, SerializeStructVariant};
    struct Repeated {
        variant: bool,
    }
    impl Serialize for Repeated {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if self.variant {
                let mut object = s.serialize_struct_variant("E", 0, "V", 2)?;
                object.serialize_field("a~/é", &true)?;
                object.serialize_field("a~/é", &false)?;
                object.end()
            } else {
                let mut object = s.serialize_struct("Repeated", 2)?;
                object.serialize_field("a~/é", &true)?;
                object.serialize_field("a~/é", &false)?;
                object.end()
            }
        }
    }
    for (variant, path) in [(false, "/a~0~1é"), (true, "/V/a~0~1é")] {
        let error = JsonValue::from_serializable(&Repeated { variant }).unwrap_err();
        assert_eq!(error.kind(), ValueConversionErrorKind::DuplicateKey);
        assert_eq!(error.pointer(), Some(path));
    }
    struct KeepGoing<'a>(&'a std::cell::Cell<usize>);
    struct Value<'a>(&'a std::cell::Cell<usize>);
    impl Serialize for Value<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0.set(self.0.get() + 1);
            s.serialize_bool(true)
        }
    }
    impl Serialize for KeepGoing<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut object = s.serialize_struct("KeepGoing", usize::MAX)?;
            for key in ["one", "two", "three"].into_iter().cycle().take(10_000) {
                let _ = object.serialize_field(key, &Value(self.0));
            }
            object.end()
        }
    }
    let calls = std::cell::Cell::new(0);
    let error = JsonValue::from_serializable_with_limits(
        &KeepGoing(&calls),
        JsonLimits {
            max_bytes: 2,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), ValueConversionErrorKind::Limit);
    assert_eq!(calls.get(), 0);
}

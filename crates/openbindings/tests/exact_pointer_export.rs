use openbindings::{JsonValue, Pointer};

fn read_pointer(text: &str) -> Pointer {
    let token = JsonValue::parse(text).unwrap();
    let units: Vec<_> = token.view().as_string().unwrap().utf16_units().collect();
    Pointer::from_utf16(&units).unwrap()
}

#[test]
fn exact_pointer_json_preserves_target_and_ordinary_serde_still_replaces() {
    let value = JsonValue::parse(r#"{"\udc00":1,"\ufffd":2}"#).unwrap();
    let pointer = value
        .view()
        .members()
        .unwrap()
        .next()
        .unwrap()
        .value
        .location()
        .pointer
        .unwrap();
    let text = pointer.to_exact_json();
    assert_eq!(text, r#""/\udc00""#);
    let restored = read_pointer(&text);
    assert_eq!(restored, pointer);
    assert_eq!(value.at_pointer(&restored).unwrap().text(), "1");

    let ordinary = serde_json::to_string(&pointer).unwrap();
    assert_eq!(ordinary, "\"/\u{fffd}\"");
    assert_eq!(
        value.at_pointer(&read_pointer(&ordinary)).unwrap().text(),
        "2"
    );
    assert_eq!(
        serde_json::to_value(&pointer).unwrap(),
        serde_json::json!("/\u{fffd}")
    );
}

#[test]
fn exact_pointer_json_round_trips_every_utf16_unit_and_json_escape() {
    // One token includes all code units, paired and unpaired. Escape RFC 6901
    // metacharacters before constructing the pointer, as a member name requires.
    let mut units = vec![47];
    for unit in 0..=u16::MAX {
        match unit {
            47 => units.extend([126, 49]),
            126 => units.extend([126, 48]),
            _ => units.push(unit),
        }
    }
    let pointer = Pointer::from_utf16(&units).unwrap();
    assert_eq!(read_pointer(&pointer.to_exact_json()), pointer);
    for text in ["", "/", "/~0~1/é/😀", "/\"\\\n"] {
        let pointer: Pointer = text.parse().unwrap();
        assert_eq!(read_pointer(&pointer.to_exact_json()), pointer);
    }
}

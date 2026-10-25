//! Refusing oversized string encodings must not allocate the encoded payload.
use super::{JsonValue, measured};
use openbindings::{InputErrorKind, JsonLimits};
use serde_json::json;

pub fn run() {
    let limit = JsonLimits::default().max_bytes;
    for (name, unit, count) in [
        ("unescaped", "a", limit),
        ("control", "\0", limit),
        ("escape-boundary", "\0", (limit - 2) / 6 + 1),
        ("quote-boundary", "\"", (limit - 2) / 2 + 1),
        ("utf8-boundary", "é", limit / 2),
    ] {
        let input = unit.repeat(count);
        let (result, bytes, nanos) = measured(|| JsonValue::string(&input));
        let error = result.unwrap_err();
        assert_eq!(
            (error.kind(), error.byte_offset(), error.code()),
            (InputErrorKind::Limit, limit, "byte-limit")
        );
        assert!(
            bytes <= 4096,
            "string rejection: {name}: {bytes} exceeds 4096 bytes"
        );
        println!(
            "{}",
            json!({"phase":"string-constructor", "case":name,
            "source_bytes":input.len(), "peak_bytes":bytes, "nanos":nanos,
            "ceiling_bytes":4096})
        );
    }
}

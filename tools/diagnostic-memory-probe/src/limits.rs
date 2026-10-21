//! Document validation at the default admission limits stays within one
//! allowance above the caller's input, whatever the document's shape: its own
//! copy of the text, the decoded text of escaped strings, and its tree and
//! validation state, three times the default byte limit in all.
use super::{ALLOCATIONS, measured};
use serde_json::json;
use std::fmt::Write;
use std::sync::atomic::Ordering::Relaxed;

const MIB: usize = 1024 * 1024;
const MAX_BYTES: usize = 64 * MIB;
/// Peak live allocation above the caller's input, at the default limits.
const ALLOWANCE: usize = 3 * MAX_BYTES;
const HEAD: &str = r#"{"openbindings":"0.2.0""#;

/// `prefix`, a member holding one string that fills the document to the byte
/// limit, then `suffix`; the string begins with an escape when `escaped`.
fn filled(prefix: &str, suffix: &str, escaped: bool) -> String {
    let member = if escaped { r#","x-fill":"\n"# } else { r#","x-fill":""# };
    let fill = MAX_BYTES - prefix.len() - suffix.len() - member.len() - 1;
    let mut text = String::with_capacity(MAX_BYTES);
    text.push_str(prefix);
    text.push_str(member);
    text.extend(std::iter::repeat_n('a', fill));
    text.push('"');
    text.push_str(suffix);
    assert_eq!(text.len(), MAX_BYTES);
    text
}
fn joined(count: usize, item: impl Fn(usize) -> String) -> String {
    let mut text = String::new();
    for i in 0..count {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&item(i));
    }
    text
}
// Nested relative `$id`s, one level each, optionally declaring one identifier
// twice at every level.
fn identifier_chain(levels: usize, repeated: bool) -> String {
    let mut text = format!(r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"$id":"https://e.test/","#);
    for _ in 0..levels {
        if repeated {
            text.push_str(r#""$defs":{"x":{"$id":"d"},"y":{"$id":"d"},"a":{"$id":"a/","#);
        } else {
            text.push_str(r#""$defs":{"a":{"$id":"a/","#);
        }
    }
    text.push_str(r#""type":"null""#);
    for _ in 0..levels {
        text.push_str("}}");
    }
    text.push_str("}}}");
    text
}

/// Each document maximizes one structure validation builds, or pairs one with
/// a string that holds as much decoded text as the remaining bytes allow.
fn shapes() -> Vec<(&'static str, String, &'static str)> {
    let mut shapes = Vec::new();
    let levels = 999_980;
    let mut nested = format!(r#"{HEAD},"operations":{{}},"x":"#);
    nested.extend(std::iter::repeat_n('[', levels));
    nested.push('"');
    let close = format!(r#""{}}}"#, "]".repeat(levels));
    let fill = MAX_BYTES - nested.len() - close.len();
    nested.extend(std::iter::repeat_n('a', fill));
    nested.push_str(&close);
    shapes.push(("nested-arrays-around-a-string", nested, "non-conformant"));
    let invalid = joined(499_990, |i| format!(r#""!{i}":{{}}"#));
    shapes.push((
        "invalid-operation-names",
        format!(r#"{HEAD},"operations":{{{invalid}}}}}"#),
        "non-conformant",
    ));
    shapes.push((
        "invalid-operation-names-filled",
        filled(&format!(r#"{HEAD},"operations":{{{invalid}}}"#), "}", false),
        "non-conformant",
    ));
    drop(invalid);
    let per = (MAX_BYTES - 64) / 999_990 - 1;
    let escaped = format!(r#""\n{}""#, "a".repeat(per - 4));
    shapes.push((
        "escaped-strings",
        format!(
            r#"{HEAD},"operations":{{}},"x-s":[{}]}}"#,
            joined(999_990, |_| escaped.clone())
        ),
        "conformant",
    ));
    shapes.push((
        "escaped-string",
        filled(&format!(r#"{HEAD},"operations":{{}}"#), "}", true),
        "conformant",
    ));
    let unpaired = format!(r#"{HEAD},"operations":{{}},"x-u":"\ud800"#);
    let fill = MAX_BYTES - unpaired.len() - 2;
    shapes.push((
        "unpaired-string",
        format!("{unpaired}{}\"}}", "a".repeat(fill)),
        "non-conformant",
    ));
    shapes.push((
        "wide-allof-and-escaped-string",
        filled(
            &format!(
                r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"allOf":[{}]}}}}"#,
                joined(999_980, |_| "{}".into())
            ),
            "}",
            true,
        ),
        "conformant",
    ));
    shapes.push((
        "aliases-and-escaped-string",
        filled(
            &format!(
                r#"{HEAD},"operations":{{"o":{{"aliases":[{}]}}}}"#,
                joined(999_980, |i| format!(r#""a{i}""#))
            ),
            "}",
            true,
        ),
        "conformant",
    ));
    shapes.push((
        "wide-object-and-escaped-string",
        filled(
            &format!(
                r#"{HEAD},"operations":{{}},"x-w":{{{}}}"#,
                joined(499_985, |i| format!(r#""k{i}":0"#))
            ),
            "}",
            true,
        ),
        "conformant",
    ));
    shapes.push((
        "schemas-and-escaped-string",
        filled(
            &format!(
                r#"{HEAD},"operations":{{}},"schemas":{{{}}}"#,
                joined(499_985, |i| format!(r#""s{i}":{{}}"#))
            ),
            "}",
            true,
        ),
        "conformant",
    ));
    shapes.push((
        "identifiers-and-escaped-string",
        filled(
            &format!(
                r#"{HEAD},"operations":{{}},"schemas":{{{}}}"#,
                joined(249_990, |i| format!(r#""{i}":{{"$id":"https://e.test/{i}"}}"#))
            ),
            "}",
            true,
        ),
        "conformant",
    ));
    shapes.push((
        "nested-relative-identifiers",
        identifier_chain(166_000, false),
        "conformant",
    ));
    shapes.push((
        "nested-repeated-identifiers",
        identifier_chain(70_000, true),
        "non-conformant",
    ));
    // A malformed reference at every level of a deep schema chain: each
    // finding's pointer is longer than the last.
    let mut deep = format!(r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"#);
    for _ in 0..120_000 {
        deep.push_str(r#""$defs":{"x":{"$ref":"a b"},"a":{"#);
    }
    deep.push_str(r#""type":"null""#);
    for _ in 0..120_000 {
        let _ = write!(deep, "}}}}");
    }
    deep.push_str("}}}");
    shapes.push(("deep-findings", deep, "non-conformant"));
    shapes
}

pub fn run() {
    for (shape, text, expected) in shapes() {
        let bytes = text.into_bytes();
        assert!(bytes.len() <= MAX_BYTES, "{shape} exceeds the byte limit");
        let before = ALLOCATIONS.load(Relaxed);
        let (outcome, peak, nanos) = measured(|| {
            let validation = openbindings::validate_document(&bytes).unwrap();
            let report = validation.report();
            (
                serde_json::to_value(report.conclusion).unwrap(),
                report.findings.len(),
                report.findings_truncated,
            )
        });
        let allocations = ALLOCATIONS.load(Relaxed) - before;
        assert_eq!(outcome.0, expected, "{shape}");
        println!(
            "{}",
            json!({"phase":"default-limits","shape":shape,"source_bytes":bytes.len(),
            "peak_bytes":peak,"allocations":allocations,"nanos":nanos,
            "conclusion":outcome.0,"findings":outcome.1,"findings_truncated":outcome.2,
            "ceiling_bytes":ALLOWANCE})
        );
        assert!(
            peak <= ALLOWANCE,
            "document validation at the default limits: {shape}: {peak} exceeds {ALLOWANCE} bytes"
        );
    }
}

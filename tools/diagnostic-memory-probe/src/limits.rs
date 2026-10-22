//! Document validation at the default admission limits, on documents that
//! mostly fill the byte or node limit with structures it builds: its own copy
//! of the text, the decoded text of escaped strings, and its tree and
//! validation state, with the buffers they grow in. Each must stay within one
//! allowance above the caller's input, seven times the default byte limit,
//! which the largest, copied identifier paths with pieces, approaches within
//! 36 MiB.
use super::{ALLOCATIONS, measured};
use serde_json::json;
use std::fmt::Write;
use std::sync::atomic::Ordering::Relaxed;

const MIB: usize = 1024 * 1024;
const MAX_BYTES: usize = 64 * MIB;
/// Peak live allocation above the caller's input, at the default limits.
const ALLOWANCE: usize = 7 * MAX_BYTES;
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

// One `$id` of `segment` repeated to fill `total` bytes, after `before`.
fn long_identifier(total: usize, before: &str, start: &str, segment: &str) -> String {
    let prefix = format!(r#"{HEAD},"operations":{{}},"schemas":{{{before}"s":{{"$id":"{start}"#);
    let suffix = r#""}}}"#;
    let count = (total - prefix.len() - suffix.len()) / segment.len();
    let mut text = String::with_capacity(total);
    text.push_str(&prefix);
    text.extend(std::iter::repeat_n(segment, count));
    text.push_str(suffix);
    text
}
const REPEATED_PAIRS: &str = r#""t1":{"$id":"https://x.test/1"},"t2":{"$id":"https://x.test/1"},"t3":{"$id":"https://x.test/2"},"t4":{"$id":"https://x.test/2"},"#;

/// Each document fills the limits with one structure validation builds, or
/// pairs one with a string that holds as much decoded text as the remaining
/// bytes allow.
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
    // Invalid operation names that hold escapes, filling the byte limit: the
    // decoded names beside the findings and their pointers.
    let width = MAX_BYTES / 499_990 - 8;
    let names = joined(499_990, |i| format!(r#""\n{i:07}{}":{{}}"#, "a".repeat(width - 10)));
    let mut escaped_names = format!(r#"{HEAD},"operations":{{{names}}}}}"#);
    drop(names);
    assert!(escaped_names.len() <= MAX_BYTES, "{}", escaped_names.len());
    escaped_names.shrink_to_fit();
    shapes.push(("escaped-invalid-operation-names", escaped_names, "non-conformant"));
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
    // A third of a million `$id`s that each add directories, under a base or
    // with authorities of their own, alone, in pairs and with a query and
    // fragment, beside an escaped string filling the remaining bytes.
    let directories = |name: &'static str,
                       base: &str,
                       each: &dyn Fn(usize) -> String,
                       expected: &'static str| {
        let identifiers = joined(333_320, |i| format!(r#"{{"$id":"{}"}}"#, each(i)));
        let schema = format!(r#""s":{{{base}"allOf":[{identifiers}]}}"#);
        let prefix = format!(r#"{HEAD},"operations":{{}},"schemas":{{{schema}}}"#);
        (name, filled(&prefix, "}", true), expected)
    };
    let base = r#""$id":"https://e.test/","#;
    let pair = |i: usize, a: &str, b: &str| if i % 2 == 0 { a } else { b }.to_owned();
    for shape in [
        directories("relative-directories", base, &|i| format!("{i}/"), "conformant"),
        directories(
            "relative-directory-pairs",
            base,
            &|i| format!("a{}/{}", i / 2, pair(i, "b/", "c/")),
            "conformant",
        ),
        directories(
            "relative-directory-pairs-with-pieces",
            base,
            &|i| format!("a{}/{}", i / 2, pair(i, "b/?#f", "c/?#g")),
            "non-conformant",
        ),
        directories("authority-directories", "", &|i| format!("s://{i}/x/"), "conformant"),
        directories(
            "authority-directories-with-pieces",
            "",
            &|i| format!("s://{i}/x/?#f"),
            "non-conformant",
        ),
        directories(
            "authority-directory-pairs-with-pieces",
            "",
            &|i| format!("s://{}/x/{}", i / 2, pair(i, "y/?#f", "z/?#g")),
            "non-conformant",
        ),
    ] {
        shapes.push(shape);
    }
    // A third of a million escaped `$id`s, each with its own authority and a
    // dot segment that makes its path a copy, filling the byte limit: the
    // decoded identifiers, their copied paths and their directories at once.
    let copied = |name: &'static str, pieces: &str, expected: &'static str| {
        let element = |i: usize, length: usize| {
            format!(
                r#"{{"$id":"s:\/\/{i:x}/q/a/../b/{}/{pieces}"}}"#,
                "c".repeat(length)
            )
        };
        let prefix = format!(r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"allOf":["#);
        let suffix = "]}}}";
        let base = prefix.len() + suffix.len() + (0..333_320).map(|i| element(i, 0).len() + 1).sum::<usize>();
        let length = (MAX_BYTES - base) / 333_320;
        let text = format!("{prefix}{}{suffix}", joined(333_320, |i| element(i, length)));
        (name, text, expected)
    };
    shapes.push(copied("copied-identifier-paths", "", "conformant"));
    shapes.push(copied("copied-identifier-paths-with-pieces", "?#f", "non-conformant"));
    // One name or alias of U+E000, the character fixed checks escape beside
    // encoded unpaired units, filling the byte limit; and the operation name
    // beside one unpaired unit, where each is presented doubled.
    let marker = |name: &'static str, prefix: &str, suffix: &str, expected: &'static str| {
        let count = (MAX_BYTES - prefix.len() - suffix.len()) / 3;
        let mut text = String::with_capacity(MAX_BYTES);
        text.push_str(prefix);
        text.extend(std::iter::repeat_n('\u{e000}', count));
        text.push_str(suffix);
        (name, text, expected)
    };
    for shape in [
        marker(
            "marker-operation-name",
            r#"{"openbindings":"0.2.0","operations":{""#,
            r#"":{}}}"#,
            "non-conformant",
        ),
        marker("marker-extension-name", &format!(r#"{HEAD},"operations":{{}},"x-"#), r#"":0}"#, "conformant"),
        marker(
            "marker-alias",
            r#"{"openbindings":"0.2.0","operations":{"o":{"aliases":[""#,
            r#""]}}}"#,
            "non-conformant",
        ),
        marker(
            "marker-operation-name-beside-an-unpaired-unit",
            r#"{"openbindings":"0.2.0","x-u":"\ud800","operations":{""#,
            r#"":{}}}"#,
            "non-conformant",
        ),
    ] {
        shapes.push(shape);
    }
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
    // One `$id` of many segments, alone and beside identifiers declared twice.
    for (shape, total, before, expected) in [
        ("long-identifier-8mib", 8 * MIB, "", "conformant"),
        ("long-identifier", MAX_BYTES, "", "conformant"),
        ("long-identifier-and-repeated-pairs-8mib", 8 * MIB, REPEATED_PAIRS, "non-conformant"),
        ("long-identifier-and-repeated-pairs", MAX_BYTES, REPEATED_PAIRS, "non-conformant"),
    ] {
        shapes.push((shape, long_identifier(total, before, "https://e.test/", "a/"), expected));
    }
    // Dot segments throughout one long `$id`: its path is copied once.
    shapes.push((
        "long-identifier-with-dot-segments",
        long_identifier(MAX_BYTES, "", "https://e.test/", "a/./"),
        "conformant",
    ));
    // One long authority, read back by the relative identifiers below it.
    let children = joined(150_000, |i| format!(r#""{i}":{{"$id":"x{i}"}}"#));
    let mut authority = format!(
        r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"$defs":{{{children}}},"$id":"https://"#
    );
    let fill = MAX_BYTES - authority.len() - r#""}}}"#.len();
    authority.extend(std::iter::repeat_n('a', fill));
    authority.push_str(r#""}}}"#);
    shapes.push((
        "long-authority-and-relative-identifiers",
        authority,
        "conformant",
    ));
    // Relative identifiers that each give up a different number of a long
    // identifier's segments: each splits the run it lands in.
    let mut nested = format!(
        r#"{HEAD},"operations":{{}},"schemas":{{"s":{{"$id":"https://e.test/{}","$defs":{{"#,
        "ab/".repeat(10_000_000)
    );
    for up in 1..2_400 {
        if up > 1 {
            nested.push(',');
        }
        let _ = write!(nested, r#""{up}":{{"$id":"{}x/"}}"#, "../".repeat(up * 4));
    }
    nested.push_str("}}}}");
    shapes.push(("relative-identifiers-inside-a-long-one", nested, "conformant"));
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

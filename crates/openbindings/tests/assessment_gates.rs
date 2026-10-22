use openbindings::*;

fn checked(text: &str) -> DocumentValidation {
    validate_document(text).unwrap()
}

#[test]
fn later_rules_require_scalar_document_strings_but_allow_deep_opaque_data() {
    // Each row pairs a document with the rules it independently violates.
    for (text, rules) in [
        (r#"{"openbindings":"0.2.0","description":"\ud800"}"#.to_owned(), vec!["OBI-02"]),
        (r#"{"openbindings":"0.2.0","operations":{"-bad":{}},"description":"\ud800"}"#.into(), vec!["OBI-02", "OBI-04"]),
        (r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"missing","source":"s","content":"\ud800"}}}"#.into(), vec!["OBI-06"]),
        (r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/nope"}}},"x-data":"\ud800"}"##.into(), vec!["OBI-12"]),
        (r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$schema":"draft-07"}}},"x-data":"\ud800"}"#.into(), vec!["OBI-02", "OBI-09"]),
        (format!(r#"{{"openbindings":"0.2.0","x-d":{}0{}}}"#, "[".repeat(10001), "]".repeat(10001)), vec!["OBI-02"]),
        (format!(r#"{{"openbindings":"0.2.0","operations":{{"-bad":{{}}}},"x-d":{}0{}}}"#, "[".repeat(10001), "]".repeat(10001)), vec!["OBI-04"]),
    ] {
        let validation = checked(&text);
        assert_eq!(validation.report().conclusion, Conformance::NonConformant, "{rules:?}");
        if text.contains(r"\ud") {
            assert_eq!(validation.report().evidence["OBI-01"], Evidence::Violated);
            for rule in &DOCUMENT_RULES[1..] {
                assert_eq!(validation.report().evidence[rule], Evidence::NotApplicable);
            }
        }
        // A permitted pair restores the prerequisite, exposing every original
        // independent violation rather than dropping those regression checks.
        let repaired = text.replace(r"\ud800", r"\ud83d\ude00");
        let validation = checked(&repaired);
        for rule in rules { assert_eq!(validation.report().evidence[rule], Evidence::Violated, "{rule}"); }
    }
}

#[test]
fn unpaired_document_strings_are_lossless_drafts_without_conformance() {
    // OBI-01 covers opaque content and schemas as well as defined fields.
    for fields in [
        r#""description":"cut \ud83d""#,
        r#""name":"\ud800","version":"\ud800""#,
        r#""sources":{"s":{"kind":"\ud800","content":{"\ud800":"\udc00"}}}"#,
        r#""x-\ud800":{"\ud800":"\udc00"}"#,
        r#""schemas":{"s":{"description":"\ud800","pattern":"\ud800","const":"\ud800","required":["\ud800","\udc00","\ue000d800;"]}}"#,
        r#""dependencies":{"d":{"operation":"op","kinds":["\ud800","\udc00","\ue000d800;"]}}"#,
    ] {
        let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},{fields}}}"#);
        let validation = checked(&text);
        assert_eq!(
            validation.report().conclusion,
            Conformance::NonConformant,
            "{fields}: {:?}",
            validation.report().findings
        );
        assert_eq!(validation.document().unwrap().as_bytes(), text.as_bytes());
    }
    for (field, rule) in [
        (
            r#""dependencies":{"d":{"operation":"op","kinds":["\ud800","\uD800"]}}"#,
            "OBI-02",
        ),
        (
            r#""schemas":{"s":{"required":["\ud800","\uD800"]}}"#,
            "OBI-10",
        ),
        (r#""schemas":{"s":{"$anchor":"\ud800"}}"#, "OBI-10"),
        (r#""schemas":{"s":{"$id":"x\ud800#bad"}}"#, "OBI-10"),
    ] {
        let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},{field}}}"#);
        let validation = checked(&text);
        assert_eq!(validation.report().evidence["OBI-01"], Evidence::Violated);
        assert_eq!(validation.report().evidence[rule], Evidence::NotApplicable);
        let repaired = text
            .replace(r"\ud800", r"\ud83d\ude00")
            .replace(r"\uD800", r"\uD83D\uDE00");
        let validation = checked(&repaired);
        assert_eq!(
            validation.report().evidence[rule],
            Evidence::Violated,
            "{field}"
        );
    }
}

#[test]
fn native_names_compare_decoded_pairs_even_when_grammar_is_violated() {
    // invalid names still participate in exact namespace/reference rules.
    let validation = checked(
        r#"{"openbindings":"0.2.0","operations":{"\ud83d\ude00":{"aliases":["\uD83D\uDE00"]}},"sources":{"\uffff":{"kind":"k"}},"bindings":{"b":{"operation":"\uD83D\uDE00","source":"\uFFFF"}},"dependencies":{"d":{"operation":"\ud83d\ude00"}}}"#,
    );
    let evidence = &validation.report().evidence;
    assert_eq!(evidence["OBI-04"], Evidence::Violated);
    assert_eq!(evidence["OBI-05"], Evidence::Violated);
    for rule in ["OBI-06", "OBI-07", "OBI-08"] {
        assert_eq!(evidence[rule], Evidence::Met);
    }
}

#[test]
fn assessment_admission_is_caller_selected_and_precedes_version() {
    // all three limits precede a complete version declaration.
    let text = r#"{"openbindings":"0.3.0","operations":{},"x":[0]}"#;
    for (limits, setting) in [
        (
            JsonLimits {
                max_bytes: 10,
                ..JsonLimits::default()
            },
            "JsonLimits::max_bytes",
        ),
        (
            JsonLimits {
                max_nodes: 2,
                ..JsonLimits::default()
            },
            "JsonLimits::max_nodes",
        ),
        (
            JsonLimits {
                max_depth: 1,
                ..JsonLimits::default()
            },
            "JsonLimits::max_depth",
        ),
    ] {
        let validation = validate_document_with_limits(text, limits).unwrap();
        // Each refusal names the setting that raises its limit.
        assert!(
            validation
                .report()
                .findings
                .iter()
                .all(|f| f.message.contains(setting))
        );
        assert_eq!(validation.report().conclusion, Conformance::Undetermined);
        assert!(
            validation
                .report()
                .evidence
                .values()
                .all(|v| *v == Evidence::Inconclusive)
        );
        assert!(
            validation
                .report()
                .findings
                .iter()
                .all(|f| f.code == "input-limit")
        );
    }
    assert!(validate_document_with_limits(text, JsonLimits::default()).is_err());
    let text = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-data":[{}0]}}"#,
        "0,".repeat(1_000_000)
    );
    assert_eq!(
        checked(&text).report().conclusion,
        Conformance::Undetermined
    );
    let raised = validate_document_with_limits(
        &text,
        JsonLimits {
            max_nodes: 1_000_100,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    assert_eq!(raised.report().conclusion, Conformance::Conformant);
}

#[test]
fn the_default_byte_limit_admits_32_mib_and_names_its_setting_past_it() {
    let limit = JsonLimits::default().max_bytes;
    assert_eq!(limit, 32 * 1024 * 1024);
    let document = padded;
    assert_eq!(
        checked(&document(limit)).report().conclusion,
        Conformance::Conformant
    );
    let over = document(limit + 1);
    let validation = checked(&over);
    let report = validation.report();
    assert_eq!(report.conclusion, Conformance::Undetermined);
    assert_eq!(report.findings.len(), DOCUMENT_RULES.len());
    for finding in &report.findings {
        assert_eq!(finding.code, "input-limit");
        assert_eq!(finding.location, None);
        assert_eq!(
            finding.message,
            "the document's 33554433 bytes exceed the 33554432-byte admission limit; \
             validate it with validate_document_with_limits and a larger \
             JsonLimits::max_bytes (validateDocument with a larger limits.maxBytes in \
             TypeScript)"
        );
    }
    // Raising the named setting admits the same document.
    let raised = validate_document_with_limits(
        &over,
        JsonLimits {
            max_bytes: limit + 1,
            max_depth: usize::MAX,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    assert_eq!(raised.report().conclusion, Conformance::Conformant);
}

// A conformant document of exactly `bytes`, padded by one string.
fn padded(bytes: usize) -> String {
    let head = r#"{"openbindings":"0.2.0","operations":{},"x-fill":""#;
    let mut text = String::with_capacity(bytes);
    text.push_str(head);
    text.extend(std::iter::repeat_n('a', bytes - head.len() - 2));
    text.push_str(r#""}"#);
    assert_eq!(text.len(), bytes);
    text
}

#[test]
fn a_raised_byte_limit_admits_documents_to_it_and_they_keep_it_for_edits() {
    let default = JsonLimits::default().max_bytes;
    let raised = JsonLimits {
        max_bytes: default + 1024,
        ..JsonLimits::default()
    };
    let (at, over) = (padded(raised.max_bytes), padded(raised.max_bytes + 1));
    // Validation admits a document at the raised limit and refuses one past it,
    // naming the setting.
    let validation = validate_document_with_limits(&at, raised).unwrap();
    assert_eq!(validation.report().conclusion, Conformance::Conformant);
    let proof = validation.conformant().unwrap();
    assert_eq!(proof.as_document().limits(), raised);
    let refused = validate_document_with_limits(&over, raised).unwrap();
    assert_eq!(refused.report().conclusion, Conformance::Undetermined);
    assert!(refused.report().findings.iter().all(|f| f.message
        == format!(
            "the document's {} bytes exceed the {}-byte admission limit; validate it \
             with validate_document_with_limits and a larger JsonLimits::max_bytes \
             (validateDocument with a larger limits.maxBytes in TypeScript)",
            raised.max_bytes + 1,
            raised.max_bytes
        )));
    // Parsing does the same, and the parsed document keeps its limits.
    let parsed = Document::parse_with_limits(&at, raised).unwrap();
    assert_eq!(parsed.limits(), raised);
    let error = Document::parse_with_limits(&over, raised).unwrap_err();
    assert_eq!(
        (error.code, error.byte_offset),
        ("byte-limit", raised.max_bytes)
    );
    assert!(
        error
            .to_string()
            .ends_with("; raise JsonLimits::max_bytes (limits.maxBytes in TypeScript)")
    );
    // Edits and typed setters parse under the document's limits: a small
    // document admitted with them grows past the default, one parsed with the
    // defaults does not.
    let empty = r#"{"openbindings":"0.2.0","operations":{}}"#;
    let large =
        JsonValue::parse_with_limits(format!(r#""{}""#, "a".repeat(default)), raised).unwrap();
    let mut grown = Document::parse_with_limits(empty, raised).unwrap();
    grown.set("/x-fill", &large).unwrap();
    assert!(grown.as_bytes().len() > default);
    assert_eq!(grown.limits(), raised);
    grown.set_description(&"d".repeat(512)).unwrap();
    assert_eq!(
        grown.validate().unwrap().report().conclusion,
        Conformance::Conformant
    );
    let mut plain = Document::parse(empty).unwrap();
    let error = plain.set("/x-fill", &large).unwrap_err();
    assert_eq!(error.code(), "byte-limit");
    // The refusal names the setting the document was admitted with.
    assert_eq!(
        error.to_string(),
        "document edit refused: the edited document exceeds its byte limit; admit the \
         document with a larger JsonLimits::max_bytes (limits.maxBytes in TypeScript)"
    );
    // A proof's document keeps its limits for editing again: at the raised
    // limit it cannot grow, and once smaller it can.
    let mut again = proof.into_inner();
    assert_eq!(again.set_name("raised").unwrap_err().code(), "byte-limit");
    again.remove("/x-fill").unwrap();
    again.set_name("raised").unwrap();
    assert_eq!(again.limits(), raised);
    // Each entry's default: the JsonLimits defaults for parsing, construction
    // and from_json, and no nesting cap for validate_document.
    assert_eq!(
        Document::parse(empty).unwrap().limits(),
        JsonLimits::default()
    );
    assert_eq!(Document::new().limits(), JsonLimits::default());
    let validated = validate_document(empty).unwrap();
    assert_eq!(
        validated.document().unwrap().limits(),
        JsonLimits {
            max_depth: usize::MAX,
            ..JsonLimits::default()
        }
    );
}

#[test]
fn edit_limits_apply_to_the_edited_document() {
    // These are the cases the TypeScript edit routes match.
    let empty = r#"{"openbindings":"0.2.0","operations":{}}"#;
    let refusal = |limit: &str, rust: &str, typescript: &str| {
        format!(
            "document edit refused: the edited document exceeds its {limit} limit; admit the \
             document with a larger JsonLimits::{rust} (limits.{typescript} in TypeScript)"
        )
    };
    // At 4 levels, a 3-level value fits a top-level member; 4 levels do not.
    let mut shallow = Document::parse_with_limits(
        empty,
        JsonLimits {
            max_depth: 4,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    let three = JsonValue::parse("[[[1]]]").unwrap();
    shallow.set("/x-a", &three).unwrap();
    shallow.remove("/x-a").unwrap();
    shallow
        .edit(&[Edit::Set {
            pointer: "/x-a".into(),
            value: &three,
        }])
        .unwrap();
    let error = shallow
        .set("/x-b", &JsonValue::parse("[[[[1]]]]").unwrap())
        .unwrap_err();
    assert_eq!(error.code(), "depth-limit");
    assert_eq!(
        error.to_string(),
        refusal("nesting", "max_depth", "maxDepth")
    );
    // At 7 nodes, one more member and value fit.
    let mut few = Document::parse_with_limits(
        empty,
        JsonLimits {
            max_nodes: 7,
            ..JsonLimits::default()
        },
    )
    .unwrap();
    few.set("/x-a", &JsonValue::integer(1)).unwrap();
    let error = few.set("/x-b", &JsonValue::integer(1)).unwrap_err();
    assert_eq!(error.code(), "node-limit");
    assert_eq!(error.to_string(), refusal("node", "max_nodes", "maxNodes"));
    // At its own size, a same-size replacement fits and a longer one does not.
    let exact = r#"{"openbindings":"0.2.0","operations":{},"x-a":"abc"}"#;
    let mut sized = Document::parse_with_limits(
        exact,
        JsonLimits {
            max_bytes: exact.len(),
            ..JsonLimits::default()
        },
    )
    .unwrap();
    sized
        .set("/x-a", &JsonValue::string("xyz").unwrap())
        .unwrap();
    let error = sized
        .set("/x-a", &JsonValue::string("wxyz").unwrap())
        .unwrap_err();
    assert_eq!(error.to_string(), refusal("byte", "max_bytes", "maxBytes"));
}

#[test]
fn parse_findings_explain_the_syntax_failure() {
    // stable machine codes and concise human text are separate.
    for (text, code, message) in [
        ("{x}", "object-key", "expected a quoted object member name"),
        (
            "[1 2]",
            "comma-or-close",
            "expected a comma or the end of the container",
        ),
        ("1e", "exponent", "expected a digit in the number exponent"),
    ] {
        let validation = checked(text);
        let finding = &validation.report().findings[0];
        assert_eq!(finding.rule, "OBI-01");
        assert_eq!(finding.code, code);
        assert_eq!(finding.message, message);
        assert!(finding.location.is_some());
    }
}

#[test]
fn the_whole_document_check_reaches_a_verdict_at_every_admitted_size() {
    // The OBI-02 check has no work budget; the admission limits bound it.
    // These documents sit well inside the default limits and beyond any
    // allowance fixed at two million steps, which left them undetermined.
    let entries = |head: &str, count: usize, entry: fn(usize) -> String, tail: &str| {
        let mut text = String::from(head);
        text.push_str(&(0..count).map(entry).collect::<Vec<_>>().join(","));
        text + tail
    };
    let schemas = entries(
        r#"{"openbindings":"0.2.0","operations":{},"schemas":{"#,
        300_000,
        |i| format!(r#""s{i}":true"#),
        "}}",
    );
    let aliases = entries(
        r#"{"openbindings":"0.2.0","operations":{"o":{"aliases":["#,
        350_000,
        |i| format!(r#""a{i}""#),
        "]}}}",
    );
    for text in [schemas, aliases] {
        let validation = checked(&text);
        assert_eq!(validation.report().conclusion, Conformance::Conformant);
    }
    // A long near miss of the costliest pattern reaches its verdict: the
    // pattern runs on a finite automaton, with no step limit to exhaust.
    let version = format!(
        r#"{{"openbindings":"0.0.0-{}!","operations":{{}}}}"#,
        "1".repeat(60_000)
    );
    let validation = checked(&version);
    assert_eq!(validation.report().evidence["OBI-02"], Evidence::Violated);
    assert!(
        validation
            .report()
            .findings
            .iter()
            .all(|finding| finding.code != "fixed-schema-limit-or-failure")
    );
}

#[test]
fn every_embedded_schema_reaches_a_meta_schema_verdict_at_every_admitted_size() {
    // OBI-10 has no work budget either. A near miss of a meta-schema pattern a
    // million bytes long, in a schema of a few nodes, and a schema of nearly a
    // million nodes both fell outside an earlier fixed allowance.
    let long = "a".repeat(1_000_000);
    let schema = |body: &str| {
        format!(r#"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"S":{body}}}}}"#)
    };
    for body in [
        format!(r#"{{"$anchor":"{long}!"}}"#),
        format!(r#"{{"$dynamicAnchor":"{long}!"}}"#),
        format!(r#"{{"$id":"https://e.example/{long}##"}}"#),
    ] {
        let validation = checked(&schema(&body));
        assert_eq!(validation.report().evidence["OBI-10"], Evidence::Violated);
    }
    let vocabulary = (0..450_000)
        .map(|i| format!(r#""https://e.example/{i}":true"#))
        .collect::<Vec<_>>()
        .join(",");
    for body in [
        format!(r#"{{"$anchor":"{long}"}}"#),
        format!(r#"{{"$vocabulary":{{{vocabulary}}}}}"#),
    ] {
        let validation = checked(&schema(&body));
        assert_eq!(validation.report().conclusion, Conformance::Conformant);
    }
}

/// What the 0.2 line's rules make of a text, as specification §8.1 tabulates it.
enum Under02 {
    Conforms,
    Violates(&'static [&'static str]),
    /// The text declares this version of another line or a prerelease.
    GovernedBy(&'static str),
}

#[test]
fn version_declarations_follow_the_specification_examples() {
    // Specification §8.1, "Version declaration examples": each text, what it
    // declares and what follows under the 0.2 line's rules. "Repeated names
    // elsewhere do not stop a declaration. The declared line's rules govern
    // them": a 0.3.0 text with a repeated member is 0.3's to judge.
    use Under02::*;
    let bytes = |parts: &[&[u8]]| parts.concat();
    let rows: Vec<(Vec<u8>, Under02)> = vec![
        (
            br#"{"openbindings":"0.2.0","operations":{}}"#.to_vec(),
            Conforms,
        ),
        (
            br#"{"openbindings":"0.2.7","operations":{}}"#.to_vec(),
            Conforms,
        ),
        (
            br#"{"openbindings":"0.2.0+build.5","operations":{}}"#.to_vec(),
            Conforms,
        ),
        (
            br#"{"openbindings":"0.3.0","operations":{}}"#.to_vec(),
            GovernedBy("0.3.0"),
        ),
        (
            br#"{"openbindings":"0.2.0-rc.1","operations":{}}"#.to_vec(),
            GovernedBy("0.2.0-rc.1"),
        ),
        (
            br#"{"openbindings":"0.3.0","operations":{},"operations":{}}"#.to_vec(),
            GovernedBy("0.3.0"),
        ),
        (
            br#"{"openbindings":"0.2.0","operations":{},"operations":{}}"#.to_vec(),
            Violates(&["OBI-01"]),
        ),
        (
            br#"{"openbindings":"0.2.0","description":"\ud800","operations":{}}"#.to_vec(),
            Violates(&["OBI-01"]),
        ),
        (
            bytes(&[
                br#"{"openbindings":"0.3.0","description":"x"#,
                b"\xff",
                br#"","operations":{}}"#,
            ]),
            Violates(&["OBI-01"]),
        ),
        (
            bytes(&[
                b"\xef\xbb\xbf",
                br#"{"openbindings":"0.3.0","operations":{}}"#,
            ]),
            Violates(&["OBI-01"]),
        ),
        (
            br#"{"openbindings":"0.2.0","openbindings":"0.3.0","operations":{}}"#.to_vec(),
            Violates(&["OBI-01"]),
        ),
        (
            br#"{"openbindings":"0.3","operations":{}}"#.to_vec(),
            Violates(&["OBI-02", "OBI-03"]),
        ),
        (
            br#"{"openbindings":2,"operations":{}}"#.to_vec(),
            Violates(&["OBI-02", "OBI-03"]),
        ),
        (
            br#"{"operations":{}}"#.to_vec(),
            Violates(&["OBI-02", "OBI-03"]),
        ),
        // Repeated names deeper in a 0.3.0 text do not stop its declaration either.
        (
            br#"{"openbindings":"0.3.0","x":{"a":1,"a":2}}"#.to_vec(),
            GovernedBy("0.3.0"),
        ),
    ];
    for (text, under) in rows {
        let shown = String::from_utf8_lossy(&text).into_owned();
        let result = validate_document(&text);
        match under {
            Conforms => assert_eq!(
                result.unwrap().report().conclusion,
                Conformance::Conformant,
                "{shown}"
            ),
            Violates(rules) => {
                let validation = result.unwrap();
                let violated: Vec<_> = validation
                    .report()
                    .evidence
                    .iter()
                    .filter(|(_, evidence)| **evidence == Evidence::Violated)
                    .map(|(rule, _)| *rule)
                    .collect();
                assert_eq!(violated, rules, "{shown}");
            }
            GovernedBy(version) => {
                let refusal = result.unwrap_err();
                assert_eq!(
                    (refusal.declared.as_str(), refusal.supported),
                    (version, "0.2.x")
                );
            }
        }
        // Wherever the text is admitted as JSON, a parsed document reaches the same answer.
        if let Ok(document) = Document::parse(&text) {
            assert_eq!(
                document.validate().map(|v| v.report().clone()),
                validate_document(&text).map(|v| v.report().clone()),
                "{shown}"
            );
        }
    }
}

#[test]
fn x_prefixed_operation_keys_enter_the_identifier_namespace() {
    // Specification §12: an `x-` key inside `operations` names an ordinary entry
    // and enters the identifier namespace OBI-05 keeps unique. An `x-` field of
    // the document or of an operation is an extension, never an identifier.
    for (members, collides, selected) in [
        (
            r#""operations":{"x-a":{},"b":{"aliases":["x-a"]}}"#,
            true,
            None,
        ),
        (
            r#""operations":{"x-a":{"aliases":["x-b"]},"c":{"aliases":["x-b"]}}"#,
            true,
            None,
        ),
        (
            r#""operations":{"x-a":{},"b":{"aliases":["x-c"]}}"#,
            false,
            Some("x-a"),
        ),
        (
            r#""x-a":{},"operations":{"b":{"aliases":["x-a"]}}"#,
            false,
            Some("b"),
        ),
        (
            r#""operations":{"b":{"x-a":{},"aliases":["x-a"]}}"#,
            false,
            Some("b"),
        ),
    ] {
        let text = format!(r#"{{"openbindings":"0.2.0",{members}}}"#);
        let validation = checked(&text);
        let report = validation.report();
        if collides {
            assert_eq!(report.conclusion, Conformance::NonConformant, "{text}");
            assert_eq!(report.evidence["OBI-05"], Evidence::Violated, "{text}");
            assert!(
                report
                    .evidence
                    .iter()
                    .all(|(rule, e)| *rule == "OBI-05" || *e == Evidence::Met),
                "{text}"
            );
        } else {
            assert_eq!(report.conclusion, Conformance::Conformant, "{text}");
            let proof = validation.conformant().unwrap();
            let key = proof.resolve_operation("x-a").map(|op| op.key().to_owned());
            assert_eq!(key.as_deref(), selected, "{text}");
        }
    }
}

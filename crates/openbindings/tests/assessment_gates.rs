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
        assert_eq!(validation.document().unwrap().to_bytes(), text.as_bytes());
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
    for limits in [
        JsonLimits {
            max_bytes: 10,
            ..JsonLimits::default()
        },
        JsonLimits {
            max_nodes: 2,
            ..JsonLimits::default()
        },
        JsonLimits {
            max_depth: 1,
            ..JsonLimits::default()
        },
    ] {
        let validation = validate_document_with_limits(text, limits).unwrap();
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

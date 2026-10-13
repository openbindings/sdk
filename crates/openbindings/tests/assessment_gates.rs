use openbindings::*;

fn checked(text: &str) -> DocumentAssessment {
    assess_document(text).unwrap()
}

#[test]
fn independent_violations_survive_unpaired_and_deep_opaque_data() {
    // CR-3: every row of the review's evidence table.
    for (text, rules) in [
        (r#"{"openbindings":"0.2.0","description":"\ud800"}"#.to_owned(), vec!["OBI-02"]),
        (r#"{"openbindings":"0.2.0","operations":{"-bad":{}},"description":"\ud800"}"#.into(), vec!["OBI-02", "OBI-04"]),
        (r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"missing","source":"s","content":"\ud800"}}}"#.into(), vec!["OBI-06"]),
        (r##"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"#/nope"}}},"x-data":"\ud800"}"##.into(), vec!["OBI-12"]),
        (r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$schema":"draft-07"}}},"x-data":"\ud800"}"#.into(), vec!["OBI-02", "OBI-09"]),
        (format!(r#"{{"openbindings":"0.2.0","x-d":{}0{}}}"#, "[".repeat(10001), "]".repeat(10001)), vec!["OBI-02"]),
        (format!(r#"{{"openbindings":"0.2.0","operations":{{"-bad":{{}}}},"x-d":{}0{}}}"#, "[".repeat(10001), "]".repeat(10001)), vec!["OBI-04"]),
    ] {
        let assessment = checked(&text);
        assert_eq!(assessment.report().conclusion, Conformance::NonConformant, "{rules:?}");
        for rule in rules { assert_eq!(assessment.report().evidence[rule], Evidence::Violated, "{rule}"); }
    }
}

#[test]
fn opaque_and_fixed_schema_strings_are_lossless() {
    // CR-5: scalar conversion is unnecessary for opaque data and ASCII constraints.
    for fields in [
        r#""description":"cut \ud83d""#,
        r#""name":"\ud800","version":"\ud800""#,
        r#""sources":{"s":{"kind":"\ud800","content":{"\ud800":"\udc00"}}}"#,
        r#""x-\ud800":{"\ud800":"\udc00"}"#,
        r#""schemas":{"s":{"description":"\ud800","pattern":"\ud800","const":"\ud800","required":["\ud800","\udc00","\ue000d800;"]}}"#,
        r#""dependencies":{"d":{"operation":"op","kinds":["\ud800","\udc00","\ue000d800;"]}}"#,
    ] {
        let text = format!(r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},{fields}}}"#);
        let assessment = checked(&text);
        assert_eq!(
            assessment.report().conclusion,
            Conformance::Conformant,
            "{fields}: {:?}",
            assessment.report().findings
        );
        assert_eq!(
            assessment.validated().unwrap().original_bytes(),
            text.as_bytes()
        );
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
        let assessment = checked(&format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{}}}},{field}}}"#
        ));
        assert_eq!(
            assessment.report().evidence[rule],
            Evidence::Violated,
            "{field}"
        );
    }
}

#[test]
fn native_names_compare_utf16_units_even_when_grammar_is_violated() {
    // CR-3/CR-5: invalid names still participate in exact namespace/reference rules.
    let assessment = checked(
        r#"{"openbindings":"0.2.0","operations":{"\ud800":{"aliases":["\uD800"]}},"sources":{"\udc00":{"kind":"k"}},"bindings":{"b":{"operation":"\uD800","source":"\uDC00"}},"dependencies":{"d":{"operation":"\ud800"}}}"#,
    );
    let evidence = &assessment.report().evidence;
    assert_eq!(evidence["OBI-04"], Evidence::Violated);
    assert_eq!(evidence["OBI-05"], Evidence::Violated);
    for rule in ["OBI-06", "OBI-07", "OBI-08"] {
        assert_eq!(evidence[rule], Evidence::Satisfied);
    }
}

#[test]
fn local_meta_checks_at_arbitrary_depth_on_small_stack() {
    // CR-4: full API, cold compilation is also covered in this test executable.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            for depth in [256, 257, 300, 12000] {
                for bad in [false, true] {
                    let leaf = if bad { r#"{"type":42}"# } else { "{}" };
                    let schema =
                        format!("{}{leaf}{}", r#"{"not":"#.repeat(depth), "}".repeat(depth));
                    let text = format!(
                        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
                    );
                    let assessment = checked(&text);
                    assert_eq!(
                        assessment.report().conclusion,
                        if bad {
                            Conformance::NonConformant
                        } else {
                            Conformance::Conformant
                        },
                        "depth {depth}"
                    );
                    assert_eq!(
                        assessment.report().evidence["OBI-10"],
                        if bad {
                            Evidence::Violated
                        } else {
                            Evidence::Satisfied
                        }
                    );
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn assessment_admission_is_caller_selected_and_precedes_version() {
    // CR-9 and HD-12: all three limits precede a complete version declaration.
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
        let assessment = assess_document_with_limits(text, limits).unwrap();
        assert_eq!(assessment.report().conclusion, Conformance::Undetermined);
        assert!(
            assessment
                .report()
                .evidence
                .values()
                .all(|v| *v == Evidence::Inconclusive)
        );
        assert!(
            assessment
                .report()
                .findings
                .iter()
                .all(|f| f.code == "input-limit")
        );
    }
    assert!(assess_document_with_limits(text, JsonLimits::default()).is_err());
    let text = format!(
        r#"{{"openbindings":"0.2.0","operations":{{}},"x-data":[{}0]}}"#,
        "0,".repeat(1_000_000)
    );
    assert_eq!(
        checked(&text).report().conclusion,
        Conformance::Undetermined
    );
    let raised = assess_document_with_limits(
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
    // N-1/CR-6: stable machine codes and concise human text are separate.
    for (text, code, message) in [
        ("{x}", "object-key", "expected a quoted object member name"),
        (
            "[1 2]",
            "comma-or-close",
            "expected a comma or the end of the container",
        ),
        ("1e", "exponent", "expected a digit in the number exponent"),
    ] {
        let assessment = checked(text);
        let finding = &assessment.report().findings[0];
        assert_eq!(finding.rule, "OBI-01");
        assert_eq!(finding.code, code);
        assert_eq!(finding.message, message);
        assert!(finding.location.is_some());
    }
}

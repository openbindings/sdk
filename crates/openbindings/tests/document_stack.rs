use openbindings::*;

fn checked(text: &str) -> DocumentValidation {
    validate_document(text).unwrap()
}

#[test]
fn local_meta_checks_at_arbitrary_depth_on_small_stack() {
    // The only test in this executable: no other test can warm the schema caches.
    // Both the document findings path and localized meta path compile cold.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let failed = checked(r#"{"openbindings":"0.2.0","name":42}"#);
            assert_eq!(failed.report().evidence["OBI-02"], Evidence::Violated);
            let locations: Vec<_> = failed
                .report()
                .findings
                .iter()
                .filter(|finding| finding.rule == "OBI-02")
                .map(|finding| {
                    finding
                        .location
                        .as_ref()
                        .unwrap()
                        .pointer
                        .as_ref()
                        .and_then(Pointer::as_str)
                        .unwrap()
                })
                .collect();
            assert_eq!(locations, ["/name", ""]);
            assert!(!failed.report().findings_truncated);
            for depth in [256, 257, 300, 12000] {
                for bad in [false, true] {
                    let leaf = if bad { r#"{"type":42}"# } else { "{}" };
                    let schema =
                        format!("{}{leaf}{}", r#"{"not":"#.repeat(depth), "}".repeat(depth));
                    let text = format!(
                        r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{schema}}}}}}}"#
                    );
                    let validation = checked(&text);
                    assert_eq!(
                        validation.report().conclusion,
                        if bad {
                            Conformance::NonConformant
                        } else {
                            Conformance::Conformant
                        },
                        "depth {depth}"
                    );
                    assert_eq!(
                        validation.report().evidence["OBI-10"],
                        if bad {
                            Evidence::Violated
                        } else {
                            Evidence::Met
                        }
                    );
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

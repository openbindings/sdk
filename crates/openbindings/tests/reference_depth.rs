use openbindings::ReferenceReport;
#[path = "common/reference_report.rs"]
mod reference_report;
use openbindings::{
    Document, Evidence, ReferenceRefusalReason, ReferenceResolution, validate_document,
};
use reference_report::{complete, incomplete};

#[test]
fn deep_references_agree_with_validation_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            for depth in [300, 2_000] {
                for named in [false, true] {
                    for present in [false, true] {
                        let target = if present { "target" } else { "missing" };
                        let (id, reference) = if named {
                            (r#""$id":"https://example.test/root","#,
                             format!("#/$defs/{target}"))
                        } else {
                            ("", format!("#/schemas/entry/$defs/{target}"))
                        };
                        let text = format!(
                            r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{{{id}"$defs":{{"target":true}},"not":{}{{"$ref":"{reference}"}}{}}}}}}}"##,
                            r#"{"not":"#.repeat(depth - 1),
                            "}".repeat(depth - 1)
                        );
                        let validation = validate_document(&text).unwrap();
                        assert_eq!(
                            validation.report().evidence["OBI-12"],
                            if present { Evidence::Met } else { Evidence::Violated }
                        );
                        let report = Document::parse(&text).unwrap().references();
                        assert!(matches!(report, ReferenceReport::Complete { .. }));
                        assert_eq!(complete(&report).len(), 1);
                        let observed = &complete(&report)[0];
                        let holder = format!("/schemas/entry{}", "/not".repeat(depth));
                        assert_eq!(observed.location.pointer, format!("{holder}/$ref"));
                        assert_eq!(observed.spelling.as_deref(), Some(reference.as_str()));
                        match &observed.resolution {
                            ReferenceResolution::Located { target, dynamic_lookup } => {
                                assert!(present);
                                assert!(!dynamic_lookup);
                                assert_eq!(target.pointer, "/schemas/entry/$defs/target");
                                assert!(target.source_uri.is_none());
                            }
                            ReferenceResolution::Unresolved { detail } => {
                                assert!(!present);
                                assert_eq!(detail.reason, ReferenceRefusalReason::MissingTarget);
                                assert_eq!(detail.code, "missing-target");
                                assert_eq!(detail.location.as_ref().unwrap().pointer, holder);
                            }
                        }
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deep_relative_identifiers_stop_at_the_identity_budget_on_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            // The identity budget counts expanded declarations before allocating
            // them. This family fits through 2,359 levels and exceeds 8 MiB at
            // 2,360, independently of the one reference's pointer allowance.
            for depth in [300, 600, 1_200, 2_000, 2_359, 2_360] {
                let text = format!(
                    r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"entry":{{"$id":"https://e.test/","not":{}{{"$ref":"#"}}{}}}}}}}"##,
                    r#"{"$id":"a/","not":"#.repeat(depth),
                    "}".repeat(depth)
                );
                let validation = validate_document(&text).unwrap();
                assert_eq!(validation.report().evidence["OBI-12"], Evidence::Met);
                let report = Document::parse(&text).unwrap().references();
                if depth == 2_360 {
                    assert!(matches!(report, ReferenceReport::Incomplete { .. }));
                    assert!(incomplete(&report).0.is_empty());
                    let limitation = incomplete(&report).1;
                    assert_eq!(limitation.reason, ReferenceRefusalReason::LimitExceeded);
                    assert_eq!(limitation.code, "schema-identity-bytes");
                    assert!(limitation.location.is_none());
                } else {
                    assert!(matches!(report, ReferenceReport::Complete { .. }));
                    assert_eq!(complete(&report).len(), 1);
                    let ReferenceResolution::Located { target, dynamic_lookup } =
                        &complete(&report)[0].resolution
                    else {
                        panic!("deepest relative resource must resolve")
                    };
                    assert!(!dynamic_lookup);
                    assert_eq!(target.pointer, format!("/schemas/entry{}", "/not".repeat(depth)));
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deep_reference_locations_still_obey_the_aggregate_report_allowance() {
    let depth = 2_000;
    let references = 1_500;
    let text = format!(
        r##"{{"openbindings":"0.2.0","operations":{{}},"schemas":{{"target":true,"entry":{}{{"allOf":[{}]}}{}}}}}"##,
        r#"{"not":"#.repeat(depth),
        vec![r##"{"$ref":"#/schemas/target"}"##; references].join(","),
        "}".repeat(depth)
    );
    let report = Document::parse(text).unwrap().references();
    assert!(matches!(report, ReferenceReport::Incomplete { .. }));
    let limitation = incomplete(&report).1;
    assert_eq!(limitation.reason, ReferenceRefusalReason::LimitExceeded);
    assert_eq!(limitation.code, "reference-report-bytes");
    assert!(limitation.location.is_none());
    assert!(!incomplete(&report).0.is_empty());
    assert!(incomplete(&report).0.len() < references);
    let prefix = format!("/schemas/entry{}/allOf", "/not".repeat(depth));
    let mut retained = 0;
    for (index, reference) in incomplete(&report).0.iter().enumerate() {
        assert_eq!(reference.location.pointer, format!("{prefix}/{index}/$ref"));
        let ReferenceResolution::Located { target, .. } = &reference.resolution else {
            panic!("present target")
        };
        assert_eq!(target.pointer, "/schemas/target");
        retained += reference.location.pointer.as_str().unwrap().len();
        retained += target.pointer.as_str().unwrap().len();
        retained += reference.keyword.len() + reference.spelling.as_ref().unwrap().len();
    }
    let next = format!("{prefix}/{}/$ref", incomplete(&report).0.len());
    assert!(retained <= 8 * 1024 * 1024);
    assert!(
        retained + next.len() + "/schemas/target".len() + "$ref".len() + "#/schemas/target".len()
            > 8 * 1024 * 1024
    );
}

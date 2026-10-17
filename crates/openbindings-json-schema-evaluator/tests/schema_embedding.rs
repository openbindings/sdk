//! Schema-embedding regressions: inspection must not alter authored identity or truth.
use openbindings::*;
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::atomic::AtomicBool;

const BASE: &str = "https://probe.invalid/root";

/// Prepare operation `op` input of a conformant document with the default evaluator.
fn prepare_input(doc: &Document, resources: &ResourceSet) -> Result<PreparedContract, NoVerdict> {
    doc.validate()
        .unwrap()
        .conformant()
        .unwrap()
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(&DefaultEvaluator::new(), resources)
}

fn fixture(schema: &str, reference: &str, supplied: bool) -> (Document, ResourceSet) {
    let (schemas, resources) = if supplied {
        (
            "{}".into(),
            ResourceSet::new([SchemaResource {
                uri: BASE.into(),
                document: JsonValue::parse(schema).unwrap(),
            }])
            .unwrap(),
        )
    } else {
        (format!(r#"{{"R":{schema}}}"#), ResourceSet::default())
    };
    let reference = serde_json::to_string(reference).unwrap();
    let doc = Document::parse(format!(r#"{{"openbindings":"0.2.0","schemas":{schemas},"operations":{{"op":{{"input":{{"$ref":{reference}}}}}}}}}"#)).unwrap();
    let validation = doc.validate().unwrap();
    if !supplied && schema.contains(r"\ud800") {
        assert_eq!(validation.report().evidence["OBI-01"], Evidence::Violated);
        assert!(validation.conformant().is_none());
    } else {
        assert!(validation.conformant().is_some());
    }
    (doc, resources)
}

#[test]
fn contained_draft_and_supplied_surrogate_collision_preserves_each_original_target() {
    // The collision remains an evaluator test through an external resource;
    // the same text inside a document is now excluded by OBI-01.
    for supplied in [false, true] {
        for reverse in [false, true] {
            for (suffix, fragment) in [
                ("", "%EE%80%80d800%3B"),
                ("", "%ee%80%80d800%3b"),
                ("/~", "%EE%80%80d800%3B~1~0"),
                ("%2F", "%EE%80%80d800%3B%252F"),
            ] {
                let literal = format!(r#""\ue000d800;{suffix}":{{"type":"string"}}"#);
                let surrogate = r#""\ud800":{"type":"integer"}"#;
                let defs = if reverse {
                    format!("{surrogate},{literal}")
                } else {
                    format!("{literal},{surrogate}")
                };
                let schema = format!(
                    r##"{{"$id":"{BASE}","$defs":{{{defs},"alias":{{"$ref":"#/$defs/{fragment}"}}}}}}"##
                );
                for via_alias in [false, true] {
                    let reference = if via_alias {
                        format!("{BASE}#/$defs/alias")
                    } else {
                        format!("{BASE}#/$defs/{fragment}")
                    };
                    let (doc, resources) = fixture(&schema, &reference, supplied);
                    let report = doc
                        .references_with_resources(resources.clone(), &AtomicBool::new(false))
                        .unwrap();
                    let found = report
                        .references
                        .iter()
                        .find(|r| r.location.pointer == "/operations/op/input/$ref")
                        .unwrap();
                    let expected_prefix = if supplied { "" } else { "/schemas/R" };
                    let literal_pointer = format!(
                        "{expected_prefix}/$defs/{}",
                        format!("\u{e000}d800;{suffix}")
                            .replace('~', "~0")
                            .replace('/', "~1")
                    );
                    let expected_target = if via_alias {
                        format!("{expected_prefix}/$defs/alias")
                    } else {
                        literal_pointer.clone()
                    };
                    assert!(
                        matches!(&found.resolution, ReferenceResolution::Located { target, .. }
                        if target.pointer == expected_target && target.resource.as_deref() == supplied.then_some(BASE)),
                        "{found:?}"
                    );
                    // Retain draft reference inspection above. Evaluation uses
                    // the same exact schema as a supplied resource in both cases.
                    let (doc, resources) = fixture(&schema, &reference, true);
                    let literal_pointer = literal_pointer
                        .strip_prefix("/schemas/R")
                        .unwrap_or(&literal_pointer);
                    let Ok(contract) = prepare_input(&doc, &resources) else {
                        panic!("expected ready")
                    };
                    assert!(matches!(
                        contract.explain(&JsonValue::parse(r#""s""#).unwrap()),
                        ValueOutcome::Satisfies
                    ));
                    let ValueOutcome::Fails { problems, .. } =
                        contract.explain(&JsonValue::parse("3").unwrap())
                    else {
                        panic!("expected string mismatch")
                    };
                    assert!(
                        problems.iter().any(|p| p
                            .schema_location
                            .as_ref()
                            .is_some_and(|at| at.pointer == format!("{literal_pointer}/type")
                                && at.resource.as_deref() == Some(BASE))),
                        "{problems:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn inspection_root_never_masks_missing_references_including_suffix_candidates() {
    const OLD: &str = "urn:openbindings:missing-inspection";
    for dominant in [false, true] {
        for suffix in ["", "-", "--"] {
            let uri = format!("{OLD}{suffix}");
            // Occupy earlier candidates as both declared identities and references.
            let input = if dominant {
                format!(r#"{{"anyOf":[true,{{"$ref":"{uri}"}}]}}"#)
            } else {
                format!(r#"{{"$ref":"{uri}"}}"#)
            };
            let schemas = (0..suffix.len())
                .map(|n| {
                    format!(
                        r#""r{n}":{{"$id":"{OLD}{}","$ref":"{OLD}---"}}"#,
                        "-".repeat(n)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let doc = Document::parse(format!(r#"{{"openbindings":"0.2.0","schemas":{{{schemas}}},"operations":{{"op":{{"input":{input}}}}}}}"#)).unwrap();
            let report = doc.references().unwrap();
            assert!(report.complete);
            assert!(report.references.iter().all(|r| matches!(&r.resolution, ReferenceResolution::Unresolved { detail } if detail.reason == ReferenceRefusalReason::ResourceUnavailable && detail.code == "resource-unavailable")), "{report:?}");
            let Ok(contract) = prepare_input(&doc, &ResourceSet::default()) else {
                panic!("expected ready")
            };
            for value in ["null", r#""s""#, "3", "{}", "[]"] {
                let outcome = contract.explain(&JsonValue::parse(value).unwrap());
                if dominant {
                    assert!(matches!(outcome, ValueOutcome::Satisfies));
                } else {
                    assert!(
                        matches!(outcome, ValueOutcome::NoVerdict { detail } if detail.reason == NoVerdictReason::ResourceUnavailable)
                    );
                }
            }
        }
    }
}

#[test]
fn unused_malformed_reference_sibling_is_isolated_but_malformed_id_stays_refused() {
    for supplied in [false, true] {
        for bad in [r#"{"not":{"$ref":"%"}}"#, r#"{"$id":"%"}"#] {
            let schema =
                format!(r#"{{"$id":"{BASE}","$defs":{{"good":{{"type":"string"}},"bad":{bad}}}}}"#);
            let (doc, resources) = fixture(&schema, &format!("{BASE}#/$defs/good"), supplied);
            let report = doc
                .references_with_resources(resources.clone(), &AtomicBool::new(false))
                .unwrap();
            let reference = report
                .references
                .iter()
                .find(|r| r.location.pointer == "/operations/op/input/$ref")
                .unwrap();
            if bad.contains("$ref") {
                assert!(matches!(
                    &reference.resolution,
                    ReferenceResolution::Located { .. }
                ));
            } else {
                assert!(
                    matches!(&reference.resolution, ReferenceResolution::Unresolved { detail } if detail.code == "resource-inspection")
                );
            }
            let prepared = prepare_input(&doc, &resources);
            if bad.contains("$ref") {
                let contract = match prepared {
                    Ok(contract) => contract,
                    Err(detail) => panic!("{detail:?}"),
                };
                assert!(matches!(
                    contract.explain(&JsonValue::parse(r#""s""#).unwrap()),
                    ValueOutcome::Satisfies
                ));
                assert!(matches!(
                    contract.explain(&JsonValue::parse("1").unwrap()),
                    ValueOutcome::Fails { .. }
                ));
            } else {
                assert!(
                    matches!(prepared, Err(detail) if detail.reason == NoVerdictReason::ConservativePreparation && detail.code == "resource-inspection")
                );
            }
        }
    }
}

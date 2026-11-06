use openbindings::JsonValue;
use openbindings_source_pull::*;
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn result(additions: &str, report: &str) -> JsonValue {
    json(&format!(r#"{{"additions":{additions},"report":{report}}}"#))
}

#[test]
fn all_37_pinned_pull_report_cases_keep_shape_and_complete_answers() {
    let pack = json(include_str!("fixtures/pull-report.json"));
    let mut positive = 0;
    let mut negative = 0;
    let mut mismatches = Vec::new();
    for case in pack.get("tests").unwrap().elements().unwrap() {
        let value = result(
            case.get("additions").unwrap().text(),
            case.get("report").unwrap().text(),
        );
        let admitted = PullResult::from_json(value.clone());
        let expected = case.get("schemaValid").unwrap().as_bool().unwrap();
        if admitted.is_ok() != expected {
            mismatches.push(format!(
                "{}: {admitted:?}",
                case.get("description").unwrap().text()
            ));
        }
        if expected {
            positive += 1;
            let Ok(admitted) = admitted else { continue };
            assert_eq!(
                admitted.report().is_complete(),
                case.get("completePull").unwrap().as_bool().unwrap()
            );
            assert_eq!(admitted.to_json().value().text(), value.text());
        } else {
            negative += 1;
        }
    }
    assert_eq!((positive, negative), (26, 11));
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
#[test]
fn all_five_published_example_results_are_admitted_without_projection_or_edits() {
    let publication = json(include_str!("fixtures/source-puller.json"));
    let examples = publication
        .get("operations")
        .unwrap()
        .get("openbindings.source-puller.pullSource")
        .unwrap()
        .get("examples")
        .unwrap();
    assert_eq!(examples.members().unwrap().len(), 5);
    for example in examples.members().unwrap() {
        let value = example.value.get("output").unwrap().to_owned();
        let result = PullResult::from_json(value.clone()).unwrap();
        assert_eq!(result.to_json().value().text(), value.text());
    }
}
#[test]
fn opaque_output_fields_and_owned_children_keep_exact_values_after_parent_drop() {
    let report = {
        let value = json(
            r#"{"additions":{"x-note":{"n":1e999999}},"report":{"exhaustive":true,"added":[],"application":{"wide":9007199254740993,"units":"\ud800"},"existing":[{"source":"s","status":"stale","change":"dangerous","proposal":{"data":null,"application":{"n":9007199254740993}}}]},"app":false}"#,
        );
        let result = PullResult::from_json(value.clone()).unwrap();
        assert_eq!(result.to_json().value().text(), value.text());
        result.report()
    };
    assert_eq!(
        report
            .value()
            .value()
            .get("application")
            .unwrap()
            .get("wide")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    let entry = report.existing().unwrap().next().unwrap();
    let proposal = entry.proposal().unwrap();
    drop(entry);
    drop(report);
    let Proposal::Source(proposal) = proposal else {
        panic!("source proposal")
    };
    assert_eq!(proposal.data().unwrap().text(), "null");
    assert_eq!(
        proposal
            .value()
            .value()
            .get("application")
            .unwrap()
            .get("n")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
}
#[test]
fn proposal_authority_is_target_local_and_unknown_metadata_never_gains_edit_power() {
    for name in [
        "operation",
        "source",
        "description",
        "preference",
        "idempotent",
        "deprecated",
        "aliases",
        "examples",
        "tags",
    ] {
        let proposal = json(&format!(r#"{{"data":null,"{name}":null}}"#));
        assert_eq!(
            BindingProposal::from_json(proposal).unwrap_err(),
            CarrierError::ProposalAuthority
        );
    }
    for name in ["kind", "description"] {
        assert_eq!(
            SourceProposal::from_json(json(&format!(r#"{{"data":null,"{name}":"new"}}"#)))
                .unwrap_err(),
            CarrierError::ProposalAuthority
        );
    }
    assert!(
        BindingProposal::from_json(json(
            r#"{"data":null,"dependencies":{"application":"metadata"}}"#
        ))
        .is_ok()
    );
    let data = r#"{"data":{"description":"opaque","operation":"opaque","source":"opaque","preference":1,"idempotent":true},"application":{"description":"metadata"}}"#;
    assert!(BindingProposal::from_json(json(data)).is_ok());
    assert!(SourceProposal::from_json(json(data)).is_ok());
    let entry=ExistingEntry::from_json(json(r#"{"schema":"S","status":"stale","change":"dangerous","proposal":{"schema":{"description":"schema annotation","uses":["opaque", "opaque"],"type":"string"}}}"#)).unwrap();
    assert!(matches!(entry.proposal(), Some(Proposal::Schema(_))));
    for schema in ["true", "false", "{}"] {
        assert!(
            ExistingEntry::from_json(json(&format!(
                r#"{{"schema":"S","status":"stale","change":"dangerous","proposal":{{"schema":{schema}}}}}"#
            )))
            .is_ok()
        );
    }
}
#[test]
fn absence_null_and_removal_only_drift_are_not_interchangeable() {
    let proposal = BindingProposal::from_json(json(r#"{"input":true}"#)).unwrap();
    assert!(proposal.data().is_none());
    assert!(proposal.output().is_none());
    let proposal = BindingProposal::from_json(json(r#"{"data":null}"#)).unwrap();
    assert_eq!(proposal.data().unwrap().text(), "null");
    assert_eq!(
        BindingProposal::from_json(json(r#"{"uses":[]}"#)).unwrap_err(),
        CarrierError::EmptyBindingProposal
    );
    for proposal in [r#"{"input":null}"#, r#"{"output":null}"#] {
        assert!(BindingProposal::from_json(json(proposal)).is_err());
    }
    assert_eq!(
        SourceProposal::from_json(json("{}")).unwrap_err(),
        CarrierError::MissingField("data")
    );
    let absent = PullReport::from_json(json(r#"{"exhaustive":true,"added":[]}"#)).unwrap();
    assert!(absent.existing().is_none());
    assert!(absent.omitted().is_none());
    let empty = PullReport::from_json(json(
        r#"{"exhaustive":true,"added":[],"existing":[],"omitted":[]}"#,
    ))
    .unwrap();
    assert_eq!(empty.existing().unwrap().len(), 0);
    assert_eq!(empty.omitted().unwrap().len(), 0);
}
#[test]
fn complete_pull_checks_exhaustive_and_all_assessment_gaps() {
    for status in [
        "excluded",
        "invalid",
        "conflict",
        "implementation-unsupported",
    ] {
        let conflict = if status == "conflict" {
            r#", "existing":"operations/o", "offered":{}"#
        } else {
            ""
        };
        let report = format!(
            r#"{{"exhaustive":true,"added":[{{"binding":"b","operation":"o","lossy":[{{"reasonCode":"approximation","message":""}}]}}],"omitted":[{{"status":"{status}","reasonCode":"kind.reason","message":"","rule":"kind.rule@1"{conflict}}}]}}"#
        );
        assert_eq!(
            PullReport::from_json(json(&report)).unwrap().is_complete(),
            status != "implementation-unsupported"
        );
    }
    assert!(PullLimitation::from_json(json(r#"{"code":"code","message":""}"#)).is_err());
    assert!(
        ExistingEntry::from_json(json(r#"{"binding":"b","status":"current","message":""}"#))
            .is_err()
    );
    assert!(
        ExistingEntry::from_json(json(r#"{"binding":"b","status":"unmatched","message":""}"#))
            .is_ok()
    );
}
#[test]
fn duplicate_members_and_uses_are_rejected_without_rewriting_nested_opaque_data() {
    assert_eq!(
        PullReport::from_json(json(r#"{"exhaustive":true,"added":[],"\u0061dded":[]}"#))
            .unwrap_err(),
        CarrierError::DuplicateMember
    );
    assert_eq!(
        AddedEntry::from_json(json(
            r#"{"binding":"b","operation":"o","uses":["schemas/A","schemas/\u0041"]}"#
        ))
        .unwrap_err(),
        CarrierError::DuplicateUse
    );
    let proposal = BindingProposal::from_json(json(r#"{"data":{"x":1,"x":2}}"#)).unwrap();
    assert!(proposal.data().unwrap().has_duplicate_names());
}
#[test]
fn binding_carrier_preferences_use_exact_integer_and_safe_range_rules() {
    for (number, valid) in [
        ("1.0", true),
        ("9007199254740991", true),
        ("-9007199254740991", true),
        ("9007199254740992", false),
        ("-9007199254740992", false),
        ("1.0000000000000000001", false),
        ("0e999999", true),
    ] {
        let value = json(&format!(
            r#"{{"bindings":{{"b":{{"operation":"o","source":"s","preference":{number}}}}}}}"#
        ));
        assert_eq!(PullAdditions::from_json(value).is_ok(), valid, "{number}");
    }
}

#[test]
fn removal_lists_and_schema_proposals_keep_absence_distinct_from_null() {
    for text in [
        r#"{"remove":[]}"#,
        r#"{"remove":["output","output"]}"#,
        r#"{"remove":["output"],"output":true}"#,
        r#"{"remove":["description"]}"#,
    ] {
        assert!(BindingProposal::from_json(json(text)).is_err(), "{text}");
    }
    let proposal =
        BindingProposal::from_json(json(r#"{"remove":["data","input","output"]}"#)).unwrap();
    assert_eq!(proposal.remove().unwrap().len(), 3);
    for text in [
        r#"{"remove":[]}"#,
        r#"{"remove":["data","data"]}"#,
        r#"{"remove":["input"]}"#,
        r#"{"data":null,"remove":["data"]}"#,
    ] {
        assert!(SourceProposal::from_json(json(text)).is_err(), "{text}");
    }
    let proposal = SourceProposal::from_json(json(r#"{"remove":["data"]}"#)).unwrap();
    assert!(proposal.data().is_none());
    assert_eq!(
        proposal.remove().unwrap().next().unwrap().as_str(),
        Some("data")
    );
    let proposal =
        SchemaProposal::from_json(json(r#"{"schema":false,"uses":["schemas/S"]}"#)).unwrap();
    assert_eq!(proposal.schema().view().as_bool(), Some(false));
    assert_eq!(proposal.uses().unwrap().len(), 1);
}

#[test]
fn published_closed_status_sets_and_open_metadata_are_distinct() {
    assert!(ExistingEntry::from_json(json(r#"{"binding":"b","status":"future"}"#)).is_err());
    assert!(
        ExistingEntry::from_json(json(
            r#"{"binding":"b","status":"stale","change":"future","proposal":{"data":null}}"#
        ))
        .is_err()
    );
    assert!(
        OmittedEntry::from_json(json(r#"{"status":"future","reasonCode":"r","message":""}"#))
            .is_err()
    );
    assert!(
        PullReport::from_json(json(
            r#"{"exhaustive":true,"added":[],"limitation":{"code":"r","message":"reason"}}"#
        ))
        .is_err()
    );
    for text in [
        r#"{"binding":"b","operation":"o","lossy":[]}"#,
        r#"{"binding":"b","operation":"o","lossy":{"reasonCode":"r","message":""}}"#,
    ] {
        assert!(AddedEntry::from_json(json(text)).is_err());
    }
    let entry = AddedEntry::from_json(json(r#"{"binding":"b","operation":"o","lossy":[{"reasonCode":"a","message":""},{"reasonCode":"b","message":"","future":true}]}"#)).unwrap();
    assert_eq!(entry.lossy().unwrap().len(), 2);
    let entry = ExistingEntry::from_json(json(r#"{"binding":"b","status":"unassessed","reasonCode":"r","message":"","references":["schemas/S"],"future":true}"#)).unwrap();
    assert_eq!(entry.status(), ExistingStatus::Unassessed);
    assert_eq!(entry.references().unwrap().len(), 1);
    for text in [
        r#"{"schema":"S","status":"current","references":["schemas/T"]}"#,
        r#"{"binding":"b","status":"current","references":[]}"#,
        r#"{"status":"invalid","reasonCode":"r","message":"","offered":true}"#,
    ] {
        let admitted = if text.contains("invalid") {
            OmittedEntry::from_json(json(text)).is_ok()
        } else {
            ExistingEntry::from_json(json(text)).is_ok()
        };
        assert!(!admitted, "{text}");
    }
}

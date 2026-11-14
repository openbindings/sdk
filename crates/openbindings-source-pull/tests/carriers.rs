use openbindings::JsonValue;
use openbindings_source_pull::*;
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn result(additions: &str, report: &str) -> JsonValue {
    json(&format!(r#"{{"additions":{additions},"report":{report}}}"#))
}

#[test]
fn all_38_pinned_pull_report_cases_keep_shape_and_complete_answers() {
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
            assert_eq!(admitted.value().value().text(), value.text());
        } else {
            negative += 1;
        }
    }
    assert_eq!((positive, negative), (27, 11));
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
        assert_eq!(result.value().value().text(), value.text());
    }
}
#[test]
fn opaque_output_fields_and_owned_children_keep_exact_values_after_parent_drop() {
    let report = {
        let value = json(
            r#"{"additions":{"x-note":{"n":1e999999}},"report":{"exhaustive":true,"added":[],"application":{"wide":9007199254740993,"units":"\ud800"},"existing":[{"source":"s","status":"stale","change":"dangerous","proposal":{"data":null,"application":{"n":9007199254740993}}}]},"app":false}"#,
        );
        let result = PullResult::from_json(value.clone()).unwrap();
        assert_eq!(result.value().value().text(), value.text());
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
            BindingProposal::from_json(proposal).unwrap_err().kind(),
            CarrierErrorKind::ProposalAuthority
        );
    }
    for name in ["kind", "description"] {
        assert_eq!(
            SourceProposal::from_json(json(&format!(r#"{{"data":null,"{name}":"new"}}"#)))
                .unwrap_err()
                .kind(),
            CarrierErrorKind::ProposalAuthority
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
        BindingProposal::from_json(json(r#"{"uses":[]}"#))
            .unwrap_err()
            .kind(),
        CarrierErrorKind::EmptyBindingProposal
    );
    for proposal in [r#"{"input":null}"#, r#"{"output":null}"#] {
        assert!(BindingProposal::from_json(json(proposal)).is_err());
    }
    assert_eq!(
        SourceProposal::from_json(json("{}")).unwrap_err().kind(),
        CarrierErrorKind::MissingField("data")
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
            .unwrap_err()
            .kind(),
        CarrierErrorKind::DuplicateMember
    );
    assert_eq!(
        AddedEntry::from_json(json(
            r#"{"binding":"b","operation":"o","uses":["schemas/A","schemas/\u0041"]}"#
        ))
        .unwrap_err()
        .kind(),
        CarrierErrorKind::DuplicateUse
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

#[test]
fn schema_proposal_uses_is_only_retained_unknown_metadata() {
    // The open proposal carrier retains additional members without treating
    // them as binding data needs, even when they are not a label list.
    for uses in [
        "null",
        "42",
        r#"["schemas/S","schemas/S"]"#,
        r#"["missing"]"#,
    ] {
        let value = json(&format!(r#"{{"schema":false,"uses":{uses}}}"#));
        let proposal = SchemaProposal::from_json(value.clone()).unwrap();
        assert_eq!(proposal.schema().view().as_bool(), Some(false));
        assert_eq!(proposal.value().value().text(), value.text());
        let entry = ExistingEntry::from_json(json(&format!(
            r#"{{"schema":"S","status":"stale","change":"dangerous","proposal":{}}}"#,
            value.text()
        )))
        .unwrap();
        let Proposal::Schema(proposal) = entry.proposal().unwrap() else {
            panic!("schema proposal")
        };
        assert_eq!(proposal.value().value().text(), value.text());
    }
}

#[test]
fn a_field_name_several_carriers_share_says_which_one() {
    let missing = |field| CarrierErrorKind::MissingField(field);
    assert_eq!(
        PullLoss::from_json(json(r#"{"reasonCode":"r"}"#))
            .unwrap_err()
            .kind(),
        missing("lossy/message")
    );
    assert_eq!(
        PullLimitation::from_json(json(r#"{"code":"c"}"#))
            .unwrap_err()
            .kind(),
        missing("limitation/message")
    );
    assert_eq!(
        ExistingEntry::from_json(json(r#"{"binding":"b"}"#))
            .unwrap_err()
            .kind(),
        missing("existing/status")
    );
    assert_eq!(
        ExistingEntry::from_json(json(
            r#"{"binding":"b","status":"unassessed","reasonCode":"r"}"#
        ))
        .unwrap_err()
        .kind(),
        missing("existing/message")
    );
    assert_eq!(
        OmittedEntry::from_json(json(r#"{"status":"invalid","reasonCode":"r"}"#))
            .unwrap_err()
            .kind(),
        missing("omitted/message")
    );
    assert_eq!(
        OmittedEntry::from_json(json(r#"{"status":"nope","reasonCode":"r","message":"m"}"#))
            .unwrap_err()
            .kind(),
        CarrierErrorKind::FieldType("omitted/status")
    );
    assert_eq!(
        OmittedEntry::from_json(json(
            r#"{"status":"conflict","reasonCode":"r","message":"m","offered":true}"#
        ))
        .unwrap_err()
        .kind(),
        missing("omitted/existing")
    );
    assert_eq!(
        ExistingEntry::from_json(json(r#"{"binding":"b","status":"current","details":1}"#))
            .unwrap_err()
            .kind(),
        CarrierErrorKind::StatusFields
    );
    assert_eq!(
        ExistingEntry::from_json(json(r#"{"binding":"b","status":"unmatched","details":1}"#))
            .unwrap_err()
            .kind(),
        CarrierErrorKind::FieldType("existing/details")
    );
    // The same message from a whole result, as an adapter's author would see it.
    let error = PullResult::from_json(json(
        r#"{"additions":{},"report":{"exhaustive":true,"added":[],"existing":[{"binding":"b","status":"unassessed","reasonCode":"r"}]}}"#,
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "the carrier requires existing/message");
}

#[test]
fn a_pull_request_takes_a_source_key_in_the_usual_string_forms() {
    let document = openbindings::Document::parse(
        r#"{"openbindings":"0.2.0","operations":{},"sources":{"s":{"kind":"example@1"}}}"#,
    )
    .unwrap()
    .validate()
    .unwrap()
    .conformant()
    .unwrap();
    let key = String::from("s");
    assert_eq!(PullRequest::new(document.clone(), &key).source(), "s");
    assert_eq!(PullRequest::new(document.clone(), "s").source(), "s");
    assert_eq!(PullRequest::new(document, key).source(), "s");
}

#[test]
fn omitted_entry_borrows_all_contract_fields_without_losing_exact_data() {
    let entry = OmittedEntry::from_json(json(
        r#"{
        "status":"conflict","reasonCode":"example.conflict","message":"",
        "item":"operations/offered","existing":"operations/first",
        "alsoIdentifies":["operations/second","operations/third"],
        "offered":{"input":{"const":9007199254740993},"x-unit":"\ud800"},
        "through":["schemas/S","operations/other/output"],"identifier":"urn:example:S"
    }"#,
    ))
    .unwrap();
    assert_eq!(entry.item().unwrap().as_str(), Some("operations/offered"));
    assert_eq!(entry.existing().unwrap().as_str(), Some("operations/first"));
    assert_eq!(
        entry
            .also_identifies()
            .unwrap()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["operations/second", "operations/third"]
    );
    assert_eq!(
        entry
            .through()
            .unwrap()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["schemas/S", "operations/other/output"]
    );
    assert_eq!(entry.identifier().unwrap().as_str(), Some("urn:example:S"));
    let offered = entry.offered().unwrap();
    assert_eq!(
        offered
            .get("input")
            .unwrap()
            .get("const")
            .unwrap()
            .number_text(),
        Some("9007199254740993")
    );
    assert_eq!(
        offered
            .get("x-unit")
            .unwrap()
            .as_string()
            .unwrap()
            .utf16_units()
            .collect::<Vec<_>>(),
        [0xd800]
    );
    // The borrowed view points at the original carrier's bytes, not a copy.
    assert_eq!(
        offered.text().as_ptr(),
        entry
            .value()
            .value()
            .get("offered")
            .unwrap()
            .text()
            .as_ptr()
    );
    let minimal = OmittedEntry::from_json(json(
        r#"{"status":"invalid","reasonCode":"r","message":""}"#,
    ))
    .unwrap();
    assert!(minimal.item().is_none());
    assert!(minimal.existing().is_none());
    assert!(minimal.also_identifies().is_none());
    assert!(minimal.offered().is_none());
    assert!(minimal.through().is_none());
    assert!(minimal.identifier().is_none());
    let boolean = OmittedEntry::from_json(json(r#"{"status":"conflict","reasonCode":"r","message":"","existing":"schemas/S","offered":false}"#)).unwrap();
    assert_eq!(boolean.offered().unwrap().as_bool(), Some(false));
}

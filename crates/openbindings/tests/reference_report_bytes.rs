use openbindings::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;

const ALLOWANCE: usize = 8 * 1024 * 1024;
fn origin_bytes(origin: &SchemaOrigin) -> usize {
    origin.source_uri.as_ref().map_or(0, String::len)
        + origin
            .pointer
            .as_str()
            .map_or_else(|| 2 * origin.pointer.utf16_units().count(), str::len)
}
fn entry_bytes(reference: &Reference) -> usize {
    origin_bytes(&reference.location)
        + reference.resource_root.as_ref().map_or(0, origin_bytes)
        + reference.keyword.len()
        + reference.spelling.as_ref().map_or(0, String::len)
        + match &reference.resolution {
            ReferenceResolution::Located { target, .. } => origin_bytes(target),
            ReferenceResolution::Unresolved { detail } => {
                detail.location.as_ref().map_or(0, origin_bytes)
                    + detail.resource_uri.as_ref().map_or(0, String::len)
                    + detail.code.len()
                    + detail.message.len()
            }
        }
}
fn assert_bounded(report: &ReferenceReport) -> usize {
    assert!(!report.complete);
    let limitation = report.limitation.as_ref().unwrap();
    assert_eq!(limitation.reason, ReferenceRefusalReason::LimitExceeded);
    assert_eq!(limitation.code, "reference-report-bytes");
    assert!(limitation.location.is_none());
    assert!(limitation.resource_uri.is_none());
    assert!(!report.references.is_empty());
    let bytes = report.references.iter().map(entry_bytes).sum::<usize>();
    assert!(bytes <= ALLOWANCE, "{bytes}");
    bytes
}

#[test]
fn missing_directory_names_charge_expansion_before_retention() {
    let base = format!("https://e.test/{}/", "a".repeat(1024 * 1024));
    let document = Document::parse(
        json!({"openbindings":"0.2.0","operations":{},"schemas":{"entry":{
            "$id":base,"allOf":(0..64).map(|i| json!({"$ref":format!("s{i}")})).collect::<Vec<_>>()
        }}})
        .to_string(),
    )
    .unwrap();
    let proof = document.validate().unwrap().conformant().unwrap();
    for report in [document.references(), proof.document().references()] {
        let bytes = assert_bounded(&report);
        assert_eq!(report.references.len(), 7);
        for (i, reference) in report.references.iter().enumerate() {
            assert_eq!(
                reference.location.pointer,
                format!("/schemas/entry/allOf/{i}/$ref")
            );
            let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
                panic!("missing")
            };
            assert_eq!(
                detail.resource_uri.as_deref(),
                Some(format!("{base}s{i}").as_str())
            );
        }
        // The next site and spelling have the same byte lengths through index 9.
        assert!(bytes + entry_bytes(&report.references[0]) > ALLOWANCE);
    }
}

#[test]
fn supplied_source_uris_charge_every_origin_copy() {
    let uri = format!("https://fetch.test/{}", "a".repeat(256 * 1024));
    for (nested, missing) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut external = json!({"allOf":(0..64).map(|_| json!({"$ref":if missing { "#absent" } else { "#" }})).collect::<Vec<_>>()});
        if nested {
            external = json!({"$defs":{"nested":{"$id":"https://id.test/nested", "allOf":external["allOf"]}}});
        }
        // The short identifier reaches a source with a long retrieval URI.
        external["$id"] = json!("https://id.test/top");
        let resources = ExternalSchemas::new([ExternalSchema {
            uri: uri.clone(),
            document: JsonValue::parse(external.to_string()).unwrap(),
        }])
        .unwrap();
        let document = Document::parse(r#"{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$ref":"https://id.test/top"}}}"#).unwrap();
        let proof = document.validate().unwrap().conformant().unwrap();
        for report in [
            document.references_with_resources(resources.clone(), &AtomicBool::new(false)),
            proof
                .document()
                .references_with_resources(resources.clone(), &AtomicBool::new(false)),
        ] {
            let bytes = assert_bounded(&report);
            assert_eq!(report.references.len(), 11);
            assert!(bytes + entry_bytes(&report.references[1]) > ALLOWANCE);
            for reference in &report.references[1..] {
                assert_eq!(reference.location.source_uri.as_ref(), Some(&uri));
                let root = reference.resource_root.as_ref().unwrap();
                assert_eq!(root.source_uri.as_ref(), Some(&uri));
                assert_eq!(root.pointer, if nested { "/$defs/nested" } else { "" });
            }
        }
    }
}

#[test]
fn source_spellings_use_utf8_bytes_and_share_the_report_allowance() {
    let spelling = "é".repeat(128 * 1024);
    let document = Document::parse(
        json!({"operations":{},"schemas":{"entry":{"allOf":vec![json!({"$ref":spelling});40]}}})
            .to_string(),
    )
    .unwrap();
    let report = document.references();
    let bytes = assert_bounded(&report);
    assert_eq!(report.references.len(), 31);
    assert!(bytes + entry_bytes(&report.references[0]) > ALLOWANCE);
    for reference in &report.references {
        assert_eq!(reference.spelling.as_deref(), Some(spelling.as_str()));
        let ReferenceResolution::Unresolved { detail } = &reference.resolution else {
            panic!("invalid URI")
        };
        assert_eq!(detail.reason, ReferenceRefusalReason::InvalidReference);
    }
}

#[test]
fn resource_root_pointers_share_the_allowance_in_both_representations() {
    for unpaired in [false, true] {
        let key = format!(
            "{}{}",
            "a".repeat(256 * 1024),
            if unpaired { r"\ud800" } else { "" }
        );
        let items = vec![r##"{"$ref":"#"}"##; 64].join(",");
        let document = Document::parse(format!(
            r#"{{"operations":{{}},"schemas":{{"{key}":{{"$id":"https://id.test/root","allOf":[{items}]}}}}}}"#
        )).unwrap();
        let report = document.references();
        let bytes = assert_bounded(&report);
        assert_eq!(report.references.len(), if unpaired { 5 } else { 10 });
        assert!(bytes + entry_bytes(&report.references[0]) > ALLOWANCE);
        for reference in &report.references {
            let root = reference.resource_root.as_ref().unwrap();
            let ReferenceResolution::Located { target, .. } = &reference.resolution else {
                panic!("located")
            };
            assert_eq!(root, target);
            assert_eq!(root.pointer.as_str().is_none(), unpaired);
        }
    }
}

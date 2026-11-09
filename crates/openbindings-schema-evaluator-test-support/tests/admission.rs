use openbindings::{Conformance, Document, Evidence};
use serde::Deserialize;

#[derive(Deserialize)]
struct Negative {
    id: String,
    #[serde(rename = "documentJson")]
    document: String,
    violates: Vec<String>,
}

#[test]
fn invalid_fixture_documents_report_required_admission_rules() {
    let cases: Vec<Negative> =
        serde_json::from_str(include_str!("../fixtures/current-admission.json")).unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        let document = Document::parse(&case.document).unwrap();
        let validation = document.validate().unwrap();
        assert_eq!(
            validation.report().conclusion,
            Conformance::NonConformant,
            "{}",
            case.id
        );
        assert!(validation.conformant().is_none(), "{}", case.id);
        for rule in case.violates {
            assert_eq!(
                validation.report().evidence[rule.as_str()],
                Evidence::Violated,
                "{}: {rule}",
                case.id
            );
        }
    }
}

#[derive(Deserialize)]
struct Reexpressions {
    reexpressions: Vec<Reexpression>,
}
#[derive(Deserialize)]
struct Reexpression {
    group: String,
    original_document: String,
    document: String,
    case_ids: Vec<String>,
}

#[test]
fn supplied_schema_reexpressions_keep_original_dialect_admission_negatives() {
    let changes: Reexpressions =
        serde_json::from_str(include_str!("../fixtures/current-reexpressions.json")).unwrap();
    assert_eq!(changes.reexpressions.len(), 4);
    assert_eq!(
        changes
            .reexpressions
            .iter()
            .map(|r| r.case_ids.len())
            .sum::<usize>(),
        9
    );
    for change in changes.reexpressions {
        let original = Document::parse(&change.original_document)
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            original.report().evidence["OBI-09"],
            Evidence::Violated,
            "{}",
            change.group
        );
        assert!(original.conformant().is_none());
        assert!(
            Document::parse(&change.document)
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .is_some(),
            "{}",
            change.group
        );
    }
}

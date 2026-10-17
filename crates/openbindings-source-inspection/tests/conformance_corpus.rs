//! The interfaces repository's published kind-support cases, run through the
//! inspection facade. The cases are a verbatim snapshot; `fixtures/README.md`
//! records provenance.
use openbindings::Source;
use openbindings_source_inspection::{
    CancellationToken, InspectionError, Inspector, SourceInspection, SourceInspector, SupportedKind,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    future::{Ready, ready},
};

const KIND_SUPPORT: &str = include_str!("fixtures/conformance/kind-support/cases.json");

/// A notional inspector with an exact warranted kind set and an advisory list.
struct Corpus {
    warranted: HashSet<String>,
    listed: Vec<String>,
}
impl SourceInspector for Corpus {
    type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        self.warranted.contains(kind)
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        self.listed
            .iter()
            .map(|kind| SupportedKind {
                kind: kind.clone(),
                description: None,
            })
            .collect()
    }
    fn inspect<'a>(&'a self, _: &'a Source, _: CancellationToken) -> Self::Inspection<'a> {
        ready(Ok(SourceInspection::complete(Vec::new())))
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn published_cases() -> Vec<Value> {
    let cases: Value = serde_json::from_str(KIND_SUPPORT).unwrap();
    assert_eq!(cases["format"], "openbindings.kind-support-cases@1");
    let cases = cases["tests"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 4, "every published kind-support case runs");
    cases
}

#[test]
fn published_kind_support_cases_hold_for_the_inspection_facade() {
    for case in published_cases() {
        let description = case["description"].as_str().unwrap();
        let listed = strings(&case["listed"]);
        let provider = Corpus {
            warranted: strings(&case["warranted"]).into_iter().collect(),
            listed: listed.clone(),
        };
        let facade = Inspector::new(&provider);
        let verdicts: Vec<_> = facade
            .check_kind_support(strings(&case["kinds"]))
            .unwrap()
            .into_iter()
            .map(|verdict| (verdict.kind, verdict.supported))
            .collect();
        let expected: Vec<_> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|verdict| {
                (
                    verdict["kind"].as_str().unwrap().to_owned(),
                    verdict["supported"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(verdicts, expected, "{description}");
        let advisory: Vec<_> = facade
            .list_supported_kinds()
            .unwrap()
            .into_iter()
            .map(|kind| kind.kind)
            .collect();
        assert_eq!(advisory, listed, "{description}");
    }
}

// The three companion facades name an empty requested kind alike.
#[test]
fn empty_requested_kind_is_refused_with_the_empty_kind_code() {
    let provider = Corpus {
        warranted: HashSet::new(),
        listed: Vec::new(),
    };
    let error = Inspector::new(&provider)
        .check_kind_support(["example.kind", ""])
        .unwrap_err();
    assert_eq!(
        error.kind(),
        openbindings_source_inspection::InspectionErrorKind::InvalidInput
    );
    assert_eq!(error.code(), "openbindings.inspection.empty-kind");
}

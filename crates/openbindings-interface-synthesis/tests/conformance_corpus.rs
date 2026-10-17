//! The interfaces repository's published kind-support cases, run through the
//! synthesis facade. The cases are a verbatim snapshot; `fixtures/README.md`
//! records provenance.
use openbindings_interface_synthesis::{
    CancellationToken, InterfaceSynthesizer, KindSupportVerdict, SupportedKind, SynthesisCandidate,
    SynthesisError, SynthesisRequest, Synthesizer,
};
use serde_json::Value;
use std::{collections::HashSet, future::Ready};

const KIND_SUPPORT: &str = include_str!("fixtures/conformance/kind-support/cases.json");

/// A notional synthesizer with an exact warranted kind set and an advisory list.
struct Corpus {
    warranted: HashSet<String>,
    listed: Vec<String>,
}
impl InterfaceSynthesizer for Corpus {
    type Synthesis<'a> = Ready<Result<SynthesisCandidate, SynthesisError>>;
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
    fn synthesize<'a>(
        &'a self,
        _: &'a SynthesisRequest<'a>,
        _: CancellationToken,
    ) -> Self::Synthesis<'a> {
        unreachable!("kind-support cases never synthesize")
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
fn published_kind_support_cases_hold_for_the_synthesis_facade() {
    for case in published_cases() {
        let description = case["description"].as_str().unwrap();
        let listed = strings(&case["listed"]);
        let provider = Corpus {
            warranted: strings(&case["warranted"]).into_iter().collect(),
            listed: listed.clone(),
        };
        let facade = Synthesizer::new(&provider);
        let expected: Vec<_> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|verdict| KindSupportVerdict {
                kind: verdict["kind"].as_str().unwrap().to_owned(),
                supported: verdict["supported"].as_bool().unwrap(),
            })
            .collect();
        assert_eq!(
            facade.check_kind_support(strings(&case["kinds"])).unwrap(),
            expected,
            "{description}"
        );
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
    let error = Synthesizer::new(&provider)
        .check_kind_support(["example.kind", ""])
        .unwrap_err();
    assert_eq!(
        error.kind(),
        openbindings_interface_synthesis::SynthesisErrorKind::InvalidInput
    );
    assert_eq!(error.code(), "openbindings.synthesis.empty-kind");
}

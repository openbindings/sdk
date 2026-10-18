//! The interfaces repository's published conformance cases for binding
//! selection and kind support, run through the operation and binding facades.
//! The cases are verbatim snapshots; `fixtures/README.md` records provenance.
use openbindings::{Document, JsonValue};
use openbindings_invocation::{
    BindingRequest, CancellationToken, InvocationError, ProviderIo,
    binding::{self, BindingInvoker},
    operation::{Attempt, Invoker, Request},
};
use serde_json::Value;
use std::{collections::HashSet, future::Future, pin::Pin, sync::Arc};

const SELECTION: [(&str, &str); 4] = [
    (
        "automatic-resolution",
        include_str!("fixtures/conformance/selection/automatic-resolution.json"),
    ),
    (
        "default-supported",
        include_str!("fixtures/conformance/selection/default-supported.json"),
    ),
    (
        "explicit-binding",
        include_str!("fixtures/conformance/selection/explicit-binding.json"),
    ),
    (
        "override-selection",
        include_str!("fixtures/conformance/selection/override-selection.json"),
    ),
];
const KIND_SUPPORT: &str = include_str!("fixtures/conformance/kind-support/cases.json");

/// A notional invoker with an exact warranted kind set and an advisory list.
/// Its invocation reports the selected binding value; it has no preflight hook.
struct Corpus {
    warranted: HashSet<String>,
    listed: Vec<String>,
}
impl BindingInvoker for Corpus {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        self.warranted.contains(kind)
    }
    fn supported_kinds(&self) -> Vec<binding::SupportedKind> {
        self.listed
            .iter()
            .map(|kind| binding::SupportedKind::new(kind.clone()))
            .collect()
    }
    fn invoke<'a>(
        &'a self,
        request: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move { io.emit(request.binding.to_json().unwrap()).await })
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

/// Name the fixture binding whose value the provider received.
fn selected_key(document: &Value, received: &JsonValue) -> String {
    let received: Value = serde_json::from_str(received.text()).unwrap();
    let keys: Vec<_> = document["bindings"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, binding)| **binding == received)
        .map(|(key, _)| key.clone())
        .collect();
    assert_eq!(keys.len(), 1, "binding values must identify their key");
    keys[0].clone()
}

async fn outcome<F: Future<Output = Result<(), InvocationError>>>(
    mut call: openbindings_invocation::Invocation<F>,
    document: &Value,
) -> Result<String, String> {
    call.close();
    let result = match call.next().await.expect("an output or a terminal failure") {
        Ok(value) => Ok(selected_key(document, &value)),
        Err(error) => Err(error.code().to_owned()),
    };
    if result.is_ok() {
        assert!(call.next().await.is_none());
    }
    result
}

#[tokio::test]
async fn published_selection_cases_resolve_through_invocation_preparation_and_preflight() {
    let mut total = 0;
    for (cluster, text) in SELECTION {
        let fixture: Value = serde_json::from_str(text).unwrap();
        assert_eq!(fixture["cluster"], cluster);
        for case in fixture["tests"].as_array().unwrap() {
            total += 1;
            let description = case["description"].as_str().unwrap();
            let document = &case["document"];
            let proof = Document::parse(document.to_string())
                .unwrap()
                .validate()
                .unwrap()
                .conformant()
                .expect("corpus documents are conformant");
            let provider = Corpus {
                warranted: strings(&case["supported"]).into_iter().collect(),
                listed: Vec::new(),
            };
            let invoker = Arc::new(Invoker::new(proof, &provider));
            let operation = case["operation"].as_str().unwrap();
            let selection = case.get("selection").map(strings);
            let keys: Vec<&str> = selection.iter().flatten().map(String::as_str).collect();
            let explicit = case.get("binding").map(|key| key.as_str().unwrap());
            let request = match (explicit, &selection) {
                (Some(key), _) => Request::binding(key),
                (None, Some(_)) => Request::ordered(operation, &keys),
                (None, None) => Request::operation(operation),
            };
            let attempt = match (explicit, &selection) {
                (Some(key), _) => Attempt::binding(key),
                (None, Some(_)) => Attempt::ordered(&keys),
                (None, None) => Attempt::new(),
            };
            let expected = &case["expected"];
            let want = match expected.get("binding") {
                Some(key) => Ok(key.as_str().unwrap().to_owned()),
                None => Err(match expected["kind"].as_str().unwrap() {
                    "ambiguous" => "ERR_BINDING_SELECTION_REQUIRED",
                    "unknown-binding" | "no-candidate" => "ERR_BINDING_NOT_FOUND",
                    kind => panic!("{description}: unknown expected kind {kind}"),
                }
                .to_owned()),
            };
            let plain = outcome(invoker.invoke(request), document).await;
            assert_eq!(plain, want, "{cluster}: {description}");
            let prepared = invoker.prepare(operation).unwrap();
            let reused = outcome(prepared.invoke(attempt), document).await;
            assert_eq!(reused, want, "{cluster} prepared: {description}");
            let preflight = invoker
                .preflight(request)
                .await
                .map(|prediction| assert!(prediction.is_none()))
                .map_err(|error| error.code().to_owned());
            assert_eq!(
                preflight,
                want.clone().map(drop),
                "{cluster} preflight: {description}"
            );
        }
    }
    assert_eq!(total, 20, "every published selection case runs");
}

#[test]
fn published_kind_support_cases_hold_for_the_binding_facade() {
    let cases: Value = serde_json::from_str(KIND_SUPPORT).unwrap();
    assert_eq!(cases["format"], "openbindings.kind-support-cases@1");
    let mut total = 0;
    for case in cases["tests"].as_array().unwrap() {
        total += 1;
        let description = case["description"].as_str().unwrap();
        let listed = strings(&case["listed"]);
        let provider = Corpus {
            warranted: strings(&case["warranted"]).into_iter().collect(),
            listed: listed.clone(),
        };
        let facade = binding::Invoker::new(&provider);
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
    assert_eq!(total, 4, "every published kind-support case runs");
}

// The three companion facades name an empty requested kind alike.
#[test]
fn empty_requested_kind_is_refused_with_the_empty_kind_code() {
    let provider = Corpus {
        warranted: HashSet::new(),
        listed: Vec::new(),
    };
    let error = binding::Invoker::new(&provider)
        .check_kind_support(["example.kind", ""])
        .unwrap_err();
    assert_eq!((error.code(), error.path()), ("empty-kind", "/kinds"));
}

//! Description and source checking for this fixture's two local kinds.
use super::{CatalogError, embedded, fixed, json, object};
use openbindings::JsonValue;
use openbindings_kind_description::{
    CancellationToken, CheckFailure, CheckRejection, CheckSourceRequest, CheckedSource,
    DescribeKindRequest, KindDescription, KindDescriptor, KindSummary, SourceChecker,
};
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
    sync::atomic::{AtomicUsize, Ordering},
};

const KINDS: [&str; 2] = ["example.fixed@1", "example.embedded@1"];
const SUPPORTS: [&str; 2] = [
    "openbindings.kind-descriptor.checkSource",
    "openbindings.source-puller.pullSource",
];
// JSON Schema uses Unicode-mode matching. These ranges accept scalar strings,
// including astral characters, but exclude unpaired UTF-16 surrogate units.
const SCALAR_STRING: &str = r"^[\u0000-\uD7FF\uE000-\u{10FFFF}]*$";

/// Local fixture capabilities; these kind identifiers are not published protocols.
#[derive(Default)]
pub struct KindCatalog {
    checks: AtomicUsize,
}
impl KindCatalog {
    /// Explicit source checks; describing never increments this count.
    pub fn checks(&self) -> usize {
        self.checks.load(Ordering::Relaxed)
    }
}
impl KindDescriptor for KindCatalog {
    type Error = CatalogError;
    type List<'a> = Ready<Result<Vec<KindSummary>, Self::Error>>;
    type Describe<'a> = Ready<Result<KindDescription, Self::Error>>;
    fn handle_list_kinds(&self, _: CancellationToken) -> Self::List<'_> {
        ready(Ok(KINDS
            .iter()
            .map(|kind| {
                KindSummary::from_json(json(
                    &serde_json::json!({"kind":kind,"supports":SUPPORTS}).to_string(),
                ))
                .unwrap()
            })
            .collect()))
    }
    fn handle_describe_kind(
        &self,
        request: DescribeKindRequest,
        _: CancellationToken,
    ) -> Self::Describe<'_> {
        let Some(kind) = request.kind().as_str().filter(|kind| KINDS.contains(kind)) else {
            return ready(Err(CatalogError::Source));
        };
        let mut value = serde_json::json!({
            "kind":kind,"description":"Local echo catalog fixture.","supports":SUPPORTS,
            "check":{"description":"Check the supplied catalog carriage without executing its operations; offered OBI entries are validated when pulled.","executes":false},
            "bindingData":{"type":"object","required":["item"],"properties":{"item":{"type":"string","pattern":SCALAR_STRING}}}
        });
        if kind == "example.embedded@1" {
            value["sourceData"] = serde_json::json!({"type":"object","required":["items"],"properties":{"items":{"type":"array","items":{"type":"object","required":["key","operation","data"],"properties":{"key":{"type":"string","pattern":SCALAR_STRING},"operation":true,"data":true}}}}});
        } else {
            value["bindingData"]["properties"]["item"] = serde_json::json!({"const":"echo"});
        }
        ready(Ok(
            KindDescription::from_json(json(&value.to_string())).unwrap()
        ))
    }
}
impl SourceChecker for KindCatalog {
    type Error = CatalogError;
    type Check<'a> = Ready<Result<CheckedSource, CheckFailure<Self::Error>>>;
    fn handle_check_source(
        &self,
        request: CheckSourceRequest,
        control: CancellationToken,
    ) -> Self::Check<'_> {
        if control.is_cancelled() {
            return ready(Err(CheckFailure::Cancelled));
        }
        self.checks.fetch_add(1, Ordering::Relaxed);
        let mut fields = BTreeMap::from([
            (
                "kind".into(),
                request.value().value().get("kind").unwrap().to_owned(),
            ),
            (
                "description".into(),
                JsonValue::string("Checked local fixture source.").unwrap(),
            ),
        ]);
        if let Some(data) = request.data() {
            fields.insert("data".into(), data);
        }
        let source = object(fields);
        let accepted = match request.kind().as_str() {
            Some("example.fixed@1") => fixed(source.view()),
            Some("example.embedded@1") => embedded(source.view()),
            _ => Err(CatalogError::Source),
        };
        ready(match accepted {
            Ok(_) => Ok(CheckedSource::from_json(source).unwrap()),
            Err(_) => Err(CheckFailure::Rejected(CheckRejection::from_json(json(
                r#"{"code":"example.invalid-source","message":"The proposed source does not have this local kind's catalog carriage."}"#
            )).unwrap())),
        })
    }
}

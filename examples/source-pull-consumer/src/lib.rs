//! External consumers and two illustrative kind implementations for current source pulling.
//! Kind definitions are local to this fixture, not OpenBindings standards.
use openbindings::{JsonRef, JsonValue};
use openbindings_schema_comparison::{ComparisonFailure, ComparisonLimits, SchemaInput, identical};
use openbindings_source_pull::*;
use std::{collections::BTreeMap, future::Future, pin::Pin};

mod kinds;
pub mod using_the_result;
pub use kinds::KindCatalog;

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}
fn object(fields: BTreeMap<String, JsonValue>) -> JsonValue {
    json(&format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(k, v)| format!("{}:{}", quote(&k), v.text()))
            .collect::<Vec<_>>()
            .join(",")
    ))
}
fn array(values: &[JsonValue]) -> JsonValue {
    json(&format!(
        "[{}]",
        values
            .iter()
            .map(|v| v.text())
            .collect::<Vec<_>>()
            .join(",")
    ))
}
#[derive(Debug)]
pub enum CatalogError {
    Source,
    Comparison(ComparisonFailure),
}
impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source => f.write_str("the source does not match the local catalog kind"),
            Self::Comparison(_) => f.write_str("catalog schema comparison failed"),
        }
    }
}
impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Comparison(error) => Some(error),
            Self::Source => None,
        }
    }
}

struct Offer {
    key: String,
    operation: JsonValue,
    data: JsonValue,
}
struct Observation {
    offers: Vec<Offer>,
    root: JsonValue,
    exhaustive: bool,
}

// example.fixed@1 has no source data. It describes a fixed echo surface.
fn fixed(source: JsonRef<'_>) -> Result<Observation, CatalogError> {
    if source.get("kind").and_then(|v| v.as_str()) != Some("example.fixed@1")
        || source.get("data").is_some()
    {
        return Err(CatalogError::Source);
    }
    let operation = json(r#"{"input":{"type":"string"},"output":{"type":"string"}}"#);
    Ok(Observation {
        offers: vec![Offer {
            key: "echo".into(),
            operation: operation.clone(),
            data: json(r#"{"item":"echo"}"#),
        }],
        root: operation,
        exhaustive: true,
    })
}
// example.embedded@1 reads only its embedded catalog. Each item owns its offered
// operation schemas and binding data. The catalog may explicitly mark a partial read.
fn embedded(source: JsonRef<'_>) -> Result<Observation, CatalogError> {
    if source.get("kind").and_then(|v| v.as_str()) != Some("example.embedded@1") {
        return Err(CatalogError::Source);
    }
    let root = source.get("data").ok_or(CatalogError::Source)?.to_owned();
    let items = root
        .get("items")
        .and_then(|v| v.elements())
        .ok_or(CatalogError::Source)?;
    let offers = items
        .map(|item| {
            Ok(Offer {
                key: item
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or(CatalogError::Source)?
                    .into(),
                operation: item
                    .get("operation")
                    .ok_or(CatalogError::Source)?
                    .to_owned(),
                data: item.get("data").ok_or(CatalogError::Source)?.to_owned(),
            })
        })
        .collect::<Result<_, _>>()?;
    let exhaustive = root.get("partial").and_then(|v| v.as_bool()) != Some(true);
    Ok(Observation {
        offers,
        root,
        exhaustive,
    })
}

fn slots(
    offer: &Offer,
    observed: &Observation,
    existing: JsonRef<'_>,
    original: JsonRef<'_>,
) -> Result<[bool; 2], CatalogError> {
    let mut answers = [true; 2];
    for (index, name) in ["input", "output"].into_iter().enumerate() {
        let left = offer
            .operation
            .get(name)
            .map(|s| SchemaInput::new(s.to_owned(), observed.root.clone()));
        let right = existing
            .get(name)
            .map(|s| SchemaInput::new(s.to_owned(), original.to_owned()));
        answers[index] = identical(left.as_ref(), right.as_ref(), &ComparisonLimits::default())
            .map_err(CatalogError::Comparison)?;
    }
    Ok(answers)
}

fn reconcile(request: &PullRequest, observed: Observation) -> Result<PullResult, CatalogError> {
    let original = request.interface().value();
    let operations = original.get("operations").unwrap();
    let bindings = original.get("bindings");
    let mut new_operations = BTreeMap::new();
    let mut new_bindings = BTreeMap::new();
    let mut added = Vec::new();
    let mut existing = Vec::new();
    let mut omitted = Vec::new();
    for offer in &observed.offers {
        // Existing paths are accounted below, rather than added a second time.
        let accounted = bindings
            .into_iter()
            .flat_map(|v| v.members().unwrap())
            .any(|b| {
                b.value.get("source").and_then(|s| s.as_str()) == Some(request.source())
                    && b.value
                        .get("data")
                        .and_then(|d| d.get("item"))
                        .and_then(|i| i.as_str())
                        == Some(&offer.key)
            });
        if accounted {
            continue;
        }
        let identifiers = std::iter::once(offer.key.as_str()).chain(
            offer
                .operation
                .get("aliases")
                .into_iter()
                .flat_map(|aliases| aliases.elements().into_iter().flatten())
                .filter_map(|alias| alias.as_str()),
        );
        let mut identified = Vec::new();
        for identifier in identifiers {
            if let Some(operation) = request.interface().resolve_operation(identifier)
                && !identified.contains(&operation.key().to_owned())
            {
                identified.push(operation.key().to_owned());
            }
        }
        if identified.len() > 1 {
            let also_identifies: Vec<_> = identified[1..]
                .iter()
                .map(|key| json(&quote(&format!("operations/{key}"))))
                .collect();
            omitted.push(json(&format!(r#"{{"status":"conflict","reasonCode":"offered-identifiers-conflict","message":"The offered identifiers identify several existing operations.","item":{},"existing":{},"alsoIdentifies":{},"offered":{}}}"#, quote(&format!("operations/{}", offer.key)), quote(&format!("operations/{}", identified[0])), array(&also_identifies).text(), offer.operation.text())));
            continue;
        }
        let operation_key = identified.first().unwrap_or(&offer.key);
        let reused = if let Some(operation) = operations.get(operation_key) {
            let compared = match slots(offer, &observed, operation, original) {
                Ok(compared) => compared,
                Err(CatalogError::Comparison(
                    ComparisonFailure::MissingReference
                    | ComparisonFailure::ReferenceCycle
                    | ComparisonFailure::InvalidReference
                    | ComparisonFailure::UnsupportedReference
                    | ComparisonFailure::UnsatisfiableMerge,
                )) => {
                    omitted.push(json(&format!(r#"{{"status":"implementation-unsupported","reasonCode":"comparison-unfinished","message":"The offered schemas could not be compared.","item":{},"existing":{}}}"#, quote(&format!("operations/{}", offer.key)), quote(&format!("operations/{operation_key}")))));
                    continue;
                }
                Err(error) => return Err(error),
            };
            if compared != [true; 2] {
                omitted.push(json(&format!(r#"{{"status":"conflict","reasonCode":"offered-schema-differs","message":"The offered input or output differs from the existing operation.","item":{},"existing":{},"offered":{}}}"#, quote(&format!("operations/{}", offer.key)), quote(&format!("operations/{operation_key}")), offer.operation.text())));
                continue;
            }
            true
        } else {
            false
        };
        let binding = format!("{}.{}", offer.key, request.source());
        new_bindings.insert(
            binding.clone(),
            json(&format!(
                r#"{{"operation":{},"source":{},"data":{}}}"#,
                quote(operation_key),
                quote(request.source()),
                offer.data.text()
            )),
        );
        if !reused {
            new_operations.insert(offer.key.clone(), offer.operation.clone());
        }
        added.push(json(&format!(
            r#"{{"binding":{},"operation":{}{}}}"#,
            quote(&binding),
            quote(operation_key),
            if reused { ",\"reused\":true" } else { "" }
        )));
    }
    for binding in bindings.into_iter().flat_map(|v| v.members().unwrap()) {
        if binding.value.get("source").and_then(|s| s.as_str()) != Some(request.source()) {
            continue;
        }
        let key = binding.name.as_str().unwrap();
        let item = binding
            .value
            .get("data")
            .and_then(|d| d.get("item"))
            .and_then(|i| i.as_str());
        let offer = observed
            .offers
            .iter()
            .find(|o| Some(o.key.as_str()) == item);
        let Some(offer) = offer else {
            existing.push(json(&format!(
                r#"{{"binding":{},"status":"unmatched"}}"#,
                quote(key)
            )));
            continue;
        };
        let operation = operations
            .get(binding.value.get("operation").unwrap().as_str().unwrap())
            .unwrap();
        let mut proposal = BTreeMap::new();
        if binding
            .value
            .get("data")
            .map(|v| v.to_owned())
            .as_ref()
            .and_then(|v| v.semantic_eq(&offer.data))
            != Some(true)
        {
            proposal.insert("data".into(), offer.data.clone());
        }
        let comparisons = match slots(offer, &observed, operation, original) {
            Ok(comparisons) => comparisons,
            Err(CatalogError::Comparison(
                ComparisonFailure::MissingReference
                | ComparisonFailure::ReferenceCycle
                | ComparisonFailure::InvalidReference
                | ComparisonFailure::UnsupportedReference
                | ComparisonFailure::UnsatisfiableMerge,
            )) => {
                existing.push(json(&format!(r#"{{"binding":{},"status":"unassessed","reasonCode":"comparison-unfinished","message":"The offered schemas could not be compared."}}"#, quote(key))));
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut remove = Vec::new();
        for (name, comparison) in ["input", "output"].into_iter().zip(comparisons) {
            if !comparison {
                if let Some(value) = offer.operation.get(name) {
                    proposal.insert(name.into(), value.to_owned());
                } else {
                    remove.push(json(&quote(name)));
                }
            }
        }
        if !remove.is_empty() {
            proposal.insert("remove".into(), array(&remove));
        }
        let status = if proposal.is_empty() {
            "\"status\":\"current\"".into()
        } else {
            format!(
                "\"status\":\"stale\",\"change\":\"dangerous\",\"proposal\":{}",
                object(proposal).text()
            )
        };
        existing.push(json(&format!(r#"{{"binding":{},{status}}}"#, quote(key))));
    }
    let additions = object(BTreeMap::from([
        ("operations".into(), object(new_operations)),
        ("bindings".into(), object(new_bindings)),
    ]));
    let limitation = if observed.exhaustive {
        ""
    } else {
        ",\"limitation\":{\"code\":\"partial-catalog\",\"message\":\"Only the supplied catalog portion was observed.\"}"
    };
    PullResult::from_json(json(&format!(r#"{{"additions":{},"report":{{"exhaustive":{}{limitation},"added":{},"existing":{},"omitted":{}}}}}"#, additions.text(), observed.exhaustive, array(&added).text(), array(&existing).text(), array(&omitted).text()))).map_err(|_| CatalogError::Source)
}

/// Illustrative source whose offered echo schema is fixed and has no source data.
#[derive(Default)]
pub struct Fixed;
/// Illustrative source which projects the exact catalog embedded in source data.
#[derive(Default)]
pub struct Embedded;
macro_rules! provider {
    ($name:ty, $project:ident) => {
        impl SourcePuller for $name {
            type Error = CatalogError;
            type Pull<'a> = Pin<
                Box<dyn Future<Output = Result<PullResult, PullFailure<Self::Error>>> + Send + 'a>,
            >;
            fn handle_pull_source(
                &self,
                request: PullRequest,
                control: CancellationToken,
            ) -> Self::Pull<'_> {
                Box::pin(async move {
                    // Real suspension exercises external future ownership without
                    // requiring an SDK async runtime or a particular host runtime.
                    let mut yielded = false;
                    std::future::poll_fn(|cx| {
                        if yielded {
                            std::task::Poll::Ready(())
                        } else {
                            yielded = true;
                            cx.waker().wake_by_ref();
                            std::task::Poll::Pending
                        }
                    })
                    .await;
                    if control.is_cancelled() {
                        return Err(PullFailure::Cancelled);
                    }
                    let source = request
                        .interface()
                        .value()
                        .get("sources")
                        .unwrap()
                        .get(request.source())
                        .unwrap();
                    $project(source)
                        .and_then(|observed| reconcile(&request, observed))
                        .map_err(PullFailure::Operational)
                })
            }
        }
    };
}
provider!(Fixed, fixed);
provider!(Embedded, embedded);

#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = pullCatalog)]
pub async fn pull_catalog(text: String) -> Result<String, wasm_bindgen::JsValue> {
    let value = JsonValue::parse(&text).map_err(|e| report(&e))?;
    let request = PullRequest::from_json(&value).map_err(|e| report(&e))?;
    let result = (Embedded)
        .pull_source(request, CancellationToken::new())
        .await
        .map_err(|e| report(&e))?;
    Ok(result.value().value().text().to_owned())
}

/// Each error layer prints only itself, so the message for JavaScript joins the chain.
#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
fn report(error: &dyn std::error::Error) -> wasm_bindgen::JsValue {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    wasm_bindgen::JsValue::from_str(&message)
}

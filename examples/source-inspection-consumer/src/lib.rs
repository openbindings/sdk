//! An independent inspector-only provider and authoring consumer.
//! The catalog is a deliberately small test kind, not an OpenBindings standard.
use openbindings::{DocumentBuilder, JsonValue, Operation, Source, WorkControl};
use openbindings_source_inspection::{
    BindableTarget, BindingFragment, InspectionError, InspectionErrorKind, InspectionLimitation,
    Inspector, SourceInspection, SourceInspector,
};
use std::{
    collections::{BTreeMap, HashSet},
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
};

/// Exact code-defined kind for a local catalog of interactions.
pub const CATALOG_KIND: &str = "urn:example:inspection-catalog:v1";

/// An inspector-only provider; no other SDK role is implemented.
pub struct CatalogInspector;

impl SourceInspector for CatalogInspector {
    type Inspection<'a> =
        Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + Send + 'a>>;

    fn supports_kind(&self, kind: &str) -> bool {
        kind == CATALOG_KIND
    }

    fn inspect<'a>(&'a self, source: &'a Source, control: &'a WorkControl) -> Self::Inspection<'a> {
        Box::pin(async move {
            // Exercise actual suspension without selecting a runtime dependency.
            let mut yielded = false;
            poll_fn(|cx| {
                if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            inspect_catalog(source, control)
        })
    }
}

fn unreadable(message: &str) -> InspectionError {
    InspectionError::new(
        InspectionErrorKind::UnreadableSource,
        "example.catalog.unreadable",
        message,
    )
}

/// Kind semantics: the entire content is an array. Each record has a unique
/// nonempty `id` and `status`: `admitted`, `excluded`, or `unavailable` (admitted
/// but not inspectable by this implementation). Optional binding content is an
/// exact JSON value. Missing framing is valid. No acquisition is performed.
pub fn inspect_catalog(
    source: &Source,
    control: &WorkControl,
) -> Result<SourceInspection, InspectionError> {
    let entries = source
        .content
        .as_ref()
        .and_then(|v| v.view().elements())
        .ok_or_else(|| unreadable("catalog content must be an array"))?;
    let mut targets = Vec::new();
    let mut omitted = 0_u64;
    let mut ids = HashSet::new();
    for entry in entries {
        if control.is_cancelled() {
            return Err(InspectionError::new(
                InspectionErrorKind::Cancelled,
                "example.catalog.cancelled",
                "catalog inspection cancelled",
            ));
        }
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| unreadable("every catalog entry requires a nonempty id"))?;
        if !ids.insert(id) {
            return Err(unreadable("catalog ids must be unique"));
        }
        match entry.get("status").and_then(|v| v.as_str()) {
            Some("excluded") => continue,
            Some("unavailable") => {
                omitted += 1;
                continue;
            }
            Some("admitted") => {}
            _ => return Err(unreadable("catalog entry has an invalid status")),
        }
        let idempotent = entry
            .get("idempotent")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| unreadable("idempotent must be boolean"))
            })
            .transpose()?;
        // Framing is intentionally optional. `input` is an exact schema value;
        // the final document assessment establishes its core conformance.
        let operation = entry.get("input").map(|input| Operation {
            input: Some(input.to_owned()),
            ..Default::default()
        });
        targets.push(BindableTarget {
            source_ref: id.into(),
            operation_key: Some(id.into()),
            operation,
            binding: BindingFragment {
                content: entry.get("binding").map(|v| v.to_owned()),
                idempotent,
                ..Default::default()
            },
        });
    }
    Ok(if omitted == 0 {
        SourceInspection::complete(targets)
    } else {
        SourceInspection::partial(
            targets,
            InspectionLimitation {
                code: "example.catalog.unavailable".into(),
                message: "admitted catalog records were unavailable to this inspector".into(),
                details: Some(BTreeMap::from([(
                    "omitted".into(),
                    JsonValue::from_serializable(&omitted).expect("small counter"),
                )])),
            },
        )
    })
}

/// Choose one inspected target, assign author-owned keys, and assess a full OBI.
pub async fn author_selected(source: Source) -> Result<JsonValue, Box<dyn std::error::Error>> {
    let inspector = Inspector::new(CatalogInspector);
    let inspection = inspector.inspect(&source).await?;
    if !inspection.exhaustive {
        return Err("selection requires an explicit decision about partial inspection".into());
    }
    let target = inspection
        .targets
        .into_iter()
        .next()
        .ok_or("no admitted target")?;
    let mut draft = DocumentBuilder::new();
    draft.operations.insert(
        "chosen.operation".into(),
        target.operation.unwrap_or_default(),
    );
    draft.sources = Some(BTreeMap::from([("chosen.source".into(), source)]));
    draft.bindings = Some(BTreeMap::from([(
        "chosen.binding".into(),
        target.binding.bind("chosen.operation", "chosen.source")?,
    )]));
    let parsed = draft.build()?;
    if parsed.assess()?.validated().is_none() {
        return Err("authored document is nonconformant".into());
    }
    Ok(draft.to_json()?)
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
mod wasm {
    use super::*;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;

    struct LocalCatalog(Rc<String>);
    impl SourceInspector for LocalCatalog {
        type Inspection<'a> =
            Pin<Box<dyn Future<Output = Result<SourceInspection, InspectionError>> + 'a>>;
        fn supports_kind(&self, kind: &str) -> bool {
            kind == CATALOG_KIND
        }
        fn inspect<'a>(
            &'a self,
            source: &'a Source,
            control: &'a WorkControl,
        ) -> Self::Inspection<'a> {
            Box::pin(async move {
                // A local Rc and JavaScript Promise deliberately cross an await.
                let local = self.0.clone();
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED))
                    .await
                    .map_err(|_| unreadable("host promise failed"))?;
                assert_eq!(local.as_str(), "local host");
                inspect_catalog(source, control)
            })
        }
    }

    /// Qualification bridge only. A full OBI carrier uses existing public core
    /// parsing, so no independent JavaScript-number or partial-source decoder is
    /// introduced. Exact values cross the host boundary as JSON text.
    #[wasm_bindgen(js_name = inspectDocumentSource)]
    pub async fn inspect_document_source(document_json: String) -> Result<String, JsValue> {
        async fn run(text: String) -> Result<String, Box<dyn std::error::Error>> {
            let draft = DocumentBuilder::from_json(&JsonValue::parse(text)?)?;
            let source = draft
                .sources
                .and_then(|mut sources| sources.remove("catalog"))
                .ok_or("missing catalog source")?;
            let inspector = Inspector::new(LocalCatalog(Rc::new("local host".into())));
            let result = inspector.inspect(&source).await?;
            drop(source);
            drop(inspector);
            Ok(result.to_json()?.text().to_owned())
        }
        run(document_json)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

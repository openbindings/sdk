//! An independent inspector-only provider and authoring consumer.
//! The catalog is a deliberately small test kind, not an OpenBindings standard.
use openbindings::{Document, JsonValue, Operation, Source};
use openbindings_source_inspection::CancellationToken;
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

    fn inspect<'a>(
        &'a self,
        source: &'a Source,
        control: CancellationToken,
    ) -> Self::Inspection<'a> {
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
            inspect_catalog(source, &control)
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
/// exact JSON value. An optional `input` is only a framing hint: this fixture
/// establishes framing for boolean declarations; other hints are left unprojected.
/// Target admission does not depend on that hint. No acquisition is performed.
pub fn inspect_catalog(
    source: &Source,
    control: &dyn openbindings::WorkControl,
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
        // Only project declarations this fixture can establish as sound. An
        // unrecognized hint does not justify an invented operation contract or
        // omission of an otherwise admitted target.
        let operation = entry
            .get("input")
            .filter(|input| input.as_bool().is_some())
            .map(|input| Operation {
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
                additional_fields: Default::default(),
            },
        )
    })
}

/// Application outcomes; partial inventories and repair evidence stay available.
#[derive(Debug)]
pub enum AuthoringOutcome {
    Ready(JsonValue),
    Partial(SourceInspection),
    Empty,
    NeedsRepair {
        draft: Box<Document>,
        validation: openbindings::DocumentValidation,
    },
}

/// Application failures retain the typed SDK error rather than replacing it with prose.
#[derive(Debug)]
pub enum AuthoringFailure {
    Inspection(InspectionError),
    Draft(openbindings::EditError),
    Version(openbindings::VersionRefusal),
}
impl std::fmt::Display for AuthoringFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inspection(error) => error.fmt(f),
            Self::Draft(error) => error.fmt(f),
            Self::Version(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for AuthoringFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Inspection(error) => error,
            Self::Draft(error) => error,
            Self::Version(error) => error,
        })
    }
}

/// Choose a first target only after complete inspection. The caller retains the
/// source, including when an incomplete inventory needs an explicit selection.
pub async fn author_selected(source: &Source) -> Result<AuthoringOutcome, AuthoringFailure> {
    let inspector = Inspector::new(CatalogInspector);
    let inspection = inspector
        .inspect(source)
        .await
        .map_err(AuthoringFailure::Inspection)?;
    if !inspection.exhaustive {
        return Ok(AuthoringOutcome::Partial(inspection));
    }
    let Some(target) = inspection.targets.into_iter().next() else {
        return Ok(AuthoringOutcome::Empty);
    };
    author_target(source, target)
}

/// Author a target explicitly selected by the application. This also supports an
/// informed selection from a partial inventory; it never upgrades completeness.
pub fn author_target(
    source: &Source,
    target: BindableTarget,
) -> Result<AuthoringOutcome, AuthoringFailure> {
    let mut draft = Document::new();
    draft
        .set_operation("chosen.operation", &target.operation.unwrap_or_default())
        .map_err(AuthoringFailure::Draft)?;
    draft
        .set_source("chosen.source", source)
        .map_err(AuthoringFailure::Draft)?;
    draft
        .set_binding(
            "chosen.binding",
            &target
                .binding
                .bind("chosen.operation", "chosen.source")
                .map_err(AuthoringFailure::Inspection)?,
        )
        .map_err(AuthoringFailure::Draft)?;
    let validation = draft.validate().map_err(AuthoringFailure::Version)?;
    if validation.conformant().is_none() {
        return Ok(AuthoringOutcome::NeedsRepair {
            draft: Box::new(draft),
            validation,
        });
    }
    Ok(AuthoringOutcome::Ready(draft.value().clone()))
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
            control: CancellationToken,
        ) -> Self::Inspection<'a> {
            Box::pin(async move {
                // A local Rc and JavaScript Promise deliberately cross an await.
                let local = self.0.clone();
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED))
                    .await
                    .map_err(|_| unreadable("host promise failed"))?;
                assert_eq!(local.as_str(), "local host");
                inspect_catalog(source, &control)
            })
        }
    }

    /// Qualification bridge only. A full OBI carrier uses existing public core
    /// parsing, so no independent JavaScript-number or partial-source decoder is
    /// introduced. Exact values cross the host boundary as JSON text.
    #[wasm_bindgen(js_name = inspectDocumentSource)]
    pub async fn inspect_document_source(document_json: String) -> Result<String, JsValue> {
        async fn run(text: String) -> Result<String, Box<dyn std::error::Error>> {
            let doc = Document::parse(text)?;
            let view = doc.source("catalog")?.ok_or("missing catalog source")?;
            let source = Source {
                kind: view.kind()?.as_str().ok_or("non-scalar kind")?.into(),
                description: view
                    .description()?
                    .map(|v| {
                        v.as_str()
                            .ok_or("non-scalar description")
                            .map(str::to_owned)
                    })
                    .transpose()?,
                content: view.content().map(|v| v.to_owned()),
                additional_fields: view
                    .value()
                    .members()
                    .unwrap()
                    .filter_map(|m| {
                        let key = m.name.as_str()?;
                        (!matches!(key, "kind" | "description" | "content"))
                            .then(|| (key.into(), m.value.to_owned()))
                    })
                    .collect(),
            };
            let provider = Rc::new(LocalCatalog(Rc::new("local host".into())));
            let inspector = Inspector::new(provider.clone());
            let result = inspector.inspect(&source).await?;
            drop(source);
            drop(inspector);
            assert_eq!(provider.0.as_str(), "local host");
            drop(provider);
            Ok(result.to_json()?.text().to_owned())
        }
        run(document_json)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

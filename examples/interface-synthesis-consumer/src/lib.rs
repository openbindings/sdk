//! Code-defined test kind, with independent source analysis shared by two roles.
use openbindings::{Binding, Dependency, Document, JsonValue, Operation, Source};
use openbindings_interface_synthesis::CancellationToken;
use openbindings_interface_synthesis::*;
use openbindings_source_inspection as inspect;
use std::{
    collections::{BTreeMap, HashSet},
    future::{Future, poll_fn},
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};
/// Fixture label. This is not an OpenBindings or OpenAPI binding specification.
pub const KIND: &str = "urn:example:capability-catalog:v1";
/// Stateful non-Clone provider. Multi-source composition is an explicit fixture capability.
#[derive(Default)]
pub struct Catalog {
    pub calls: AtomicUsize,
    pub single_source: bool,
}
fn failure(kind: SynthesisErrorKind, message: &str) -> SynthesisError {
    SynthesisError::new(kind, "example.catalog.refused", message)
}
#[derive(Clone)]
struct Unit {
    id: String,
    scope: CoverageScope,
    status: CoverageStatus,
}
fn analyze(source: &Source) -> Result<(Vec<Unit>, bool), SynthesisError> {
    let unreadable = || {
        failure(
            SynthesisErrorKind::UnreadableSource,
            "catalog requires unique unit IDs and known scopes/dispositions/interactions",
        )
    };
    let content = source.content.as_ref().ok_or_else(unreadable)?;
    let array = content
        .view()
        .get("units")
        .and_then(|v| v.elements())
        .ok_or_else(unreadable)?;
    let mut seen = HashSet::new();
    let mut units = Vec::new();
    for u in array {
        let id = u
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(unreadable)?;
        let scope = match match u.get("scope") {
            Some(v) => v.as_str().ok_or_else(unreadable)?,
            None => "target",
        } {
            "target" => CoverageScope::Target,
            "alternative" => CoverageScope::Alternative,
            "projection" => CoverageScope::Projection,
            "dependency" => CoverageScope::Dependency,
            _ => return Err(unreadable()),
        };
        if !seen.insert((scope, id)) {
            return Err(unreadable());
        }
        let status = match match u.get("status") {
            Some(v) => v.as_str().ok_or_else(unreadable)?,
            None => "represented",
        } {
            "represented" => CoverageStatus::Represented,
            "excluded" => CoverageStatus::Excluded,
            "invalid" => CoverageStatus::Invalid,
            "lossy" => CoverageStatus::Lossy,
            "implementation-unsupported" => CoverageStatus::ImplementationUnsupported,
            _ => return Err(unreadable()),
        };
        // All roles admit the same artifact, even when a unit is not emitted.
        // Keep this authoring reader independent of the execution oracle.
        let interaction = match u.get("interaction") {
            Some(value) => value.as_str().ok_or_else(unreadable)?,
            None => "echo",
        };
        if !matches!(interaction, "echo" | "unary" | "server" | "count" | "empty") {
            return Err(unreadable());
        }
        units.push(Unit {
            id: id.into(),
            scope,
            status,
        });
    }
    Ok((
        units,
        match content.view().get("unknownRemainder") {
            Some(v) => v.as_bool().ok_or_else(unreadable)?,
            None => false,
        },
    ))
}
/// Derive with transparent fixture semantics and no I/O. Sources carry their
/// fixture unit dispositions and supported interactions (echo by default).
/// Lossy framing is a sound restriction to null for input-consuming interactions.
pub fn derive(
    request: &SynthesisRequest<'_>,
    control: &dyn openbindings::WorkControl,
    single_source: bool,
) -> Result<SynthesisCandidate, SynthesisError> {
    if single_source && request.sources.len() > 1 {
        return Err(failure(
            SynthesisErrorKind::UnsupportedRequest,
            "this provider composes at most one source",
        ));
    }
    // Acquire/establish all inputs before producing output; unreadable is not partial.
    let analyzed: Vec<_> = request
        .sources
        .iter()
        .map(|s| analyze(s.source))
        .collect::<Result<_, _>>()?;
    let mut doc = Document::new();
    doc.openbindings = request
        .openbindings_version
        .unwrap_or(openbindings::AUTHORING_VERSION)
        .into();
    doc.name = request.name.map(str::to_owned);
    doc.version = request.version.map(str::to_owned);
    doc.description = request.description.map(str::to_owned);
    let mut sources = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut entries = Vec::new();
    let mut exhaustive = true;
    let mut reserved: HashSet<String> = request
        .sources
        .iter()
        .filter_map(|s| s.name.map(str::to_owned))
        .collect();
    for (index, (input, (units, unknown))) in request.sources.iter().zip(analyzed).enumerate() {
        if control.is_cancelled() {
            return Err(failure(SynthesisErrorKind::Cancelled, "cancelled"));
        }
        exhaustive &= !unknown;
        let name = if let Some(name) = input.name {
            name.to_owned()
        } else {
            let mut name = format!("source{index}");
            while !reserved.insert(name.clone()) {
                name.push('_');
            }
            name
        };
        sources.insert(name.clone(), input.source.clone());
        for (ordinal, unit) in units.into_iter().enumerate() {
            if control.is_cancelled() {
                return Err(failure(SynthesisErrorKind::Cancelled, "cancelled"));
            }
            let mut e = CoverageEntry::new(index, &unit.id, unit.scope, unit.status);
            if unit.status != CoverageStatus::Represented {
                e.reason_code = Some(format!("example.catalog.{:?}", unit.status));
                e.message = Some(
                    "Disposition explicitly declared by this fixture's unit semantics.".into(),
                );
            }
            if unit.status == CoverageStatus::Excluded {
                e.rule = Some("urn:example:capability-catalog:v1; README fixture semantics".into());
            }
            if matches!(
                unit.status,
                CoverageStatus::Represented | CoverageStatus::Lossy
            ) {
                let key = format!("s{index}.u{ordinal}");
                doc.operations.insert(
                    key.clone(),
                    Operation {
                        input: Some(
                            JsonValue::parse(if unit.status == CoverageStatus::Lossy {
                                r#"{"type":"null"}"#
                            } else {
                                "true"
                            })
                            .unwrap(),
                        ),
                        ..Default::default()
                    },
                );
                if unit.scope == CoverageScope::Dependency {
                    dependencies.insert(
                        key.clone(),
                        Dependency {
                            operation: key.clone(),
                            ..Default::default()
                        },
                    );
                    e.dependency_key = Some(key);
                } else {
                    bindings.insert(
                        key.clone(),
                        Binding {
                            operation: key.clone(),
                            source: name.clone(),
                            content: Some(match unit.scope {
                                CoverageScope::Target => JsonValue::string(&unit.id).unwrap(),
                                CoverageScope::Alternative | CoverageScope::Projection => {
                                    let scope = if unit.scope == CoverageScope::Alternative {
                                        "alternative"
                                    } else {
                                        "projection"
                                    };
                                    JsonValue::parse(format!(
                                        r#"{{"scope":"{scope}","id":{}}}"#,
                                        JsonValue::string(&unit.id).unwrap().text()
                                    ))
                                    .unwrap()
                                }
                                CoverageScope::Dependency => {
                                    unreachable!("dependency branch above")
                                }
                            }),
                            ..Default::default()
                        },
                    );
                    e.source_key = Some(name.clone());
                    e.operation_key = Some(key.clone());
                    e.binding_key = Some(key);
                }
            }
            entries.push(e);
        }
    }
    if !sources.is_empty() {
        doc.sources = Some(sources)
    }
    if !bindings.is_empty() {
        doc.bindings = Some(bindings)
    }
    if !dependencies.is_empty() {
        doc.dependencies = Some(dependencies)
    }
    Ok(SynthesisCandidate {
        document: doc.build()?,
        coverage: CoverageDraft {
            entries,
            exhaustive,
            limitation: (!exhaustive).then(|| CoverageLimitation {
                code: "example.catalog.unknown".into(),
                message: "Fixture explicitly declares an unknown remainder.".into(),
                details: None,
                additional_fields: BTreeMap::new(),
            }),
            ..Default::default()
        },
    })
}
impl InterfaceSynthesizer for Catalog {
    type Synthesis<'a> =
        Pin<Box<dyn Future<Output = Result<SynthesisCandidate, SynthesisError>> + Send + 'a>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == KIND
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        vec![SupportedKind {
            kind: KIND.into(),
            description: None,
        }]
    }
    fn synthesize<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        c: CancellationToken,
    ) -> Self::Synthesis<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::Relaxed);
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
            derive(r, &c, self.single_source)
        })
    }
}
impl inspect::SourceInspector for Catalog {
    type Inspection<'a> =
        std::future::Ready<Result<inspect::SourceInspection, inspect::InspectionError>>;
    fn supports_kind(&self, k: &str) -> bool {
        k == KIND
    }
    fn inspect<'a>(&'a self, s: &'a Source, c: CancellationToken) -> Self::Inspection<'a> {
        std::future::ready((|| {
            if c.is_cancelled() {
                return Err(inspect::InspectionError::new(
                    inspect::InspectionErrorKind::Cancelled,
                    "example.cancelled",
                    "cancelled",
                ));
            }
            let (units, unknown) = analyze(s).map_err(|e| {
                inspect::InspectionError::new(
                    inspect::InspectionErrorKind::UnreadableSource,
                    e.code(),
                    e.message(),
                )
            })?;
            // A synthesis projection gap does not hide an inspectable admitted target.
            let targets = units
                .into_iter()
                .filter(|u| {
                    u.scope == CoverageScope::Target
                        && !matches!(u.status, CoverageStatus::Excluded | CoverageStatus::Invalid)
                })
                .map(|u| inspect::BindableTarget {
                    source_ref: u.id.clone(),
                    binding: inspect::BindingFragment {
                        content: Some(JsonValue::string(&u.id).unwrap()),
                        ..Default::default()
                    },
                    operation_key: None,
                    operation: None,
                })
                .collect();
            Ok(if unknown {
                inspect::SourceInspection::partial(
                    targets,
                    inspect::InspectionLimitation {
                        code: "example.catalog.unknown".into(),
                        message: "Unknown remainder".into(),
                        details: None,
                        additional_fields: Default::default(),
                    },
                )
            } else {
                inspect::SourceInspection::complete(targets)
            })
        })())
    }
}
/// A synthesis-only implementation used to prove capability independence.
pub struct Skeleton;
impl InterfaceSynthesizer for Skeleton {
    type Synthesis<'a> = std::future::Ready<Result<SynthesisCandidate, SynthesisError>>;
    fn supports_kind(&self, _: &str) -> bool {
        false
    }
    fn synthesize<'a>(
        &'a self,
        r: &'a SynthesisRequest<'a>,
        c: CancellationToken,
    ) -> Self::Synthesis<'a> {
        std::future::ready(derive(r, &c, true))
    }
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
mod wasm {
    use super::*;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    struct Local(Rc<String>);
    impl InterfaceSynthesizer for Local {
        type Synthesis<'a> =
            Pin<Box<dyn Future<Output = Result<SynthesisCandidate, SynthesisError>> + 'a>>;
        fn supports_kind(&self, k: &str) -> bool {
            k == KIND
        }
        fn synthesize<'a>(
            &'a self,
            r: &'a SynthesisRequest<'a>,
            c: CancellationToken,
        ) -> Self::Synthesis<'a> {
            Box::pin(async move {
                let local = self.0.clone();
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED))
                    .await
                    .map_err(|_| {
                        failure(SynthesisErrorKind::ProviderFailure, "host promise failed")
                    })?;
                assert_eq!(&*local, "local");
                derive(r, &c, false)
            })
        }
    }
    /// Qualification bridge only; exact content remains JSON text throughout.
    #[wasm_bindgen(js_name=synthesizeDocumentSource)]
    pub async fn synthesize_document_source(text: String) -> Result<String, JsValue> {
        async fn run(text: String) -> Result<String, Box<dyn std::error::Error>> {
            let draft = Document::from_json(&JsonValue::parse(text)?)?;
            let source = draft
                .sources
                .and_then(|mut s| s.remove("catalog"))
                .ok_or("missing catalog")?;
            let provider = Rc::new(Local(Rc::new("local".into())));
            let facade = Synthesizer::new(provider.clone());
            let inputs = [SynthesisSource {
                name: Some("catalog"),
                source: &source,
            }];
            let request = SynthesisRequest {
                sources: &inputs,
                ..Default::default()
            };
            let result = facade.synthesize_with_coverage(&request).await?;
            drop(source);
            drop(facade);
            drop(provider);
            Ok(result.to_json()?.text().into())
        }
        run(text)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

//! OpenAPI kind adaptation outside the normative document model.
//!
//! This provisional package implements source inspection for the unpublished
//! OpenAPI 3.1 candidate only. It performs no I/O, invocation or synthesis.
//! Use the SDK's checked [`Inspector`](openbindings_source_inspection::Inspector).
//!
//! ```
//! use openbindings::{JsonValue, Source};
//! use openbindings_openapi::{OpenApi, OPENAPI_31};
//! use openbindings_source_inspection::Inspector;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let source = Source {
//!     kind: OPENAPI_31.into(),
//!     content: Some(JsonValue::parse(r#"{"document":{"openapi":"3.1.2","paths":{}}}"#)?),
//!     ..Default::default()
//! };
//! let inspector = Inspector::new(OpenApi::default());
//! let inventory = inspector.inspect(&source).await?;
//! assert!(inventory.exhaustive);
//! # Ok(()) }
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use dynamic_openapi_client::{Code, Document, DocumentBuilder, ReferenceKind, ValueKind};
use openbindings::{JsonKind, JsonValue, Source, WorkControl};
use openbindings_source_inspection::{
    BindableTarget, BindingFragment, InspectionError, InspectionErrorKind, InspectionLimitation,
    SourceInspection, SourceInspector, SupportedKind,
};
use std::{
    collections::BTreeMap,
    future::{Ready, ready},
    sync::Mutex,
};

/// Client-owned resource inputs and limits; no second resource vocabulary is imposed.
pub use dynamic_openapi_client::{GraphLimits, Limits, ResourceInput, ResourceLayout};

/// Unpublished OpenAPI 3.1 candidate identifier, compared exactly.
pub const OPENAPI_31: &str = "openbindings.openapi-3.1@1";

/// Immutable provider configuration. A new provider establishes a new snapshot.
///
/// Resources are caller-supplied bytes, not permission to fetch anything. Limits
/// apply independently to input, the client graph, and adapter-owned result text.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Options {
    /// Per-resource client parsing and analysis budgets.
    pub limits: Limits,
    /// Aggregate client resource graph budgets.
    pub graph_limits: GraphLimits,
    /// Original resources, with real retrieval identities and any explicit layouts.
    pub resources: Vec<ResourceInput>,
    /// Maximum encoded source carrier bytes retained for repeat-call identity.
    pub source_bytes: usize,
    /// Maximum logical target/diagnostic text bytes, with conservative fixed
    /// overhead for limitation evidence. This is not a heap-allocation counter.
    /// Container allocations are additionally bounded by the client's operation/node limits.
    pub result_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            graph_limits: GraphLimits::default(),
            resources: vec![],
            source_bytes: 16 << 20,
            result_bytes: 8 << 20,
        }
    }
}

/// A reusable inspection provider with one bounded private prepared entry.
///
/// Warm calls compare exact carrier bytes and clone owned target metadata; they
/// do not reparse or serialize the document. Options/resources never mutate.
/// A different carrier replaces the entry. Drop the provider to release it.
/// Concurrent native callers share this entry through a mutex; work is synchronous
/// and bounded, with cancellation checks between stages/targets. It cannot preempt
/// a parser call or a thread waiting for this mutex. No executor is required.
pub struct OpenApi {
    options: Options,
    cached: Mutex<Option<Prepared>>,
    #[cfg(test)]
    preparations: std::sync::atomic::AtomicUsize,
}
struct Prepared {
    carrier: String,
    // Retain reusable client preparation privately for future independent roles.
    _document: Document,
    inspection: SourceInspection,
}
impl Default for OpenApi {
    fn default() -> Self {
        Self {
            options: Options::default(),
            cached: Mutex::new(None),
            #[cfg(test)]
            preparations: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}
impl OpenApi {
    /// Admit a fixed supplied-resource collection under explicit limits.
    /// Resource admission errors are configuration refusals, before any inspection.
    /// Resources are validated once here, then included in each new preparation.
    pub fn new(options: Options) -> Result<Self, InspectionError> {
        if options.resources.len() > options.graph_limits.resources {
            return Err(limit());
        }
        let mut builder =
            DocumentBuilder::new(options.limits.clone(), options.graph_limits.clone());
        for resource in &options.resources {
            builder
                .add(resource.clone())
                .map_err(|e| client_error(e.diagnostic()))?;
        }
        Ok(Self {
            options,
            ..Self::default()
        })
    }

    fn run(
        &self,
        source: &Source,
        control: &WorkControl,
    ) -> Result<SourceInspection, InspectionError> {
        check_cancelled(control)?;
        if !self.supports_kind(&source.kind) {
            return Err(error(
                InspectionErrorKind::UnsupportedKind,
                "unsupported-kind",
                "unsupported inspection kind",
            ));
        }
        let content = source
            .content
            .as_ref()
            .ok_or_else(|| invalid("source content is absent"))?;
        if content.text().len() > self.options.source_bytes {
            return Err(limit());
        }
        let mut cached = self.cached.lock().map_err(|_| {
            error(
                InspectionErrorKind::ProviderFailure,
                "state",
                "provider state was poisoned",
            )
        })?;
        check_cancelled(control)?;
        if let Some(prepared) = cached.as_ref().filter(|p| p.carrier == content.text()) {
            return Ok(prepared.inspection.clone());
        }
        // Release the old snapshot before admitting another full source.
        *cached = None;
        let carrier = Carrier::read(content)?;
        let bytes = carrier.document.ok_or_else(|| {
            error(
                InspectionErrorKind::UnreadableSource,
                "source-unavailable",
                "location-only source has no supplied entry document; supply content.document",
            )
        })?;
        let input = match carrier.location {
            Some(uri) => {
                ResourceInput::at(uri, bytes.as_bytes()).map_err(|_| unsupported_location())?
            }
            None => ResourceInput::anonymous(bytes.as_bytes()),
        };
        let mut builder = DocumentBuilder::new(
            self.options.limits.clone(),
            self.options.graph_limits.clone(),
        );
        let entry = builder
            .add(input)
            .map_err(|e| client_error(e.diagnostic()))?;
        for resource in &self.options.resources {
            check_cancelled(control)?;
            builder
                .add(resource.clone())
                .map_err(|e| client_error(e.diagnostic()))?;
        }
        let document = builder.build(entry).map_err(|e| client_error(&e))?;
        check_cancelled(control)?;
        if document.root().kind() != ValueKind::Object {
            return Err(invalid("entry document must be an object"));
        }
        if !matches!(
            document
                .root()
                .get("openapi")
                .as_ref()
                .and_then(|v| v.as_str()),
            Some("3.1.0" | "3.1.1" | "3.1.2")
        ) {
            return Err(error(
                InspectionErrorKind::InvalidInput,
                "unsupported-version",
                "this candidate accepts OpenAPI 3.1.0, 3.1.1 and 3.1.2 only",
            ));
        }
        let inspection = inspect_document(&document, control, self.options.result_bytes)?;
        check_cancelled(control)?;
        #[cfg(test)]
        self.preparations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *cached = Some(Prepared {
            carrier: content.text().into(),
            _document: document,
            inspection: inspection.clone(),
        });
        Ok(inspection)
    }
}
impl SourceInspector for OpenApi {
    type Inspection<'a> = Ready<Result<SourceInspection, InspectionError>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == OPENAPI_31
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        vec![SupportedKind {
            kind: OPENAPI_31.into(),
            description: Some(
                "Provisional OpenAPI 3.1 source inspection; no acquisition, framing or invocation"
                    .into(),
            ),
        }]
    }
    fn inspect<'a>(&'a self, source: &'a Source, control: WorkControl) -> Self::Inspection<'a> {
        ready(self.run(source, &control))
    }
}

struct Carrier<'a> {
    document: Option<&'a str>,
    location: Option<&'a str>,
}
impl<'a> Carrier<'a> {
    fn read(content: &'a JsonValue) -> Result<Self, InspectionError> {
        let fields = content
            .view()
            .members()
            .ok_or_else(|| invalid("source content must be an object"))?;
        let (mut document, mut location) = (None, None);
        for field in fields {
            match field.name.as_str() {
                Some("document") if document.is_none() => {
                    document = Some(match field.value.kind() {
                        JsonKind::Object => field.value.text(),
                        JsonKind::String => field.value.as_str().ok_or_else(|| {
                            invalid("document text must contain Unicode scalar characters")
                        })?,
                        _ => return Err(invalid("document must be an object or JSON/YAML text")),
                    });
                }
                Some("location") if location.is_none() => {
                    let uri = field
                        .value
                        .as_str()
                        .ok_or_else(|| invalid("location must be a URI string"))?;
                    let uri = uri.strip_suffix('#').unwrap_or(uri);
                    let parsed = fluent_uri::UriRef::parse(uri).map_err(|_| {
                        invalid("location must be an absolute URI with no nonempty fragment")
                    })?;
                    if parsed.scheme().is_none() || parsed.fragment().is_some() {
                        return Err(invalid(
                            "location must be an absolute URI with no nonempty fragment",
                        ));
                    }
                    // The client additionally declines userinfo. That policy is
                    // an implementation limitation, not malformed kind content.
                    ResourceInput::at(uri, &[][..]).map_err(|_| unsupported_location())?;
                    location = Some(uri);
                }
                _ => {
                    return Err(invalid(
                        "source content contains an unknown or duplicate member",
                    ));
                }
            }
        }
        if document.is_none() && location.is_none() {
            return Err(invalid("source content requires document or location"));
        }
        Ok(Self { document, location })
    }
}

fn pointer_token(text: &str) -> String {
    text.replace('~', "~0").replace('/', "~1")
}
fn inspect_document(
    document: &Document,
    control: &WorkControl,
    mut remaining: usize,
) -> Result<SourceInspection, InspectionError> {
    let mut targets = Vec::new();
    let mut issues = Vec::new();
    let root = document.root();
    if let Some(authored_paths) = root.get("paths") {
        if let Some(members) = authored_paths.members() {
            for (path, item) in members.filter(|(p, _)| p.starts_with('/')) {
                check_cancelled(control)?;
                let pointer = format!("/paths/{}", pointer_token(path));
                // Non-object declarations cannot supply an Operation Object.
                if item.kind() != ValueKind::Object {
                    continue;
                }
                let resolved = document
                    .protocol_at(&pointer, ReferenceKind::PathItem)
                    .and_then(|site| document.resolve_protocol(&site));
                match resolved {
                    Ok(resolved) => {
                        // Only standard 3.1 fields in the resolved Path Item chain
                        // define targets. The client's raw inventory also exposes
                        // later-edition fields such as additionalOperations.
                        for item in resolved.chain {
                            for (method, operation) in
                                item.members().expect("resolved Path Item object")
                            {
                                if !matches!(
                                    method,
                                    "get"
                                        | "put"
                                        | "post"
                                        | "delete"
                                        | "options"
                                        | "head"
                                        | "patch"
                                        | "trace"
                                ) || operation.kind() != ValueKind::Object
                                {
                                    continue;
                                }
                                check_cancelled(control)?;
                                let target = format!("{pointer}/{method}");
                                let encoded = serde_json_compat::to_string(
                                    &serde_json_compat::json!({"target":target}),
                                )
                                .expect("string JSON");
                                let operation_key = operation.get("operationId").and_then(|v| {
                                    v.as_str().filter(|id| !id.is_empty()).map(str::to_owned)
                                });
                                charge(
                                    &mut remaining,
                                    target.len().saturating_add(encoded.len()).saturating_add(
                                        operation_key.as_ref().map_or(0, String::len),
                                    ),
                                )?;
                                targets.push(BindableTarget {
                                    source_ref: target,
                                    binding: BindingFragment {
                                        content: Some(
                                            JsonValue::parse(encoded).map_err(|_| limit())?,
                                        ),
                                        ..Default::default()
                                    },
                                    operation_key,
                                    operation: None,
                                });
                            }
                        }
                    }
                    Err(diagnostic) => {
                        let need = diagnostic
                            .resource_need()
                            .and_then(|need| need.requested_uri())
                            .map(str::to_owned);
                        let issue = serde_json_compat::json!({"pointer":pointer,"clientCode":format!("{:?}",diagnostic.code()),"resource":need});
                        charge(&mut remaining, issue.to_string().len())?;
                        issues.push(issue);
                    }
                }
            }
        } else {
            let issue =
                serde_json_compat::json!({"pointer":"/paths","clientCode":"InvalidPathsShape"});
            charge(&mut remaining, issue.to_string().len())?;
            issues.push(issue);
        }
    }
    Ok(if issues.is_empty() {
        SourceInspection::complete(targets)
    } else {
        // Include fixed limitation text and JSON/container spelling allowance.
        charge(&mut remaining, 512usize.saturating_add(issues.len()))?;
        let evidence =
            JsonValue::parse(serde_json_compat::to_vec(&issues).expect("diagnostic JSON"))
                .map_err(|_| limit())?;
        SourceInspection::partial(targets, InspectionLimitation {
            code: "openbindings.openapi.analysis-incomplete".into(),
            message: "Some paths could not be established. Supply missing resources/layouts when identified; other client diagnostics may require adapter/client support. Unreported paths are not declared invalid.".into(),
            details: Some(BTreeMap::from([("issues".into(), evidence)])),
        })
    })
}
fn charge(remaining: &mut usize, bytes: usize) -> Result<(), InspectionError> {
    *remaining = remaining.checked_sub(bytes).ok_or_else(limit)?;
    Ok(())
}
fn error(kind: InspectionErrorKind, code: &str, message: &str) -> InspectionError {
    InspectionError::new(kind, format!("openbindings.openapi.{code}"), message)
}
fn invalid(message: &str) -> InspectionError {
    error(InspectionErrorKind::InvalidInput, "invalid-source", message)
}
fn unsupported_location() -> InspectionError {
    error(
        InspectionErrorKind::UnreadableSource,
        "unsupported-location",
        "the client cannot admit this retrieval URI; URI userinfo is not supported",
    )
}
fn limit() -> InspectionError {
    error(
        InspectionErrorKind::ProviderFailure,
        "limit",
        "configured source, graph or result budget was exhausted",
    )
}
fn check_cancelled(control: &WorkControl) -> Result<(), InspectionError> {
    if control.is_cancelled() {
        Err(error(
            InspectionErrorKind::Cancelled,
            "cancelled",
            "inspection was cancelled",
        ))
    } else {
        Ok(())
    }
}
fn client_error(diagnostic: &dynamic_openapi_client::Diagnostic) -> InspectionError {
    if diagnostic.code() == Code::Limit {
        return limit();
    }
    if matches!(
        diagnostic.code(),
        Code::InvalidJson | Code::InvalidYaml | Code::InvalidUtf8 | Code::DuplicateMember
    ) {
        return error(
            InspectionErrorKind::InvalidInput,
            "invalid-document",
            &format!("invalid document representation: {:?}", diagnostic.code()),
        );
    }
    error(
        InspectionErrorKind::UnreadableSource,
        "client-admission",
        &format!(
            "OpenAPI resource admission refused: {:?}",
            diagnostic.code()
        ),
    )
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use openbindings_source_inspection::Inspector;
    use std::sync::atomic::Ordering;
    #[tokio::test]
    async fn repeat_calls_do_not_parse_or_bridge_the_source_again() {
        let provider = OpenApi::default();
        let source = Source {
            kind: OPENAPI_31.into(),
            content: Some(
                JsonValue::parse(
                    r#"{"document":{"openapi":"3.1.2","paths":{"/first":{"get":{}}}}}"#,
                )
                .unwrap(),
            ),
            ..Default::default()
        };
        let inspector = Inspector::new(&provider);
        for _ in 0..100 {
            assert_eq!(inspector.inspect(&source).await.unwrap().targets.len(), 1);
        }
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 1);
        // Equal bytes in a different arena need no new preparation.
        let mut changed = source.clone();
        changed.content = Some(JsonValue::parse(source.content.as_ref().unwrap().text()).unwrap());
        inspector.inspect(&changed).await.unwrap();
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 1);
        changed.content = Some(
            JsonValue::parse(r#"{"document":{"openapi":"3.1.2","paths":{"/other":{"post":{}}}}}"#)
                .unwrap(),
        );
        assert_eq!(
            inspector.inspect(&changed).await.unwrap().targets[0].source_ref,
            "/paths/~1other/post"
        );
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 2);
        changed.content=Some(JsonValue::parse(r#"{"document":{"openapi":"3.1.2","paths":{"/other":{"post":{}}}},"location":"https://fixture.invalid/api"}"#).unwrap());
        inspector.inspect(&changed).await.unwrap();
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 3);
    }
}

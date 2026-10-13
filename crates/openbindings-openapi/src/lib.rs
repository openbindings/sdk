//! Experimental OpenAPI inspection, synthesis and finite JSON invocation.
//!
//! This optional adapter implements `local.openapi-json-probe.r1`. Its meaning
//! is documented in the package README; it is not a published binding standard.
//! Core remains unaware of OpenAPI, transport and operation effects.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod analysis;
mod invocation;
mod synthesis;

use dynamic_openapi_client::DocumentBuilder;
pub use dynamic_openapi_client::{GraphLimits, Limits, ResourceInput, ResourceLayout};
pub use invocation::{InvocationFuture, Transport};
use openbindings::{JsonValue, Source, WorkControl};
use openbindings_source_inspection::{
    InspectionError, InspectionErrorKind, SourceInspection, SourceInspector, SupportedKind,
};
use std::{
    future::{Ready, ready},
    sync::{Arc, Mutex},
};

/// Exact private experiment identifier. Saved r1 documents retain this meaning.
pub const EXPERIMENTAL_KIND: &str = "local.openapi-json-probe.r1";
/// Configuration point containing a declared absolute server URL selector.
pub const SERVER_POINT: &str = "local.openapi-json-probe.r1.server";
/// Kind-defined unsuccessful HTTP status, with application data `{ "status": n }`.
pub const HTTP_STATUS_ERROR: &str = "local.openapi-json-probe.r1.http-status";

/// Fixed resources and finite admission budgets. Changing these requires a new provider.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Options {
    /// Client per-resource, operation and HTTP budgets.
    pub limits: Limits,
    /// Aggregate resource graph budgets.
    pub graph_limits: GraphLimits,
    /// Explicit resource bytes and identities; no automatic acquisition occurs.
    pub resources: Vec<ResourceInput>,
    /// Maximum encoded source content, including its document text.
    pub source_bytes: usize,
    /// Maximum charged adapter inventory bytes, including retained server/parameter metadata.
    /// This is a logical work budget, not an allocator or process RSS measurement.
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

/// Configuration/source admission category. Messages never include source values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Invalid carrier or malformed supplied data.
    InvalidInput,
    /// A different kind or an unimplemented source/profile.
    Unsupported,
    /// The available resources cannot establish the source.
    Unreadable,
    /// A configured budget was exhausted.
    Limit,
    /// Cooperative cancellation.
    Cancelled,
}
/// Safe adapter admission error; role facades map it into their own error vocabulary.
#[derive(Clone, Debug)]
pub struct Error {
    kind: ErrorKind,
    code: &'static str,
}
impl Error {
    /// Typed admission category.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    /// Stable implementation diagnostic, not a binding-spec requirement.
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub(crate) fn new(kind: ErrorKind, code: &'static str) -> Self {
        Self { kind, code }
    }
    pub(crate) fn inspection(&self) -> InspectionError {
        let kind = match self.kind {
            ErrorKind::InvalidInput => InspectionErrorKind::InvalidInput,
            ErrorKind::Unsupported if self.code == "unsupported-kind" => {
                InspectionErrorKind::UnsupportedKind
            }
            ErrorKind::Unsupported => InspectionErrorKind::UnreadableSource,
            ErrorKind::Unreadable => InspectionErrorKind::UnreadableSource,
            ErrorKind::Limit => InspectionErrorKind::ProviderFailure,
            ErrorKind::Cancelled => InspectionErrorKind::Cancelled,
        };
        InspectionError::new(
            kind,
            format!("openbindings.openapi.{}", self.code),
            self.to_string(),
        )
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenAPI adapter admission: {}", self.code)
    }
}
impl std::error::Error for Error {}

/// Reusable adapter. Inspection and synthesis need no transport; bind one once
/// with [`Self::with_transport`] to enable the existing BindingInvoker role.
///
/// One immutable prepared source is cached. No mutex is held during host I/O.
/// Supplied resources must be reconstructed for an independent cold reload.
pub struct OpenApi<T = ()> {
    options: Options,
    transport: T,
    cached: Mutex<Option<Arc<analysis::Prepared>>>,
    #[cfg(test)]
    preparations: std::sync::atomic::AtomicUsize,
}
impl Default for OpenApi<()> {
    fn default() -> Self {
        Self {
            options: Options::default(),
            transport: (),
            cached: Mutex::new(None),
            #[cfg(test)]
            preparations: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}
impl OpenApi<()> {
    /// Check a fixed supplied-resource collection without performing I/O.
    pub fn new(options: Options) -> Result<Self, Error> {
        let mut builder =
            DocumentBuilder::new(options.limits.clone(), options.graph_limits.clone());
        for resource in &options.resources {
            builder
                .add(resource.clone())
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "resource-admission"))?;
        }
        Ok(Self {
            options,
            ..Self::default()
        })
    }
}
impl<T> OpenApi<T> {
    /// Bind application-owned HTTP execution. Native and local non-Send hosts are supported.
    pub fn with_transport<H: Transport>(self, transport: H) -> OpenApi<H> {
        OpenApi {
            options: self.options,
            transport,
            cached: self.cached,
            #[cfg(test)]
            preparations: self.preparations,
        }
    }
    fn prepare(
        &self,
        source: &Source,
        control: &WorkControl,
    ) -> Result<Arc<analysis::Prepared>, Error> {
        check(control)?;
        if source.kind != EXPERIMENTAL_KIND {
            return Err(Error::new(ErrorKind::Unsupported, "unsupported-kind"));
        }
        let content = source
            .content
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing-source-content"))?;
        if content.bytes().len() > self.options.source_bytes {
            return Err(Error::new(ErrorKind::Limit, "source-limit"));
        }
        let mut cache = self
            .cached
            .lock()
            .map_err(|_| Error::new(ErrorKind::Unreadable, "poisoned-cache"))?;
        check(control)?;
        if let Some(prepared) = cache.as_ref().filter(|p| {
            std::ptr::eq(p.content.text(), content.text()) || p.content.text() == content.text()
        }) {
            return Ok(prepared.clone());
        }
        *cache = None;
        let prepared = Arc::new(analysis::prepare(content, &self.options, control)?);
        #[cfg(test)]
        self.preparations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *cache = Some(prepared.clone());
        Ok(prepared)
    }
}
impl<T> SourceInspector for OpenApi<T> {
    type Inspection<'a>
        = Ready<Result<SourceInspection, InspectionError>>
    where
        Self: 'a;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == EXPERIMENTAL_KIND
    }
    fn supported_kinds(&self) -> Vec<SupportedKind> {
        vec![SupportedKind {
            kind: EXPERIMENTAL_KIND.into(),
            description: Some(
                "Private finite JSON experiment; consult its qualified capability profile".into(),
            ),
        }]
    }
    fn inspect<'a>(&'a self, source: &'a Source, control: WorkControl) -> Self::Inspection<'a> {
        ready(
            self.prepare(source, &control)
                .map(|p| p.inspection())
                .map_err(|e| e.inspection()),
        )
    }
}
fn check(control: &WorkControl) -> Result<(), Error> {
    if control.is_cancelled() {
        Err(Error::new(ErrorKind::Cancelled, "cancelled"))
    } else {
        Ok(())
    }
}
fn json(value: &serde_json_compat::Value) -> Result<JsonValue, Error> {
    JsonValue::parse(value.to_string()).map_err(|_| Error::new(ErrorKind::Limit, "result-limit"))
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::sync::atomic::Ordering;
    fn source(uri: &str) -> Source {
        Source {kind:EXPERIMENTAL_KIND.into(),content:Some(json(&serde_json_compat::json!({"document":include_str!("../tests/fixtures/r1/parcels.json"),"retrievalUri":uri})).unwrap()),..Default::default()}
    }
    #[test]
    fn one_source_cache_checks_content_base_and_fixed_options() {
        let provider = OpenApi::default();
        let control = WorkControl::new();
        let a = source("https://a.invalid/doc.json");
        let first = provider.prepare(&a, &control).unwrap();
        let same = provider.prepare(&a.clone(), &control).unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        let equivalent = source("https://a.invalid/doc.json");
        assert!(Arc::ptr_eq(
            &first,
            &provider.prepare(&equivalent, &control).unwrap()
        ));
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 1);
        let b = source("https://b.invalid/doc.json");
        assert!(!Arc::ptr_eq(
            &first,
            &provider.prepare(&b, &control).unwrap()
        ));
        assert_eq!(provider.preparations.load(Ordering::Relaxed), 2);
        drop(same);
        drop(first);
        let weak = Arc::downgrade(provider.cached.lock().unwrap().as_ref().unwrap());
        provider.prepare(&a, &control).unwrap();
        assert!(weak.upgrade().is_none());
        let options = Options {
            source_bytes: 0,
            ..Default::default()
        };
        assert!(
            OpenApi::new(options)
                .unwrap()
                .prepare(&a, &control)
                .is_err()
        );
    }
}

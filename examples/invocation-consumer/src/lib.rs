//! An external provider and caller using only the current invocation contracts.
use openbindings::{JsonValue, Source};
use openbindings_invocation::{
    BindingInvoker, BindingRequest, CancellationToken, ContextChallenge, InvocationError,
    InvocationSupport, PreflightOutcome, ProviderIo,
};
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Illustrative echo realizations for the source-pull consumer's two local kinds.
#[derive(Default)]
pub struct Catalog {
    require_context: bool,
    values: AtomicUsize,
    predictions: AtomicUsize,
}
impl Catalog {
    /// Require separately supplied context before consuming any operation input.
    pub fn requiring_context() -> Self {
        Self {
            require_context: true,
            ..Self::default()
        }
    }
    /// Number of values actually consumed by this provider's operation work.
    pub fn values(&self) -> usize {
        self.values.load(Ordering::Relaxed)
    }
    /// Number of explicitly requested advisory predictions.
    pub fn predictions(&self) -> usize {
        self.predictions.load(Ordering::Relaxed)
    }
    fn challenge(&self, request: &BindingRequest) -> Option<ContextChallenge> {
        let supplied = request
            .context
            .as_ref()
            .and_then(|c| c.value().value().get("permit"))
            .and_then(|v| v.as_str());
        (self.require_context && supplied != Some("approved")).then(|| ContextChallenge::try_from(JsonValue::parse(r#"{"message":"Supply the caller-approved permit.","schema":{"type":"object","required":["permit"],"properties":{"permit":{"const":"approved"}}}}"#).unwrap()).unwrap())
    }
}
fn error(code: &str) -> InvocationError {
    InvocationError::new(code, None).unwrap()
}
fn selected(request: &BindingRequest) -> Result<Source, InvocationError> {
    let binding = request
        .interface
        .binding(&request.binding)
        .ok_or_else(|| error("example.missing-binding"))?;
    let source = binding.source();
    let data = binding
        .data()
        .ok_or_else(|| error("example.missing-selector"))?;
    let item = data
        .get("item")
        .and_then(|v| v.as_str())
        .ok_or_else(|| error("example.invalid-selector"))?;
    match source.kind() {
        "example.fixed@1" if item == "echo" && source.data().is_none() => {}
        "example.embedded@1" => {
            let catalog = source
                .data()
                .ok_or_else(|| error("example.invalid-source"))?;
            let items = catalog
                .get("items")
                .and_then(|v| v.elements())
                .ok_or_else(|| error("example.invalid-source"))?;
            if !items
                .into_iter()
                .any(|entry| entry.get("key").and_then(|v| v.as_str()) == Some(item))
            {
                return Err(error("example.missing-item"));
            }
        }
        _ => return Err(error("example.unsupported-source")),
    }
    Ok(source)
}
impl InvocationSupport for Catalog {
    fn can_invoke(&self, kind: &str) -> bool {
        matches!(kind, "example.fixed@1" | "example.embedded@1")
    }
}
impl BindingInvoker for Catalog {
    type Invoke<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn handle_invoke<'a>(
        &'a self,
        request: BindingRequest,
        mut io: ProviderIo,
        control: CancellationToken,
    ) -> Self::Invoke<'a> {
        Box::pin(async move {
            let _source = selected(&request)?;
            if let Some(challenge) = self.challenge(&request) {
                return Err(InvocationError::challenge(challenge));
            }
            while let Some(value) = io.next().await {
                if control.is_cancelled() {
                    return Err(InvocationError::cancelled());
                }
                self.values.fetch_add(1, Ordering::Relaxed);
                io.send(value).await?;
            }
            Ok(())
        })
    }
    fn handle_preflight<'a>(
        &'a self,
        request: BindingRequest,
        _: CancellationToken,
    ) -> Self::Preflight<'a> {
        self.predictions.fetch_add(1, Ordering::Relaxed);
        ready(selected(&request).map(|_| {
            self.challenge(&request)
                .map(PreflightOutcome::Challenge)
                .unwrap_or(PreflightOutcome::NoPrediction)
        }))
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
mod wasm;

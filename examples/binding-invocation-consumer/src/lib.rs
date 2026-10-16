//! Independent three-interface fixture; invocation does not reuse the authoring oracle.
use openbindings::{JsonRef, JsonValue, Source};
use openbindings_interface_synthesis as synthesis;
use openbindings_invocation::CancellationToken;
use openbindings_invocation::{BindingRequest, InvocationError, ProviderIo, binding};
use openbindings_source_inspection as inspection;
use std::{
    collections::HashSet,
    future::{Future, poll_fn},
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
};

pub use interface_synthesis_consumer::KIND;

/// Existing authoring fixture plus independent execution counters and target evidence.
#[derive(Default)]
pub struct Catalog {
    pub authoring: interface_synthesis_consumer::Catalog,
    pub attempts: AtomicUsize,
    pub effects: AtomicUsize,
    pub last_target: Mutex<Option<String>>,
    pub last_scope: Mutex<Option<String>>,
}

impl inspection::SourceInspector for Catalog {
    type Inspection<'a> =
        <interface_synthesis_consumer::Catalog as inspection::SourceInspector>::Inspection<'a>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn inspect<'a>(
        &'a self,
        source: &'a Source,
        control: CancellationToken,
    ) -> Self::Inspection<'a> {
        inspection::SourceInspector::inspect(&self.authoring, source, control)
    }
}

impl synthesis::InterfaceSynthesizer for Catalog {
    type Synthesis<'a> =
        <interface_synthesis_consumer::Catalog as synthesis::InterfaceSynthesizer>::Synthesis<'a>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn synthesize<'a>(
        &'a self,
        request: &'a synthesis::SynthesisRequest<'a>,
        control: CancellationToken,
    ) -> Self::Synthesis<'a> {
        synthesis::InterfaceSynthesizer::synthesize(&self.authoring, request, control)
    }
}

/// A real Pending boundary, independent of an executor or detached task.
pub async fn yield_once() {
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
}

struct ExecutionTarget<'a> {
    id: &'a str,
    scope: &'a str,
    disposition: &'a str,
    mode: &'a str,
}

fn field<'a>(
    unit: JsonRef<'a>,
    name: &str,
    default: Option<&'a str>,
) -> Result<&'a str, InvocationError> {
    match unit.get(name) {
        Some(value) => value.as_str().ok_or_else(InvocationError::refused),
        None => default.ok_or_else(InvocationError::refused),
    }
}

// Independent execution reader: validate every literal unit, then select the
// requested (scope,id). It does not consume authoring's analysis or coverage.
fn execution_target<'a>(
    request: BindingRequest<'a>,
) -> Result<ExecutionTarget<'a>, InvocationError> {
    let binding = request
        .binding
        .content
        .as_ref()
        .ok_or_else(InvocationError::refused)?
        .view();
    let (scope, id) = if let Some(id) = binding.as_str() {
        ("target", id)
    } else {
        if binding.members().is_none_or(|m| m.count() != 2) {
            return Err(InvocationError::refused());
        }
        let scope = field(binding, "scope", None)?;
        if !matches!(scope, "alternative" | "projection") {
            return Err(InvocationError::refused());
        }
        (scope, field(binding, "id", None)?)
    };
    if id.is_empty() {
        return Err(InvocationError::refused());
    }
    let content = request
        .source
        .content
        .as_ref()
        .ok_or_else(InvocationError::refused)?;
    if content
        .get("unknownRemainder")
        .is_some_and(|v| v.as_bool().is_none())
    {
        return Err(InvocationError::refused());
    }
    let units = content
        .get("units")
        .and_then(|v| v.elements())
        .ok_or_else(InvocationError::refused)?;
    let mut seen = HashSet::new();
    let mut selected = None;
    for unit in units {
        let unit_id = field(unit, "id", None)?;
        let unit_scope = field(unit, "scope", Some("target"))?;
        let disposition = field(unit, "status", Some("represented"))?;
        let mode = field(unit, "interaction", Some("echo"))?;
        if unit_id.is_empty()
            || !matches!(
                unit_scope,
                "target" | "alternative" | "projection" | "dependency"
            )
            || !matches!(
                disposition,
                "represented" | "excluded" | "invalid" | "lossy" | "implementation-unsupported"
            )
            || !matches!(mode, "echo" | "unary" | "server" | "count" | "empty")
            || !seen.insert((unit_scope, unit_id))
        {
            return Err(InvocationError::refused());
        }
        if unit_scope == scope && unit_id == id {
            selected = Some(ExecutionTarget {
                id: unit_id,
                scope: unit_scope,
                disposition,
                mode,
            });
        }
    }
    let selected = selected.ok_or_else(InvocationError::refused)?;
    if !matches!(selected.disposition, "represented" | "lossy") {
        return Err(InvocationError::refused());
    }
    Ok(selected)
}

impl binding::BindingInvoker for Catalog {
    type Invocation<'a> = Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>;
    fn supports_kind(&self, kind: &str) -> bool {
        kind == KIND
    }
    fn invoke<'a>(
        &'a self,
        request: BindingRequest<'a>,
        mut io: ProviderIo,
        _: CancellationToken,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let ExecutionTarget {
                id, scope, mode, ..
            } = execution_target(request)?;
            *self.last_target.lock().unwrap() = Some(id.into());
            *self.last_scope.lock().unwrap() = Some(scope.into());
            yield_once().await;
            if mode == "empty" {
                io.close_input();
                return Ok(());
            }
            let mut count = 0;
            while let Some(value) = io.next().await {
                count += 1;
                self.effects.fetch_add(1, Ordering::Relaxed);
                match mode {
                    "unary" => {
                        io.close_input();
                        io.emit(value).await?;
                        break;
                    }
                    "server" => {
                        io.close_input();
                        for _ in 0..7 {
                            io.emit(value.clone()).await?;
                        }
                        break;
                    }
                    "count" => {}
                    _ => io.emit(value).await?,
                }
            }
            if mode == "count" {
                io.emit(JsonValue::parse(count.to_string()).unwrap())
                    .await?;
            }
            Ok(())
        })
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-probe"))]
mod wasm;

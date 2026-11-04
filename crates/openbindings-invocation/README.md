# OpenBindings invocation

`BindingClient<P>` runs one binding of an owned conformant document.
`OperationClient<P,E>` adds operation/alias resolution, caller-directed binding
selection, and validation of each declared input/output contract. The SDK chooses
no executor, registry, transport, credential store or retry policy.

This implementation targets the binding-invoker and operation-invoker **0.1**
interfaces at revision `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`, independently of
OpenBindings core **0.2** and the package version.

```rust
use openbindings::{Document, JsonValue};
use openbindings_invocation::{BindingClient, BindingInvoker, BindingRequest,
    CancellationToken, InvocationError, OutputEvent, PreflightOutcome, ProviderIo};
use std::{future::{Future, Ready, ready}, pin::Pin};

struct Echo;
impl BindingInvoker for Echo {
    type Invoke<'a> = Pin<Box<dyn Future<Output=Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<PreflightOutcome, InvocationError>>;
    fn invoke<'a>(&'a self, request: BindingRequest, mut io: ProviderIo,
        _: CancellationToken) -> Self::Invoke<'a> {
        Box::pin(async move {
            // This example's kind echoes values. Real providers read the named
            // binding and its source from request.interface, following their kind.
            let _binding = request.interface.binding(&request.binding).unwrap();
            while let Some(value) = io.next().await { io.send(value).await?; }
            Ok(())
        })
    }
    fn preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Ok(PreflightOutcome::NoPrediction))
    }
}
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let interface = Document::parse(r#"{
  "openbindings":"0.2.0", "operations":{"echo":{}},
  "sources":{"s":{"kind":"example.echo"}},
  "bindings":{"b":{"operation":"echo","source":"s"}}
}"#)?.validate()?.conformant().unwrap();
let client = BindingClient::new(Echo);
let call = client.invoke(BindingRequest::new(interface, "b"), CancellationToken::new())?;
let (mut input, mut output) = call.split();
let sending = async {
    input.send(JsonValue::parse("9007199254740993").unwrap()).await?;
    input.close();
    Ok::<_, openbindings_invocation::WriteError>(())
};
let receiving = async {
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(value) => assert_eq!(value.text(), "9007199254740993"),
            OutputEvent::InputClosed => (),
            OutputEvent::Complete => return Ok(()),
            OutputEvent::Error(error) => return Err(error),
            _ => (),
        }
    }
    Err(InvocationError::transport_closed())
};
let (sent, completed) = tokio::join!(sending, receiving);
completed?;
sent?;
# Ok(())
# }
```

The caller's **output handle alone drives** provider and validation work. Poll
input submission and output consumption concurrently when either can block.
Dropping output cancels the attempt and releases its future; input and control
handles retain no provider future. Closing caller input drains accepted values.
Closing provider input declines further values and may emit `InputClosed` while
output continues. Terminal completion suppresses an undelivered closure notice.
Committed values precede the terminal; an output value alone never establishes
success. Cancellation and failure promise no rollback.

Providers may split `ProviderIo` into independently polled input/output handles.
Both input and output implement bounded handoff, and input also implements
`futures_core::Stream`. Invalid operation input never becomes a clean EOF.
A dropped pending output emission never commits its value. Finite per-poll I/O
credits bound SDK-mediated progress, including directions without schemas;
arbitrary provider CPU remains the provider's cooperative responsibility.

`BindingRequest` and `OperationRequest` own the original `ConformantDocument`,
address and optional opaque `openbindings_context::Context`. Their JSON decoders
admit closed opening payloads and require document conformance. Malformed requests
return local `BoundaryFailure`; well-shaped missing names or keys produce the
appropriate invocation terminal. Exact data preserves absence, null and original
number spelling. A document's opaque data still obeys core document admission.

Operation-name selection uses the provider's optional native `InvocationSupport`
facet. Every ordered selection key must belong to that operation, including keys
after the first usable choice. An explicit binding bypasses support selection.
No preference, deprecation flag, key order or registration order silently chooses
among multiple candidates. This routing facet is not a portable kind-description
operation; a `BindingInvoker` needs no descriptor to run an explicit binding.

`OperationClient` requires an explicit `SchemaEvaluator` and `ExternalSchemas`.
Lazy invocation prepares each declared direction when its first value reaches the
boundary. `prepare` returns a `PreparedOperation` owning concrete validators and
retaining no provider or evaluator. Supply a provider when consuming it into an
attempt, or clone it when its
validators support cloning. Local Rc-based evaluators and validators are supported.
A thread-safe composition retains conditional Send; no global Send/Sync trait
bounds or evaluator erasure are imposed. `ValidationEvidence` distinguishes a
preparation refusal from the original per-value outcome. This evidence stays local;
portable validation errors have code only.

Preflight has its own associated future and returns `NoPrediction` or the shared
`{message, schema}` challenge as a successful result. It selects as invocation does,
but never validates values or dispatches the operation. Ordinary errors carry no
prediction; `ERR_REFUSED` predicts refusal of the same input. A later invocation
selects independently and may discover additional requirements. Context acquisition, approvals and retry policy belong to the host.

`InvocationLimits` bounds queue capacity and value/error encoding. Token-byte
limits do not bound retained parent JSON arenas. Provider code/data errors remain
exact, subject to finite encoding limits and reserved-code admission. Providers
own the no-output/no-operation-effect guarantee of `CONTEXT_REQUIRED`; the SDK
rejects such a challenge after output has already committed.

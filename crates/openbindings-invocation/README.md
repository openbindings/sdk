# OpenBindings invocation

For an application call with schema gates and typed output, read
[document to typed result](https://github.com/openbindings/sdk/blob/main/docs/document-to-typed-result.md).

For a complete protocol implementation walkthrough, read
[writing a binding kind adapter](https://github.com/openbindings/sdk/blob/main/docs/writing-a-kind.md).

`BindingInvoker` runs one binding of an owned conformant document.
`OperationInvoker` resolves operations, selects bindings and validates values.
`DefaultOperationInvoker<P,E>` supplies that implementation using a binding invoker,
an evaluator and external schemas. The SDK chooses
no async runtime, registry, transport, credential store or retry policy.

Implement `handle_invoke` and `handle_preflight`, then import `BindingInvokerExt`
to call the checked `invoke` and `preflight` methods directly on your adapter.
These SDK-owned methods perform admission, cancellation and result checks;
calling a `handle_*` method explicitly bypasses those checks. Custom handoff
limits are arguments to `invoke_with_limits` and `preflight_with_limits`.
Invalid limits return `BoundaryFailureKind::InvalidLimits` before provider work.

This implementation targets the binding-invoker and operation-invoker **0.1.0**
interfaces at revision `484c9399207de99bd56921cead5f44a7b5740d9e`, independently of
OpenBindings core **0.2** and the package version.

```rust
use openbindings::{Document, JsonValue};
use openbindings_invocation::{BindingInvokerExt, BindingInvoker, BindingRequest,
    CancellationToken, ContextChallenge, InvocationError, OutputEvent, ProviderIo};
use std::{future::{Future, Ready, ready}, pin::Pin};

struct Echo;
impl BindingInvoker for Echo {
    type Invoke<'a> = Pin<Box<dyn Future<Output=Result<(), InvocationError>> + Send + 'a>>;
    type Preflight<'a> = Ready<Result<Option<ContextChallenge>, InvocationError>>;
    fn handle_invoke<'a>(&'a self, request: BindingRequest, mut io: ProviderIo,
        _: CancellationToken) -> Self::Invoke<'a> {
        Box::pin(async move {
            // This example's kind echoes values. Real providers read the named
            // binding and its source from request.interface, following their kind.
            let _binding = request.interface.binding(&request.binding).unwrap();
            while let Some(value) = io.next().await { io.send(value).await?; }
            Ok(())
        })
    }
    fn handle_preflight<'a>(&'a self, _: BindingRequest, _: CancellationToken) -> Self::Preflight<'a> {
        ready(Ok(None))
    }
}
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let interface = Document::parse(r#"{
  "openbindings":"0.2.0", "operations":{"echo":{}},
  "sources":{"s":{"kind":"example.echo"}},
  "bindings":{"b":{"operation":"echo","source":"s"}}
}"#)?.validate()?.conformant().unwrap();
let invoker = Echo;
let call = invoker.invoke(BindingRequest::new(interface, "b"), CancellationToken::new())?;
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
handles retain no provider future. `InvocationControl::cancel` settles the
attempt. Its provider future is never polled again and is dropped at the output
handle's next poll or when that handle is dropped. Closing caller input drains accepted values.
Closing provider input declines further values and may emit `InputClosed` while
output continues. Terminal completion suppresses an undelivered closure notice.
Committed values precede the terminal; an output value alone never establishes
success. Cancellation and failure promise no rollback.

Successful input `send` acknowledges queue acceptance. Retain `call.control()`
before splitting, or obtain `output.control()`, to read `undelivered_inputs()`:
`None` while unsettled, then `Some(count)` of accepted inputs never returned to
the provider by a receive operation. The count is stable after completion,
error or cancellation; delivery alone does not establish processing.

Providers may split `ProviderIo` into independently polled input/output handles.
Both input and output implement bounded handoff, and input also implements
`futures_core::Stream`. Invalid operation input never becomes a clean EOF.
A dropped pending output emission never commits its value. Finite per-poll I/O
credits bound SDK-mediated progress, including directions without schemas;
arbitrary provider CPU remains the provider's cooperative responsibility.

`BindingRequest` and `OperationRequest` own the original `ConformantDocument`,
address and optional opaque `openbindings_context::Context`. Their JSON decoders
admit closed opening payloads and require document conformance. Context follows
the shared
[SDK admission policy](https://github.com/openbindings/sdk/blob/main/crates/openbindings-context/README.md):
an object anywhere in it that repeats a decoded member name is refused as
`DuplicateMember` at `/context`. Malformed requests return local
`BoundaryFailure`; well-shaped missing names or keys, including empty operation-invoker
`operation` and `binding` names, produce the appropriate invocation terminal.
Core's OBI-02 name grammar excludes empty keys and aliases.
The binding-invoker carrier requires a nonempty binding name. Exact data preserves absence, null and original
number spelling. A document's opaque data still obeys core document admission.

Operation-name selection uses the provider's optional native `InvocationSupport`
facet. Every ordered selection key must belong to that operation, including keys
after the first usable choice. An explicit binding bypasses support selection.
No preference, deprecation flag, key order or registration order silently chooses
among multiple candidates. This routing facet is not a portable kind-description
operation. Direct `BindingInvokerExt` calls need only `BindingInvoker` and do
not validate operation schemas. `DefaultOperationInvoker` construction,
configuration and `prepare` need only `BindingInvoker`. Its `invoke`, `preflight`
and `OperationInvoker` implementation need `BindingInvoker + InvocationSupport`,
even with an explicit binding; `PreparedOperation::invoke` also requires both traits.

Call `DefaultOperationInvoker::select(&interface, &address)` before loading
source-specific context. It returns the core `openbindings::Binding`, with its
key, source and operation, using only the document, address and the provider's
`InvocationSupport::can_invoke`. It performs no schema work, context access or
provider invocation. Read `binding.source()`, load that source's stored context,
then pass the selected key in `OperationRequest::binding(interface, binding.key())`
with that context to `invoke` or `preflight`. This explicit binding keeps the
later attempt on the source whose context you loaded. Selection checks the whole
ordered list's operation membership before support queries. It does not admit
the address: an empty ordered list or an empty key in that list returns
`ERR_BINDING_NOT_FOUND`, while `invoke` and `preflight` reject those malformed
lists with a `BoundaryFailure`. The `select` method itself needs only
`P: InvocationSupport`.

For a prepared operation, call `prepared.select(&provider, &choice)`, read its
binding's source and load that source's context, then invoke with
`BindingChoice::Binding(binding.key().into())` and that context. Prepared selection
also requires the binding to realize the prepared operation, including for an
explicit key. Its provider needs only `InvocationSupport`. To preflight that
binding, use an explicit `OperationRequest::binding` with the prepared interface
snapshot and the source's context. Selection does not pin a later operation-name
or ordered-list selection.

`prepared.input()` and `prepared.output()` borrow the prepared schemas, with
`None` for an undeclared direction. Use the borrowed input schema to check inline
values before loading context or consuming the prepared operation into a call.

`DefaultOperationInvoker` requires an explicit `SchemaEvaluator` and `ExternalSchemas`.
Lazy invocation prepares each declared direction when its first value reaches the
boundary. `prepare` returns a `PreparedOperation` owning concrete validators and
retaining no provider or evaluator. Supply a provider when consuming it into an
attempt, or clone it when its
validators support cloning. Local Rc-based evaluators and validators are supported.
A thread-safe composition retains conditional Send; no global Send/Sync trait
bounds or evaluator erasure are imposed. `ValidationEvidence` distinguishes a
preparation refusal from the original per-value outcome. This evidence stays local;
portable validation errors have code only.

Applications can route to several binding invokers by implementing the two
`handle_*` methods and forwarding the same `ProviderIo` to the chosen adapter.
Use the child's `handle_*` methods there so a second invocation is not created.
Implement `InvocationSupport` when invoking or preflighting through that router
with `DefaultOperationInvoker`.
The SDK provides no aggregate or registry.

Third parties can implement `OperationInvoker` directly. `Invocation::new` builds
the full lazy lifecycle from limits, cancellation and a closure receiving I/O;
it permits operation-owned error codes. It does not resolve requests or validate
values. The implementation must do so, and can use `OperationRequest::admit` for
the SDK's local address checks. `DefaultOperationInvoker` retains inherent methods
and explicit preparation/validation settings. Generic calls through the
`OperationInvoker` trait do not promise `Send`; concrete return types can be `Send`.

Preflight has its own associated future and returns `None` for no prediction or
`Some(ContextChallenge)` carrying the shared `{message, schema}` challenge. It selects as invocation does,
but never validates values or dispatches the operation. Ordinary errors carry no
prediction; `ERR_REFUSED` predicts refusal of the same input. A later invocation
selects independently and may discover additional requirements. Context acquisition, approvals and retry policy belong to the host.

`InvocationLimits` bounds queue capacity and error encoding. Its optional
`max_value_bytes` defaults to `None`, so invocation adds no size cap on already
parsed values. Set `Some(bytes)` for a delivery policy; `Some(0)` refuses every
value. The check does not bound parsing allocations or retained parent JSON arenas.
JSON parsing retains its independent 32 MiB default byte limit. `InvocationError`
codes are nonempty Rust strings and therefore contain only Unicode scalar values;
decoding a code with an unpaired surrogate returns `BoundaryFailureKind::UnpairedString`.
Unknown codes in that domain and exact error data remain unchanged, subject to
finite encoding limits and reserved-code admission. Providers
own the no-output/no-operation-effect guarantee of `CONTEXT_REQUIRED`; the SDK
rejects such a challenge after output has already committed.

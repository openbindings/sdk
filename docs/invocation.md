# Invocation lifecycle

`BindingClient<P>` runs an explicitly named binding from an owned conformant
interface. `OperationClient<P,E>` also resolves operation keys/aliases, applies the
caller's binding choice, and evaluates values against each declared input/output schema. Both return
an invocation with separate input, output and control handles.

## Call an operation

```rust
use openbindings::{ConformantDocument, ExternalSchemas, JsonValue};
use openbindings_invocation::{BindingInvoker, CancellationToken, InvocationSupport,
    OperationClient, OperationRequest, OutputEvent};
use openbindings_json_schema_evaluator::DefaultEvaluator;

async fn call<P: BindingInvoker + InvocationSupport>(
    interface: ConformantDocument, provider: P, value: JsonValue,
) -> Result<Vec<JsonValue>, Box<dyn std::error::Error>> {
    let client = OperationClient::new(provider, DefaultEvaluator::default(), ExternalSchemas::default());
    let request = OperationRequest::binding(interface, "echo.binding");
    let (mut input, mut output) = client.invoke(request, CancellationToken::new())?.split();
    // A single value fits the default input queue. Streaming writers and readers
    // must be polled concurrently in the application's runtime.
    input.send(value).await?;
    input.close();
    let mut values = Vec::new();
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(value) => values.push(value),
            OutputEvent::InputClosed => (),
            OutputEvent::Complete => return Ok(values),
            OutputEvent::Error(error) => return Err(error.into()),
            _ => (),
        }
    }
    Err("invocation ended without a terminal event".into())
}
```

Requests own the original `ConformantDocument`, address and optional opaque
`openbindings_context::Context`. No source/binding carrier is copied or sliced on
the provider's behalf. A draft must establish conformance before becoming a request;
opaque document data still obeys document admission. Wire decoding rejects malformed
outer fields through local `BoundaryFailure`, separately from invocation terminals.

`OperationRequest::operation(interface, name)` requires a sole invocable candidate.
`ordered(interface, name, keys)` selects the first invocable listed binding, after
checking that **every** key belongs to the operation. Repeated keys are permitted.
`binding(interface, key)` derives the operation and bypasses support selection.
No preference, deprecation flag, key order or registration order silently breaks
ambiguity. The optional native `InvocationSupport` facet supplies routing knowledge;
it is not a portable kind-description query. A binding-only provider needs no facet.

## Driving, closure and cancellation

The output handle alone polls provider work and value validation. Sending input
accepts into a bounded queue; it does not start or advance the provider. Poll
streaming input submission and output consumption concurrently. Dropping a pending
input send does not submit its value. Dropping a pending output read releases that
caller's wake registration while keeping the invocation reusable.

Caller input closure drains previously accepted values. Provider input closure
declines further values and may emit `InputClosed` while output continues. Terminal
settlement suppresses an undelivered closure notice. Invalid operation input never
becomes clean EOF: the driver settles the failure and disposes the provider future.
Providers may split `ProviderIo`; its input has both an async `next` and a Stream
implementation, and its output `send` commits only when that live emission completes.
Dropping an uncommitted emission withdraws it, including after validation accepted it.

Committed outputs remain ordered before the terminal. An output alone never
establishes success. After `Complete` or `Error`, subsequent reads return `None`.
`InvocationControl` observes the winning terminal and local evidence without
retaining or polling provider work. Local cancellation affects only that attempt;
external cancellation synchronously revokes its shared handoffs. Dropping output
cancels and releases its future. None of these operations promises rollback.

`InvocationLimits` bounds input/output queue capacity, value token bytes and
terminal encoding bytes. A token-byte bound does not bound retained parent JSON
arenas. Sixty-four SDK I/O credits per outer output poll bound cooperative progress,
including absent-schema bypass and graceful EOF. Already blocked polls consume no
credits. Arbitrary provider CPU is outside that cooperative I/O bound.

## Schema evaluation and prepared reuse

The evaluator and external schema set are explicit dependencies. Lazy invocation
prepares a declared direction when its first value reaches that boundary, then
reuses that validator within the attempt. An absent schema invokes no evaluator
and makes no validation claim. No whole-graph readiness check is required.

An established mismatch yields `ERR_OPERATION_VALIDATION_FAILED`; preparation
refusal, undefinedness or inability to establish a value verdict yields
`ERR_SCHEMA_UNRESOLVED`. Neither forwards that value. `ValidationEvidence` keeps a
`PreparationFailure` separate from `ValueOutcome`; it never manufactures a value
verdict from preparation. Evidence stays local and is available only for the
winning validation terminal. Error data and context challenges are never checked
against the operation's output schema.

For reuse, call `client.prepare(interface, name, &control)`. The resulting
`PreparedOperation<V>` owns the original document, canonical operation key and
optional validators. It retains no provider, evaluator or client lifetime, and can
outlive the whole originating client. Failed or cancelled preparation releases
partial validators; retry with healthy control and unchanged inputs is supported.

Consume a prepared owner using
`prepared.invoke(&provider, choice, context, control)`. `BindingChoice::Sole`,
`Ordered(keys)` and `Binding(key)` express the remaining caller choice. A foreign
operation's binding fails before provider work. Clone the prepared owner when V
supports cloning; callers decide whether V copies or shares compiled state. Each
attempt owns V, so a Send-but-not-Sync validator can yield a Send future with a Sync
provider and Send provider future. Local Rc evaluators/validators also work.
Prepared contracts can move into spawned host tasks alongside an Arc provider;
no leaked or global invoker is required.

## Context and advisory preflight

Context is an opaque object. The shared challenge is `{message, schema}`, where
schema is an object; message retains exact JSON string semantics. Providers own
kind-specific interpretation and the no-output/no-operation-effect guarantee of
`CONTEXT_REQUIRED`. The SDK rejects that challenge after an output has committed.
Acquisition, approval, credential storage and retry policy belong to the host.

`BindingInvoker` declares separate `Invoke` and `Preflight` future types. A provider
with no prediction returns `Ok(PreflightOutcome::NoPrediction)`; a known requirement
returns `Ok(PreflightOutcome::Challenge(challenge))`. An error carries no prediction.
Both clients synchronously admit preflight requests, then return the advisory
future. Operation preflight selects exactly as invocation does, but never prepares
schemas, consumes values, dispatches the operation or reserves a later choice.
Its future captures no evaluator, so a local evaluator does not prevent a Send
advisory future when the provider supports it.

The portable `InvocationError` carries only code and optional exact data, preserving
absence versus null. Only current interface/kind-owned codes have portable meaning.
Provider-supplied operation-layer resolution/validation codes are rejected at the
provider boundary; `ERR_BINDING_NOT_FOUND` belongs to the binding interface and is
allowed. Local evidence and resource-policy findings never become portable data.

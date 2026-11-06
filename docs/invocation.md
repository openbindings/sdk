# Invocation lifecycle

`BindingInvoker` runs an explicitly named binding from an owned conformant
interface. Import `BindingInvokerExt` to call its checked methods directly.
`OperationInvoker` describes operation invocation; `DefaultOperationInvoker<P,E>`
provides the SDK implementation. It resolves operation keys/aliases, applies the
caller's binding choice, and evaluates values against each declared input/output schema. Both return
an invocation with separate input, output and control handles.

Binding adapters implement `handle_invoke` and `handle_preflight`. The checked
methods belong to a blanket implementation and cannot be overridden. Calling the
public hooks explicitly bypasses those checks. Routers forward hooks to their
selected adapters with the same I/O, so checks and channels are not duplicated.
Custom binding limits are supplied to `invoke_with_limits` or `preflight_with_limits`;
invalid limits are local `BoundaryFailureKind::InvalidLimits` failures.

Third-party `OperationInvoker` implementations can construct an `Invocation` with
`Invocation::new(limits, control, start)`. The closure receives connected `ProviderIo`
and local cancellation only when output is first polled. The constructor handles
streaming lifecycle, not operation resolution or schema validation; those remain
the implementation's responsibility. `OperationRequest::admit` checks address shape
without serialization. The full handle and its input/output/control split are retained.

## Call an operation

```rust
use openbindings::{ConformantDocument, Detail, ExternalSchemas, JsonValue};
use openbindings_invocation::{BindingInvoker, CancellationToken, InvocationSupport,
    DefaultOperationInvoker, OperationRequest, OutputEvent,
    operation::ValidationEvidence};
use openbindings_json_schema_evaluator::DefaultEvaluator;

async fn call<P: BindingInvoker + InvocationSupport>(
    interface: ConformantDocument, provider: P, value: JsonValue,
) -> Result<Vec<JsonValue>, Box<dyn std::error::Error>> {
    let invoker = DefaultOperationInvoker::new(provider, DefaultEvaluator::default(), ExternalSchemas::default())
        .with_validation_detail(Detail::Explain);
    let request = OperationRequest::binding(interface, "echo.binding");
    let (mut input, mut output) = invoker.invoke(request, CancellationToken::new())?.split();
    // A single value fits the default input queue. Streaming writers and readers
    // must be polled concurrently in the application's runtime.
    input.send(value).await?;
    input.close();
    let control = output.control();
    let mut values = Vec::new();
    while let Some(event) = output.next().await {
        match event {
            OutputEvent::Output(value) => values.push(value),
            OutputEvent::InputClosed => (),
            OutputEvent::Complete => {
                // Even success may leave accepted input undelivered.
                println!("undelivered inputs: {}", control.undelivered_inputs().unwrap());
                return Ok(values);
            }
            OutputEvent::Error(error) => {
                if let Some(failure) = control.validation_failure() {
                    println!("validation failed at {}", failure.direction().as_str());
                    match failure.evidence() {
                        ValidationEvidence::Preparation(cause) => eprintln!("{cause}"),
                        ValidationEvidence::Evaluation(outcome) => {
                            if let openbindings::ValueOutcome::Fails { problems, problems_complete, .. } = outcome {
                                println!("{} problems; complete: {problems_complete}", problems.len());
                            }
                        }
                    }
                }
                return Err(error.into());
            }
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

A successful `send` acknowledges acceptance, not delivery to the provider.
Keep `let control = call.control()` before splitting, or use `output.control()`.
After the invocation ends, `control.undelivered_inputs()` returns `Some(count)`
for accepted values never returned by a provider receive, including on `Complete`,
error or cancellation. It returns `None` while the invocation is unsettled.
For example, if two sends succeed and the provider receives only the first
before completing, the count is `Some(1)`. The call example above checks it even
on success. Delivery alone does not establish processing or effects.

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
cancels and releases its future. On cancellation the provider future is dropped
and is never polled again. An adapter cannot await cancellation in that future
and then run async cleanup. Synchronous `Drop` cleanup can release resources;
work requiring async cleanup must be owned separately. The cancellation token is
useful for detached work and for synchronous loops that check it cooperatively.
Checked finite source-pull and kind-description calls use the same drop model.
None of these operations promises rollback.

`InvocationLimits` bounds input/output queue capacity and terminal encoding bytes.
`max_value_bytes` is optional and defaults to `None`: invocation adds no size cap
on already parsed input or output values. Set `Some(bytes)` to apply a delivery
policy, including `Some(0)` to refuse every value. This check does not bound parsing
allocations or retained parent JSON arenas. JSON parsing independently retains its
32 MiB default byte limit. Error encoding checks its byte budget before extending
the serialization buffer; it does not bound allocations already made by the provider.
Sixty-four SDK I/O credits per outer output poll bound cooperative progress,
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

Call `invoker.with_validation_detail(Detail::Explain)` before invoking or preparing
to request problems. The default is `Detail::Outcome`, which reports the verdict
without problems. Retain `call.control()` or `output.control()` and read
`validation_failure()` after a validation terminal. It returns the withheld
`direction()` and `evidence()`: either a preparation failure or the value outcome.
It is `None` when another terminal won, and never adds evidence to portable error
data. Explanation limits and `problems_complete` still apply; see
[reading explanations](schema-evaluators.md#read-an-explanation).

For reuse, call `invoker.prepare(interface, name, &control)`. The resulting
`PreparedOperation<V>` owns the original document, canonical operation key and
optional validators. It retains no provider, evaluator or invoker lifetime, and can
outlive the whole originating invoker. Failed or cancelled preparation releases
partial validators; retry with healthy control and unchanged inputs is supported.

Consume a prepared owner using
`prepared.invoke(&provider, choice, context, control)`. `BindingChoice::Sole`,
`Ordered(keys)` and `Binding(key)` express the remaining caller choice. A foreign
operation's binding fails before provider work. Clone the prepared owner when V
supports cloning; callers decide whether V copies or shares compiled state. Each
attempt owns V, so a Send-but-not-Sync validator can yield a Send future with a Sync
provider and Send provider future. Local Rc evaluators/validators also work.
Prepared schemas can move into spawned host tasks alongside an Arc provider;
no leaked or global invoker is required.

## Keep a stream in a long-lived task

An `Invocation` borrows its invoker. A function cannot create a local invoker
(or local `Arc`) and return that invocation. Move an `Arc` into a task and create
the invocation inside it, so the invoker lives for the whole stream. This Tokio
example submits one request value, closes input, and streams outputs and the
terminal event to `on_event`. Enable Tokio's `rt` feature.

```rust
use std::{error::Error, sync::Arc};
use openbindings::JsonValue;
use openbindings_invocation::{BindingInvoker, CancellationToken,
    DefaultOperationInvoker, InvocationSupport, OperationRequest, OutputEvent};
use openbindings_json_schema_evaluator::DefaultEvaluator;

fn spawn_stream<P>(
    invoker: Arc<DefaultOperationInvoker<P, DefaultEvaluator>>,
    request: OperationRequest,
    value: JsonValue,
    mut on_event: impl FnMut(OutputEvent) + Send + 'static,
) -> tokio::task::JoinHandle<Result<(), Box<dyn Error + Send + Sync>>>
where
    P: BindingInvoker + InvocationSupport + Send + Sync + 'static,
    for<'a> P::Invoke<'a>: Send,
{
    tokio::spawn(async move {
        let (mut input, mut output) = invoker.invoke(request, CancellationToken::new())?.split();
        input.send(value).await?;
        input.close();
        while let Some(event) = output.next().await {
            on_event(event);
        }
        Ok(())
    })
}
```

One value fits the default input queue before output is polled. Keep the returned
task handle to observe admission/send errors and task panics. The
callback should not block; a transport can forward events to its own bounded
channel. Bidirectional streams also need concurrent input submission and output
consumption inside the task. Providers whose futures are not `Send` need a local
task on their runtime instead of `tokio::spawn`.

## Context and advisory preflight

Context is an opaque object. The shared challenge is `{message, schema}`, where
schema is an object; message retains exact JSON string semantics. Providers own
kind-specific interpretation and the no-output/no-operation-effect guarantee of
`CONTEXT_REQUIRED`. The SDK rejects that challenge after an output has committed.
Acquisition, approval, credential storage and retry policy belong to the host.

`BindingInvoker` declares separate `Invoke` and `Preflight` future types. A provider
with no prediction returns `Ok(PreflightOutcome::NoPrediction)`; a known requirement
returns `Ok(PreflightOutcome::Challenge(challenge))`. An error carries no prediction.
Both checked invocation entry points synchronously admit preflight requests, then return the advisory
future. Operation preflight selects exactly as invocation does, but never prepares
schemas, consumes values, dispatches the operation or reserves a later choice.
Its future captures no evaluator, so a local evaluator does not prevent a Send
advisory future when the provider supports it.

The portable `InvocationError` carries only code and optional exact data, preserving
absence versus null. Only current interface/kind-owned codes have portable meaning.
Provider-supplied operation-layer resolution/validation codes are rejected at the
provider boundary; `ERR_BINDING_NOT_FOUND` belongs to the binding interface and is
allowed. Local evidence and resource-policy findings never become portable data.

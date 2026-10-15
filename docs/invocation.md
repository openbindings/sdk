# Invocation lifecycle

## Call an operation in a document

`operation::Invoker` retains a document and an explicit binding provider. It
resolves names and pure support at call construction, then returns the **same
`Invocation`** as the binding entry point. Provider execution remains lazy.

```rust
use openbindings::{JsonValue, Document, ResourceSet};
use openbindings_invocation::{binding::BindingInvoker, operation::{Attempt, Invoker}};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

async fn call<P: BindingInvoker>(document: Document, provider: P)
    -> Result<(), Box<dyn std::error::Error>>
{
    let proof = document.validate()?.conformant().ok_or("document is not conformant")?;
    let invoker = Invoker::new(proof, provider)
        .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default());
    let prepared = invoker.prepare("echo")?; // Keep for further attempts.
    let mut invocation = prepared.invoke(Attempt::new());
    invocation.write(JsonValue::parse("42")?).await?;
    invocation.close();
    while let Some(result) = invocation.next().await {
        let value = result?; // Observe the terminal too, even after receiving a value.
        assert_eq!(value.text(), "42");
    }
    Ok(())
}
```

The optional evaluator package is an explicit application dependency. Without
`.with_evaluator`, absent schemas still forward values; a declared schema yields
`ERR_SCHEMA_UNRESOLVED` on its first actual value. Supplying an evaluator builds
one immutable document/resource context. Plain `invoke` prepares each used
direction lazily once per attempt and retains that contract for its stream; zero
values do not trigger preparation. Fresh attempts prepare again. There is no network retrieval
or whole-graph readiness gate.

For repeated attempts, call `let prepared = invoker.prepare("echo")?` and then
`prepared.invoke(Attempt::new())`. The `operation::PreparedOperation`
handle borrows the invoker and eagerly retains both declared value contracts.
It performs no provider work and pins no binding or context. Each call takes an
`Attempt` with no operation name: `Attempt::new()` requires a sole supported
binding, `Attempt::ordered(&["preferred", "fallback"])` supplies an ordered list,
and `Attempt::binding("exact-key")` selects one binding of the prepared operation.
Use `.with_context(&context)` to supply an opaque object per attempt. Every
ordered key must belong to the operation; an exact binding belonging to another
operation returns `ERR_OPERATION_NOT_FOUND` before provider work. Attempt Debug
output redacts binding choices and context contents. Absent directions need no
evaluator.
Preparation returns `ERR_OPERATION_NOT_FOUND` for an unknown operation, `ERR_REFUSED`
for role metadata it cannot represent, and `ERR_SCHEMA_UNRESOLVED` when a declared
value contract cannot be prepared; each releases partial work. `prepare_with_control`
returns `ERR_CANCELLED` when cancelled, leaving no handle.
A ready contract can still return no verdict or an undefined result for a
particular value. Both stop the affected value with `ERR_SCHEMA_UNRESOLVED`.
`ValidationFailure::outcome()` retains `ValueOutcome::Undefined` separately from
`NoVerdict`, so callers can distinguish an established undefined result from
inability to answer.
`PreparedOperation::invoke_with_control` isolates each attempt's cancellation,
queues and validation evidence, so concurrent attempts can share one handle.

For plain invocation, use `Request::operation("name-or-alias")` when exactly one
supported binding exists, `Request::ordered("name", &["preferred", "fallback"])` for caller policy,
or `Request::binding("exact-key")` to select directly. Every ordered key must
belong to the operation, including entries after the first supported one.
Unsupported automatic/ordered candidates are skipped; an unsupported explicit
binding yields `ERR_BINDING_NOT_FOUND` in invocation and preflight. Metadata and provider registration order never choose a binding.
Use `.with_context(&context)` to forward one opaque object unchanged.

`operation::Invoker::new` requires a `Conformant<Document>`, including when no evaluator
is configured. Invalid documents are rejected before invocation construction;
parsed editor reads still report malformed fields and ambiguous names.
`with_evaluator` is infallible and performs no preparation. A non-scalar source kind
cannot match a provider's scalar kind declaration. Other exact metadata that cannot
fit a role carrier returns `ERR_REFUSED` before dispatch, with a local
`boundary_error()` identifying the field. `preflight` uses the same selection rules
and never prepares value contracts.

Input is validated before delivery to the provider; successful output is validated
before entering the committed output queue. Invalid input **does not become clean
EOF**: the invocation ends and the pending provider read is abandoned. Providers
polling split endpoints independently must also observe their supplied
`CancellationToken` and release those tasks on cancellation. A failed output emission
returns an error. Already committed outputs precede the single terminal failure;
earlier effects are not undone. Application errors and context challenges are
forwarded without applying the successful-output schema.

An established mismatch yields code-only `ERR_OPERATION_VALIDATION_FAILED`;
preparation or evaluation without a verdict yields code-only `ERR_SCHEMA_UNRESOLVED`.
`Invocation`, `Outputs` and `Control` expose `validation_failure()` for the winning
terminal's local evidence: direction, preparation/evaluation stage, and the original
core `ValueOutcome`, including diagnostic completeness. The evidence is never
portable error data. Debug redacts it; custom evaluator messages and locations
require an application disclosure policy. Diagnostic limits come from the chosen
evaluator, independently of invocation's portable-error byte limit.

Carrier caches have slots only for declared source/binding keys; repeated unknown
requests cannot grow them. Used carriers retain exact content and are decoded
once, without serializing or copying a whole document per call. Drop the invoker
to release its cache owners. Caller-retained documents, prepared contracts, and
values can independently retain their own backing storage.

## Ownership and progress

Provider execution is lazy. Polling a write, read or input-closure future drives the
provider job; retaining the handle alone performs no background work. Providers
must obey ordinary Rust future rules: polls return promptly, blocking work is
arranged by the application, and pending work arranges a wakeup. No detached task,
runtime, HTTP client, registry or transport is installed by the companion.
Provider panics remain Rust panics; the SDK does not convert them to portable
invocation errors or promise recovery of the same invocation after unwinding.

There is one caller input owner and one output owner. `split()` consumes the
combined handle once. `Outputs` implements `Stream` and `FusedStream`, as well as
an inherent asynchronous `next`. `ProviderIo::split()` supports independently
polled provider directions. No `Sink` implementation suggests that flush means
remote dispatch acknowledgement.

| Event | Behavior |
| --- | --- |
| `write(value).await` succeeds | Value entered the bounded SDK handoff; it may not have been dispatched. |
| Drop a pending write/read | No input admission/output consumption; its waiter is released. |
| Caller `close()` or input drop | Idempotent half-close; accepted input can drain, output continues. |
| Provider input close/drop | Stop delivery and discard unread input; output can continue. |
| Job returns | First terminal wins; queued outputs precede success or one failure. |
| `control.cancel()` | Settle cancellation, close input and signal/drop cooperative work; queued output remains before terminal. |
| Output owner drop | Abandon active work, release the provider future and output queue. |
| Poll after terminal | Stable exhaustion; `outcome()` remains available. |

A cancellation handle weakly references the driver and does not retain provider
work after output abandonment. Its **Rust lifetime still follows the borrowed
invocation**; copy the owned `outcome()` snapshot to retain a result afterward.
A caller `CancellationToken` may cancel several attempts, but cancelling one invocation
never cancels siblings sharing that external control. Cancellation cannot roll
back effects or prove that detached external work stopped. A scoped cancellation
relay also reaches cooperative synchronous provider/evaluator work during its
current poll. External cancellation observed before or after that poll wins its
returned result; an already settled terminal stays authoritative.

## Exact values and boundaries

`BindingInvocationInput::from_json` checks the closed input envelope, strict
source/binding carriers, duplicate interpreted names and context's object
boundary. `BindingValue` permits omitted operation/source/content fields;
`BindingValue::from(&core_binding)` explicitly preserves both core strings. Reuse
these carriers across calls: there is no per-call source serialization or reparse.

Content, extensions, context internals and application error data retain exact
JSON: numbers beyond binary64, duplicate opaque names, spelling, and explicit
null. Absent content/context/error data remains absent. Opaque data is never
silently converted through a JavaScript number or Rust map.

`InvocationError` has only a nonempty code and optional exact data. Its typed
`context_required()` accessor is available only for a structurally checked
challenge. Open requirement families survive; `config.value` checks its point,
JSON Pointer and optional object-or-Boolean schema without evaluating it.
Requests, carriers and failures redact values in `Debug`/`Display`. Providers
remain responsible for ensuring portable error data is appropriate application
data, not credentials or raw native protocol evidence.

`ERR_REFUSED` and `CONTEXT_REQUIRED` carry no-operation-effects guarantees.
The facade checks carrier admission, support, challenge structure and forbids a
challenge after output; **it cannot prove a provider has not dispatched**.
A provider must uphold that remaining guarantee. An oversized result or a
post-output challenge becomes `ERR_EXECUTION_FAILED`, with a bounded value-free
`boundary_error()` finding. No automatic preflight, context resolution or retry
occurs. Preflight is advisory and separately cancellable; failure does not gate
a later invocation. Preflight requirements belong in `Ok(Some(challenge))`; an
erroneous provider `Err(CONTEXT_REQUIRED)` becomes `ERR_EXECUTION_FAILED`.

## Resource policy

`Invoker::new` starts with **one queued input, four queued outputs, 1 MiB per value
and 64 KiB per encoded error/advisory challenge**. These are native policy defaults,
not role requirements. `Invoker::with_limits` makes a different finite policy
explicit. Value limits are checked before admission; an oversized caller value
is a local `WriteError` and does not redefine an active invocation's terminal.
Oversized provider output terminates unsuccessfully without emitting the value.

In addition to queue slots, there can be one pending caller value, one pending
provider emission, a terminal record and bounded driver/waiter state. Provider
private state and caller-held output are outside these queues. A selected
`JsonValue` subtree retains its entire parent arena: a one-byte `value.bytes()`
can retain a 1 MiB source. These caps are **not a global memory limit**. An explicit
bounded reparse can isolate a snapshot when ownership isolation is required;
this is a copy with a cost, not the default handoff.

## Interface mapping

The shared interfaces define operation meanings; this SDK defines native
ownership, queue and cancellation policy.

| Role concept | Native API / responsibility |
| --- | --- |
| `checkKindSupport`, `listSupportedKinds` | `Invoker::check_kind_support`, `list_supported_kinds`; exact, pure, job-specific. |
| Initial by-value payload | `BindingInvocationInput` / borrowed `BindingRequest`, checked before provider work. |
| `invokeBinding` | `Invoker::invoke` or the explicit external-control variant; one attempt. |
| Input/output frames | Owned native endpoints and terminal sequence; no production frame transport. |
| `input_closed` | `Input::is_closed`, `closed`, `Control::input_is_closed`; nonterminal. |
| `complete` / `error` | Job return, fallible output sequence and owned outcome snapshot. |
| `preflightBinding` | `Invoker::preflight` for no prediction, or `preflight_with_provider` for the optional hook; no operation I/O or dispatch. |
| `invokeOperation`, `preflightOperation` | `operation::Invoker` with `operation::Request`; shared lifecycle and explicit evaluator. |
| Declared schema failures | Code-only portable error plus local `validation_failure()` evidence. |
| Context/effect semantics | Checked structure plus documented provider obligations; no resolver. |
| Source interpretation | Kind adapter; the companion treats content as opaque. |

Provider errors using operation-invoker-owned resolution or validation codes are
refused at the provider boundary as `ERR_EXECUTION_FAILED`, without their data.
Invocation retains a value-free `reserved-provider-code` boundary finding. The
facade's own selection and validation errors retain their reserved codes and
local evidence. `InvocationError` displays its code in both Display and Debug;
application data remains redacted.

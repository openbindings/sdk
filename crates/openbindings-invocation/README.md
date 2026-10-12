# OpenBindings invocation

An optional Rust companion for **binding and operation invocation**: a provider implements one
ordinary asynchronous job; a checked `binding::Invoker` supplies the bounded
input/output lifetime. The same path handles zero-value, unary, server-streaming,
client-streaming and bidirectional interactions.

This crate applies the project's Binding Invoker and Operation Invoker **0.1.0** roles using
OpenBindings **0.2** carriers. It is provisional, separate from the normative
core, and does not add invocation to the published TypeScript SDK.

## Implement and call a provider

```rust
use openbindings::{JsonValue, Source, WorkControl};
use openbindings_invocation::{
    binding::{BindingInvoker, Invoker},
    BindingRequest, BindingValue, InvocationError, ProviderIo,
};
use std::{future::Future, pin::Pin};

struct Echo<'a> { kind: &'a str }
impl BindingInvoker for Echo<'_> {
    type Invocation<'a> =
        Pin<Box<dyn Future<Output = Result<(), InvocationError>> + Send + 'a>>
        where Self: 'a;

    fn supports_kind(&self, kind: &str) -> bool { kind == self.kind }

    fn invoke<'a>(
        &'a self, _request: BindingRequest<'a>, mut io: ProviderIo,
        _control: WorkControl,
    ) -> Self::Invocation<'a> {
        Box::pin(async move {
            while let Some(value) = io.next().await {
                io.emit(value).await?;
            }
            Ok(())
        })
    }
}

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let kind = String::from("urn:example:echo:v1");
let invoker = Invoker::new(Echo { kind: &kind });
let source = Source { kind: kind.clone(), ..Default::default() };
let binding = BindingValue::default(); // A role binding may be {}.
let request = BindingRequest { source: &source, binding: &binding, context: None };
let (mut input, mut output) = invoker.invoke(request).split();

let send = async {
    input.write(JsonValue::parse("9007199254740993").unwrap()).await?;
    input.close();
    Ok::<_, openbindings_invocation::WriteError>(())
};
let receive = async {
    let mut count = 0;
    while let Some(value) = output.next().await {
        let value = value?; // Observe a failure even after earlier output.
        assert_eq!(value.text(), "9007199254740993");
        count += 1;
    }
    assert_eq!(count, 1); // Successful exhaustion is distinct from one JSON null.
    Ok::<_, InvocationError>(())
};
// The SDK needs no executor of its own. Tokio is one caller-side choice.
let (write_result, terminal_result) = tokio::join!(send, receive);
terminal_result?; // Preserve the authoritative output failure if writing closed.
write_result?;
# Ok(())
# }
```

The associated future may borrow configuration and need not be `Send` or
`'static`. A named future avoids boxing; the example boxes only to keep an
asynchronous provider concise. The checked native handle is `Send` when its
provider borrows and future permit it. `&P`, `Arc<P>` and local `Rc<P>` forward
provider behavior. Preflight defaults to no prediction, so an invocation-only
provider needs no preflight implementation.

For a no-input operation, drop or close input and read output through terminal.
For a no-output operation, read to normal exhaustion. For a unary result, check
both cardinality and terminal success; returning the first value can conceal a
later failure. For bidi work, poll both halves concurrently. A fail-fast join can
discard the authoritative output failure when a write first observes closure.

## Call an operation in a document

`operation::Invoker` retains a document and an explicit binding provider. It
resolves names and pure support at call construction, then returns the **same
`Invocation`** as the binding entry point. Provider execution remains lazy.

```rust
use openbindings::{JsonValue, ParsedDocument, ResourceSet};
use openbindings_invocation::{binding::BindingInvoker, operation::{Invoker, Request}};
use openbindings_json_schema_evaluator::DefaultEvaluator;
use std::sync::Arc;

async fn call<P: BindingInvoker>(document: ParsedDocument, provider: P)
    -> Result<(), Box<dyn std::error::Error>>
{
    let invoker = Invoker::new(document, provider)
        .with_evaluator(Arc::new(DefaultEvaluator::new()), ResourceSet::default())?;
    let mut invocation = invoker.invoke(Request::operation("echo"));
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
one immutable document/resource context, with core's bounded preparation cache.
Each used side prepares lazily once per attempt and retains that contract for
its stream; zero values do not trigger preparation. There is no network retrieval
or whole-graph readiness gate.

Use `Request::operation("name-or-alias")` when exactly one supported binding
exists, `Request::ordered("name", &["preferred", "fallback"])` for caller policy,
or `Request::binding("exact-key")` to select directly. Every ordered key must
belong to the operation, including entries after the first supported one.
Unsupported automatic/ordered candidates are skipped; an unsupported explicit
binding refuses. Metadata and provider registration order never choose a binding.
Use `.with_context(&context)` to forward one opaque object unchanged.

The retained snapshot need not have a whole-document conformance certificate.
Core interpretation rules still apply: malformed interpreted structures refuse,
missing or ambiguous operations remain unresolved, and a binding's operation must
be a primary key. Accepting an invocation does not certify document conformance.
Use core assessment separately when application policy requires that certificate.
`preflight` uses the same selection rules and never prepares value contracts.

Input is validated before delivery to the provider; successful output is validated
before entering the committed output queue. Invalid input **does not become clean
EOF**: the invocation ends and the pending provider read is abandoned. Providers
polling split endpoints independently must also observe their supplied
`WorkControl` and release those tasks on cancellation. A failed output emission
returns an error. Already committed outputs precede the single terminal failure;
earlier effects are not undone. Application errors and context challenges are
forwarded without applying the successful-output schema.

An established mismatch yields code-only `ERR_OPERATION_VALIDATION_FAILED`;
preparation or evaluation without a verdict yields code-only `ERR_SCHEMA_UNRESOLVED`.
`Invocation`, `Outputs` and `Control` expose `validation_failure()` for the winning
terminal's local evidence: side, preparation/evaluation stage, and the original
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
executor, HTTP client, registry or transport is installed by the companion.
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
A caller `WorkControl` may cancel several attempts, but cancelling one invocation
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

## Contract mapping and evidence

Pinned authority: interfaces `8b25ddccb477db31d7a507fe07227e08f544679d`;
`APPLIED_ROLE_REVISION` and `tests/fixtures` retain its version and byte hashes.

| Role concept | Native API / responsibility |
| --- | --- |
| `checkKindSupport`, `listSupportedKinds` | `Invoker::check_kind_support`, `list_supported_kinds`; exact, pure, job-specific. |
| Initial by-value payload | `BindingInvocationInput` / borrowed `BindingRequest`, checked before provider work. |
| `invokeBinding` | `Invoker::invoke` or the explicit external-control variant; one attempt. |
| Input/output frames | Owned native endpoints and terminal sequence; no production frame transport. |
| `input_closed` | `Input::is_closed`, `closed`, `Control::input_is_closed`; nonterminal. |
| `complete` / `error` | Job return, fallible output sequence and owned outcome snapshot. |
| `preflightBinding` | `Invoker::preflight`; no-prediction default, no operation I/O or dispatch. |
| `invokeOperation`, `preflightOperation` | `operation::Invoker` with `operation::Request`; shared lifecycle and explicit evaluator. |
| Declared schema failures | Code-only portable error plus local `validation_failure()` evidence. |
| Context/effect semantics | Checked structure plus documented provider obligations; no resolver. |
| Source interpretation | Kind adapter; the companion treats content as opaque. |

Maintained value, lifecycle and frame-correspondence tests cover the role rules.
The frame mapper is test-only and does not establish a new streaming fixture
standard. The external Catalog consumer joins inspection, synthesis and explicit
binding invocation against an independent execution oracle. The invoker-only
consumer guards dependency isolation; the Node/Wasm probe qualifies a thin Rust
bridge, not a released TypeScript API or browser deployment. See the repository's
[binding invocation checkpoint](https://github.com/openbindings/sdk/blob/main/docs/binding-invocation-checkpoint.md)
for the acceptance map and qualification commands.

Context provisioning, protocol transports and the real OpenAPI kind adapter
remain subsequent work. Operation consumer and cancellation tests, the shared
Catalog pipeline, and the expanded Node/Wasm probe qualify the operation layer.
The resource probe records warmed binding-versus-operation overhead and owner
release; it does not claim HTTP parity, browser memory bounds, or arbitrary schema
throughput.
